//! Process-wide manager that owns the bus, store, supervised subscribers
//! and the router. Workflow tools reach into the manager via a global
//! `OnceLock` set up at puffer startup.

use crate::action::{ActionDispatcher, BuiltinActionDispatcher};
use crate::classify::{Classifier, NullClassifier};
use crate::router::SubscriptionRouter;
use crate::store::SubscriptionStore;
use anyhow::Result;
use puffer_subscriber_runtime::{
    EventBus, EventEnvelope, Manifest, SubscriberCommand, SubscriberHandle, SubscriberSupervisor,
    SupervisorConfig,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;

/// Builder for [`SubscriptionManager`]. Lets callers swap in custom
/// dispatcher / classifier implementations (e.g. a real LLM-backed
/// classifier) before construction.
pub struct SubscriptionManagerBuilder {
    bus: EventBus,
    store_path: PathBuf,
    dispatcher: Arc<dyn ActionDispatcher>,
    classifier: Arc<dyn Classifier>,
}

impl SubscriptionManagerBuilder {
    /// Starts a builder with the default dispatcher and classifier and
    /// the supplied store path. The bus is freshly constructed.
    pub fn new(store_path: impl Into<PathBuf>) -> Self {
        Self {
            bus: EventBus::new(),
            store_path: store_path.into(),
            dispatcher: Arc::new(BuiltinActionDispatcher::new()),
            classifier: Arc::new(NullClassifier),
        }
    }

    /// Override the bus (useful for tests).
    pub fn with_bus(mut self, bus: EventBus) -> Self {
        self.bus = bus;
        self
    }

    /// Override the action dispatcher.
    pub fn with_dispatcher(mut self, dispatcher: Arc<dyn ActionDispatcher>) -> Self {
        self.dispatcher = dispatcher;
        self
    }

    /// Override the classifier.
    pub fn with_classifier(mut self, classifier: Arc<dyn Classifier>) -> Self {
        self.classifier = classifier;
        self
    }

    /// Loads the store and spawns the router on the supplied Tokio runtime.
    pub fn build(self, handle: Handle) -> Result<SubscriptionManager> {
        let store = Arc::new(SubscriptionStore::load(&self.store_path)?);
        let dispatcher = self.dispatcher.clone();
        let classifier = self.classifier.clone();
        let bus = self.bus.clone();
        let store_for_router = store.clone();
        let router = handle.block_on(async move {
            SubscriptionRouter::spawn(bus, store_for_router, dispatcher, classifier)
        });
        Ok(SubscriptionManager {
            handle,
            bus: self.bus,
            store,
            router: Mutex::new(Some(router)),
            subscribers: Mutex::new(HashMap::new()),
        })
    }
}

/// Process-wide subscription manager. Owns the bus, store, router task,
/// and the set of supervised subscriber children.
pub struct SubscriptionManager {
    handle: Handle,
    bus: EventBus,
    store: Arc<SubscriptionStore>,
    router: Mutex<Option<SubscriptionRouter>>,
    subscribers: Mutex<HashMap<String, SubscriberHandle>>,
}

impl SubscriptionManager {
    /// Returns the underlying spec store.
    pub fn store(&self) -> Arc<SubscriptionStore> {
        self.store.clone()
    }

    /// Returns a handle on the event bus (used by tests and by future
    /// "live tail" UI).
    pub fn bus(&self) -> EventBus {
        self.bus.clone()
    }

    /// Spawns a subscriber from a manifest directory. Returns the
    /// subscriber's id. If a subscriber with that id is already running,
    /// returns `Ok(id)` without spawning a duplicate.
    pub fn start_subscriber(&self, manifest: Manifest) -> Result<String> {
        let id = manifest.spec.id.clone();
        let mut guard = self.subscribers.lock().unwrap();
        if guard.contains_key(&id) {
            return Ok(id);
        }
        let bus = self.bus.clone();
        let handle = self.handle.block_on(async move {
            SubscriberSupervisor::spawn(manifest, bus, SupervisorConfig::default()).await
        })?;
        guard.insert(id.clone(), handle);
        Ok(id)
    }

    /// Sends a control command to the named subscriber. Returns an error
    /// when the subscriber is unknown or not running.
    pub fn send_command(&self, subscriber_id: &str, command: &SubscriberCommand) -> Result<()> {
        let sender = {
            let guard = self.subscribers.lock().unwrap();
            guard
                .get(subscriber_id)
                .ok_or_else(|| anyhow::anyhow!("subscriber `{subscriber_id}` is not running"))?
                .commands
                .clone()
        };
        self.handle
            .block_on(async move { sender.send(command).await })
    }

    /// Sends a control command and waits for the next matching event kind
    /// on `topic`.
    pub fn send_command_and_wait(
        &self,
        subscriber_id: &str,
        topic: &str,
        command: &SubscriberCommand,
        terminal_kinds: &[&str],
        timeout: std::time::Duration,
    ) -> Result<EventEnvelope> {
        let sender = {
            let guard = self.subscribers.lock().unwrap();
            guard
                .get(subscriber_id)
                .ok_or_else(|| anyhow::anyhow!("subscriber `{subscriber_id}` is not running"))?
                .commands
                .clone()
        };
        let mut rx = self.bus.subscribe_topic(topic);
        let terminal_kinds: Vec<String> =
            terminal_kinds.iter().map(|kind| kind.to_string()).collect();
        self.handle.block_on(async move {
            sender.send(command).await?;
            let deadline = tokio::time::Instant::now() + timeout;
            loop {
                let remaining = deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .ok_or_else(|| anyhow::anyhow!("timed out waiting for subscriber event"))?;
                let envelope = tokio::time::timeout(remaining, rx.recv())
                    .await
                    .map_err(|_| anyhow::anyhow!("timed out waiting for subscriber event"))?
                    .ok_or_else(|| anyhow::anyhow!("subscriber event bus closed"))?;
                if terminal_kinds
                    .iter()
                    .any(|kind| kind == &envelope.event.kind)
                {
                    return Ok(envelope);
                }
            }
        })
    }

    /// Returns the ids of currently supervised subscribers.
    pub fn subscriber_ids(&self) -> Vec<String> {
        self.subscribers.lock().unwrap().keys().cloned().collect()
    }

    /// Shuts down router and every supervised subscriber. Best-effort.
    pub fn shutdown(&self) {
        if let Some(router) = self.router.lock().unwrap().take() {
            self.handle.block_on(async move { router.shutdown().await });
        }
        let handles: Vec<_> = self
            .subscribers
            .lock()
            .unwrap()
            .drain()
            .map(|(_, h)| h)
            .collect();
        for handle in handles {
            self.handle.block_on(async move { handle.shutdown().await });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_subscriber_runtime::SubscriberCommand;
    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn start_subscriber_allows_immediate_control_command() {
        let temp = tempdir().unwrap();
        let subscriber_dir = temp.path().join("subscriber");
        std::fs::create_dir_all(&subscriber_dir).unwrap();
        std::fs::write(
            subscriber_dir.join("manifest.toml"),
            r#"manifest_version = 1
id = "test-subscriber"
kind = "subscriber"
topic = "test-topic"

[run]
cmd = ["sh", "run.sh"]
"#,
        )
        .unwrap();
        std::fs::write(
            subscriber_dir.join("run.sh"),
            r#"IFS= read -r _line || exit 0
printf '%s\n' '{"topic":"test-topic","kind":"message","text":"ready"}'
"#,
        )
        .unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(1)
            .build()
            .unwrap();
        let manager = SubscriptionManagerBuilder::new(temp.path().join("subscriptions.json"))
            .build(runtime.handle().clone())
            .unwrap();
        let mut rx = manager.bus().subscribe_topic("test-topic");
        let manifest = Manifest::load(&subscriber_dir).unwrap();

        manager.start_subscriber(manifest).unwrap();
        manager
            .send_command(
                "test-subscriber",
                &SubscriberCommand::Custom {
                    op: "ping".into(),
                    args: Value::Null,
                },
            )
            .unwrap();

        let envelope = runtime
            .block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await
            })
            .unwrap()
            .unwrap();
        assert_eq!(envelope.subscriber_id, "test-subscriber");
        assert_eq!(envelope.event.text, "ready");

        manager.shutdown();
    }

    #[test]
    fn start_subscriber_passes_absolute_state_dir() {
        let temp = tempdir().unwrap();
        let subscriber_dir = temp.path().join("subscriber");
        std::fs::create_dir_all(&subscriber_dir).unwrap();
        std::fs::write(
            subscriber_dir.join("manifest.toml"),
            r#"manifest_version = 1
id = "state-subscriber"
kind = "subscriber"
topic = "state-topic"

[run]
cmd = ["sh", "run.sh"]

[state]
dir = "state"
"#,
        )
        .unwrap();
        std::fs::write(
            subscriber_dir.join("run.sh"),
            r#"printf '{"topic":"state-topic","kind":"state","text":"%s"}\n' "$PUFFER_SKILL_STATE_DIR"
"#,
        )
        .unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(1)
            .build()
            .unwrap();
        let manager = SubscriptionManagerBuilder::new(temp.path().join("subscriptions.json"))
            .build(runtime.handle().clone())
            .unwrap();
        let mut rx = manager.bus().subscribe_topic("state-topic");
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        let manifest = Manifest::load("subscriber").unwrap();

        manager.start_subscriber(manifest).unwrap();
        std::env::set_current_dir(original_cwd).unwrap();

        let envelope = runtime
            .block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await
            })
            .unwrap()
            .unwrap();
        assert!(
            std::path::Path::new(&envelope.event.text).is_absolute(),
            "state dir should be absolute, got {}",
            envelope.event.text
        );

        manager.shutdown();
    }

    #[test]
    fn send_command_and_wait_returns_terminal_event() {
        let temp = tempdir().unwrap();
        let subscriber_dir = temp.path().join("subscriber");
        std::fs::create_dir_all(&subscriber_dir).unwrap();
        std::fs::write(
            subscriber_dir.join("manifest.toml"),
            r#"manifest_version = 1
id = "wait-subscriber"
kind = "subscriber"
topic = "wait-topic"

[run]
cmd = ["sh", "run.sh"]
"#,
        )
        .unwrap();
        std::fs::write(
            subscriber_dir.join("run.sh"),
            r#"IFS= read -r _line || exit 0
printf '%s\n' '{"topic":"wait-topic","kind":"ignored","text":"first"}'
printf '%s\n' '{"topic":"wait-topic","kind":"login_error","text":"terminal","payload":{"error":"boom"}}'
"#,
        )
        .unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(1)
            .build()
            .unwrap();
        let manager = SubscriptionManagerBuilder::new(temp.path().join("subscriptions.json"))
            .build(runtime.handle().clone())
            .unwrap();
        let manifest = Manifest::load(&subscriber_dir).unwrap();

        manager.start_subscriber(manifest).unwrap();
        let envelope = manager
            .send_command_and_wait(
                "wait-subscriber",
                "wait-topic",
                &SubscriberCommand::Custom {
                    op: "ping".into(),
                    args: Value::Null,
                },
                &["login_awaiting_code", "login_error"],
                std::time::Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(envelope.event.kind, "login_error");
        assert_eq!(envelope.event.payload["error"], "boom");

        manager.shutdown();
    }
}
