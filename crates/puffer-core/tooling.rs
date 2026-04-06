use anyhow::Result;
use puffer_resources::LoadedResources;
use puffer_session_store::SessionStore;
use puffer_tools::ToolRegistry;

use crate::AppState;

pub(crate) fn list_tools(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    let registry = ToolRegistry::from_resources(resources);
    if registry.tools().next().is_none() {
        return super::emit_system(
            state,
            session_store,
            "No executable tools are registered.".to_string(),
        );
    }
    let mut text = String::from("Tools:\n");
    for tool in registry.tools() {
        let approval = tool
            .spec
            .policy
            .approval_policy
            .as_deref()
            .unwrap_or("unspecified");
        let sandbox = tool
            .spec
            .policy
            .sandbox_policy
            .as_deref()
            .unwrap_or("unspecified");
        text.push_str(&format!(
            "- {} ({}) approval={} sandbox={}\n",
            tool.spec.name, tool.spec.handler, approval, sandbox
        ));
    }
    super::emit_system(state, session_store, text)
}

pub(crate) fn describe_permissions(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    let registry = ToolRegistry::from_resources(resources);
    let mut text = String::from("Tool permission policies:\n");
    for tool in registry.tools() {
        text.push_str(&format!(
            "- {}: approval={} sandbox={}\n",
            tool.spec.name,
            tool.spec
                .policy
                .approval_policy
                .as_deref()
                .unwrap_or("unspecified"),
            tool.spec
                .policy
                .sandbox_policy
                .as_deref()
                .unwrap_or("unspecified")
        ));
    }
    super::emit_system(state, session_store, text)
}

pub(crate) fn describe_hooks(state: &mut AppState, session_store: &SessionStore) -> Result<()> {
    super::emit_system(
        state,
        session_store,
        "Hook runtime is not implemented yet. Current tool events are declarative only."
            .to_string(),
    )
}

pub(crate) fn describe_tasks(state: &mut AppState, session_store: &SessionStore) -> Result<()> {
    super::emit_system(
        state,
        session_store,
        "Background task management is not implemented yet. No queued tasks are active."
            .to_string(),
    )
}
