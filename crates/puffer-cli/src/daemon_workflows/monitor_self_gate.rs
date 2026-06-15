use puffer_config::ConfigPaths;
use puffer_subscriber_runtime::Event;
use puffer_subscriptions::SelfMessageGate;
use serde_json::Value;
use std::sync::Arc;

use super::monitor_task_ignore::monitor_tasks_path;

/// Gate that dispatches self/outgoing events to the monitor triage agent only
/// when the event's `chat_id` belongs to a chat that has at least one OPEN
/// (not completed, not cancelled) monitor task in the monitor task store.
///
/// On any error (missing file, parse failure, missing chat_id) returns `false`
/// (drop), which is the safe #569-preserving default.
pub(crate) struct MonitorSelfGate {
    paths: ConfigPaths,
}

impl MonitorSelfGate {
    pub(crate) fn new(paths: ConfigPaths) -> Self {
        Self { paths }
    }
}

impl SelfMessageGate for MonitorSelfGate {
    fn should_dispatch_self_message(&self, event: &Event) -> bool {
        let Some(chat_id) = chat_id_string(&event.payload) else {
            return false;
        };
        let path = monitor_tasks_path(&self.paths);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return false;
        };
        let Ok(store) = serde_json::from_str::<Value>(&raw) else {
            return false;
        };
        store
            .get("tasks")
            .and_then(Value::as_array)
            .is_some_and(|tasks| {
                tasks.iter().any(|t| {
                    let open = !matches!(
                        t.get("status").and_then(Value::as_str),
                        Some("completed") | Some("cancelled")
                    );
                    open && task_chat_id_string(t).as_deref() == Some(chat_id.as_str())
                })
            })
    }
}

/// Extract `payload["chat_id"]` as a normalised string.
/// Accepts both `Value::String` and `Value::Number`.
fn chat_id_string(payload: &Value) -> Option<String> {
    value_to_string(payload.get("chat_id")?)
}

/// Extract `task["metadata"]["chat_id"]` as a normalised string.
fn task_chat_id_string(task: &Value) -> Option<String> {
    value_to_string(task.get("metadata")?.get("chat_id")?)
}

/// Convert a `&Value` to a canonical string used for chat_id comparison.
/// - `Value::String`: returned if non-empty after trimming.
/// - `Value::Number`: stringified via `to_string()`.
/// - Everything else: `None`.
fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_config::ConfigPaths;
    use serde_json::json;
    use std::fs;

    fn make_event(payload: Value) -> Event {
        Event {
            topic: "telegram-user".into(),
            kind: "message".into(),
            control: false,
            dedup_key: None,
            text: String::new(),
            payload,
        }
    }

    fn write_task_store(paths: &ConfigPaths, store: &Value) {
        let path = monitor_tasks_path(paths);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_string_pretty(store).unwrap()).unwrap();
    }

    #[test]
    fn gate_true_only_when_chat_has_open_monitor_task() {
        let tempdir = tempfile::tempdir().unwrap();
        let paths = ConfigPaths::discover(tempdir.path());
        let gate = Arc::new(MonitorSelfGate::new(paths.clone()));

        // Write store with one open task for chat 42.
        write_task_store(
            &paths,
            &json!({
                "tasks": [{
                    "task_id": "t1",
                    "status": "pending",
                    "metadata": { "_monitor": true, "chat_id": 42 }
                }]
            }),
        );

        let event_42 = make_event(json!({ "chat_id": 42, "is_outgoing": true }));
        let event_99 = make_event(json!({ "chat_id": 99, "is_outgoing": true }));

        // Chat 42 has an open task → dispatch.
        assert!(gate.should_dispatch_self_message(&event_42));
        // Chat 99 has no task → drop.
        assert!(!gate.should_dispatch_self_message(&event_99));

        // Now mark the task completed → should become false for chat 42.
        write_task_store(
            &paths,
            &json!({
                "tasks": [{
                    "task_id": "t1",
                    "status": "completed",
                    "metadata": { "_monitor": true, "chat_id": 42 }
                }]
            }),
        );
        assert!(!gate.should_dispatch_self_message(&event_42));
    }

    #[test]
    fn gate_matches_chat_id_across_number_and_string_representation() {
        let tempdir = tempfile::tempdir().unwrap();
        let paths = ConfigPaths::discover(tempdir.path());
        let gate = Arc::new(MonitorSelfGate::new(paths.clone()));

        // Store has numeric chat_id; event has string "42".
        write_task_store(
            &paths,
            &json!({
                "tasks": [{
                    "task_id": "t2",
                    "status": "pending",
                    "metadata": { "_monitor": true, "chat_id": 42 }
                }]
            }),
        );
        let event_str = make_event(json!({ "chat_id": "42", "is_outgoing": true }));
        assert!(gate.should_dispatch_self_message(&event_str));

        // Reverse: store has string "42"; event has numeric 42.
        write_task_store(
            &paths,
            &json!({
                "tasks": [{
                    "task_id": "t3",
                    "status": "pending",
                    "metadata": { "_monitor": true, "chat_id": "42" }
                }]
            }),
        );
        let event_num = make_event(json!({ "chat_id": 42, "is_outgoing": true }));
        assert!(gate.should_dispatch_self_message(&event_num));
    }

    #[test]
    fn gate_false_when_no_store_or_no_chat_id() {
        let tempdir = tempfile::tempdir().unwrap();
        let paths = ConfigPaths::discover(tempdir.path());
        let gate = Arc::new(MonitorSelfGate::new(paths.clone()));

        // No store file at all → false.
        let event = make_event(json!({ "chat_id": 42, "is_outgoing": true }));
        assert!(!gate.should_dispatch_self_message(&event));

        // Write a valid store so the file exists, but event has no chat_id → false.
        write_task_store(
            &paths,
            &json!({
                "tasks": [{
                    "task_id": "t4",
                    "status": "pending",
                    "metadata": { "_monitor": true, "chat_id": 42 }
                }]
            }),
        );
        let event_no_chat_id = make_event(json!({ "is_outgoing": true }));
        assert!(!gate.should_dispatch_self_message(&event_no_chat_id));
    }
}
