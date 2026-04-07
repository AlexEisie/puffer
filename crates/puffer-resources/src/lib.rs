mod loader;
mod model;

use std::collections::BTreeSet;

pub use loader::{
    agent_by_id, hook_by_id, load_resources, plugin_by_id, plugin_mcp_servers, prompt_by_id,
    skill_by_name,
};
pub use model::{
    AgentSpec, HookSpec, IdeSpec, LoadedItem, LoadedResources, MascotSpec, McpServerSpec,
    PluginCommandSpec, PluginSpec, PromptTemplate, PromptVariableSpec, ProviderPack, SkillSpec,
    SourceInfo, SourceKind, ToolDisplaySpec, ToolMetadataSpec, ToolSpec,
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

/// Renders a prompt template by id, including any chained parent prompts.
pub fn render_prompt_by_id(
    resources: &LoadedResources,
    id: &str,
    variables: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let mut visited = BTreeSet::new();
    let mut sections = Vec::new();
    append_prompt_sections(resources, id, variables, &mut visited, &mut sections);
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

fn append_prompt_sections(
    resources: &LoadedResources,
    id: &str,
    variables: &std::collections::BTreeMap<String, String>,
    visited: &mut BTreeSet<String>,
    sections: &mut Vec<String>,
) {
    if !visited.insert(id.to_string()) {
        return;
    }
    let Some(prompt) = prompt_by_id(resources, id) else {
        return;
    };
    for chained in &prompt.value.chained_from {
        append_prompt_sections(resources, chained, variables, visited, sections);
    }
    sections.push(prompt.value.render(variables));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoadedItem, PromptTemplate, SourceInfo, SourceKind};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn render_prompt_by_id_includes_chained_templates_and_variables() {
        let resources = LoadedResources {
            prompts: vec![
                LoadedItem {
                    value: PromptTemplate {
                        id: "base".to_string(),
                        description: "Base".to_string(),
                        template: "Base $ARGUMENTS".to_string(),
                        variables: vec![PromptVariableSpec {
                            name: "ARGUMENTS".to_string(),
                            description: String::new(),
                            required: false,
                            default: None,
                        }],
                        provider_override: None,
                        model_override: None,
                        mode: None,
                        chained_from: Vec::new(),
                    },
                    source_info: SourceInfo {
                        path: PathBuf::from("base.yaml"),
                        kind: SourceKind::Builtin,
                    },
                },
                LoadedItem {
                    value: PromptTemplate {
                        id: "review".to_string(),
                        description: "Review".to_string(),
                        template: "Review $CWD".to_string(),
                        variables: vec![PromptVariableSpec {
                            name: "CWD".to_string(),
                            description: String::new(),
                            required: false,
                            default: None,
                        }],
                        provider_override: None,
                        model_override: None,
                        mode: Some("review".to_string()),
                        chained_from: vec!["base".to_string()],
                    },
                    source_info: SourceInfo {
                        path: PathBuf::from("review.yaml"),
                        kind: SourceKind::Builtin,
                    },
                },
            ],
            ..LoadedResources::default()
        };
        let rendered = render_prompt_by_id(
            &resources,
            "review",
            &BTreeMap::from([
                ("ARGUMENTS".to_string(), "now".to_string()),
                ("CWD".to_string(), "/tmp/work".to_string()),
            ]),
        )
        .expect("rendered prompt");
        assert!(rendered.contains("Base now"));
        assert!(rendered.contains("Review /tmp/work"));
    }
}
