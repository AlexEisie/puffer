mod loader;
mod model;

pub use loader::{load_resources, plugin_by_id, plugin_mcp_servers, prompt_by_id, skill_by_name};
pub use model::{
    HookSpec, IdeSpec, LoadedItem, LoadedResources, MascotSpec, McpServerSpec, PluginCommandSpec,
    PluginSpec, PromptTemplate, ProviderPack, SkillSpec, SourceInfo, SourceKind, ToolSpec,
};

/// Looks up a mascot by id.
pub fn mascot_by_id<'a>(resources: &'a LoadedResources, id: &str) -> Option<&'a MascotSpec> {
    resources
        .mascots
        .iter()
        .find(|mascot| mascot.value.id == id)
        .map(|mascot| &mascot.value)
}

/// Returns all loaded hooks matching the requested event name.
pub fn hooks_for_event<'a>(
    resources: &'a LoadedResources,
    event: &str,
) -> Vec<&'a LoadedItem<HookSpec>> {
    resources
        .hooks
        .iter()
        .filter(|hook| hook.value.event == event)
        .collect()
}
