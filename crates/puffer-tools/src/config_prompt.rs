use puffer_config::{
    supported_config_settings, ConfigSettingScope, ConfigSettingSpec, ConfigSettingValueKind,
};
use puffer_resources::LoadedResources;
use std::collections::BTreeMap;

/// Renders the dynamic Config tool description from the shared settings catalog.
pub(crate) fn render_config_tool_description(resources: &LoadedResources) -> String {
    let mut text = String::from(
        "Get or set Claude Code configuration settings.\n\n\
View or change Puffer Code settings. Use when the user requests configuration changes, asks about current settings, or when adjusting a setting would benefit them.\n\n\
## Usage\n\
- **Get current value:** Omit the \"value\" parameter\n\
- **Set new value:** Include the \"value\" parameter\n\n\
## Configurable settings list\n\
The following settings are available for you to change:\n\n",
    );

    append_settings_section(
        &mut text,
        "User Settings (stored in ~/.puffer/config.toml)",
        ConfigSettingScope::User,
    );
    text.push('\n');
    append_settings_section(
        &mut text,
        "Workspace Settings (stored in .puffer/config.toml)",
        ConfigSettingScope::Workspace,
    );
    text.push('\n');
    append_settings_section(
        &mut text,
        "Session Settings (apply to the current session only)",
        ConfigSettingScope::Session,
    );
    text.push('\n');
    text.push_str(&render_model_section(resources));
    text.push_str(
        "## Examples\n\
- Get theme: { \"setting\": \"theme\" }\n\
- Set theme: { \"setting\": \"theme\", \"value\": \"harbor\" }\n\
- Enable vim mode: { \"setting\": \"editorMode\", \"value\": \"vim\" }\n\
- Change model: { \"setting\": \"model\", \"value\": \"openai/gpt-5\" }\n\
- Set copyFullResponse: { \"setting\": \"copyFullResponse\", \"value\": true }\n\
- Set OpenAI headers: { \"setting\": \"openaiHeaders\", \"value\": { \"x-test\": \"one\" } }\n\
- Set status line padding: { \"setting\": \"statusLinePadding\", \"value\": 2 }\n",
    );
    text
}

fn append_settings_section(text: &mut String, heading: &str, scope: ConfigSettingScope) {
    text.push_str("### ");
    text.push_str(heading);
    text.push('\n');
    for spec in supported_config_settings()
        .iter()
        .filter(|spec| spec.scope == scope)
    {
        text.push_str(&render_setting_line(spec));
        text.push('\n');
    }
}

fn render_setting_line(spec: &ConfigSettingSpec) -> String {
    let mut line = format!("- {}", spec.canonical_key);
    let value_hint = render_value_hint(spec);
    if !value_hint.is_empty() {
        line.push_str(": ");
        line.push_str(&value_hint);
    }
    line.push_str(" - ");
    line.push_str(spec.description);
    if !spec.aliases.is_empty() {
        line.push_str(" Aliases: ");
        line.push_str(&spec.aliases.join(", "));
        line.push('.');
    }
    if matches!(
        spec.value_kind,
        ConfigSettingValueKind::NullableString
            | ConfigSettingValueKind::StringMap
            | ConfigSettingValueKind::NullableUnsignedInteger
    ) {
        line.push_str(" Use null to clear.");
    }
    line
}

fn render_value_hint(spec: &ConfigSettingSpec) -> String {
    if !spec.options.is_empty() {
        return spec
            .options
            .iter()
            .map(|option| format!("\"{option}\""))
            .collect::<Vec<_>>()
            .join(", ");
    }
    match spec.value_kind {
        ConfigSettingValueKind::Boolean => "true/false".to_string(),
        ConfigSettingValueKind::StringMap => "{\"key\":\"value\"}".to_string(),
        ConfigSettingValueKind::NullableUnsignedInteger => "integer".to_string(),
        ConfigSettingValueKind::String | ConfigSettingValueKind::NullableString => String::new(),
    }
}

fn render_model_section(resources: &LoadedResources) -> String {
    let mut models = BTreeMap::new();
    for provider in &resources.providers {
        for model in &provider.value.models {
            models.insert(
                format!("{}/{}", model.provider, model.id),
                model.display_name.clone(),
            );
        }
    }

    if models.is_empty() {
        return "## Model\n- model - Override the active model selector (use a provider/model string or null to clear)\n\n".to_string();
    }

    let mut text =
        "## Model\n- model - Override the active model selector. Available options:\n  - null/\"default\": Clear the model override\n".to_string();
    for (selector, description) in models {
        text.push_str(&format!("  - \"{selector}\": {description}\n"));
    }
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_resources::{LoadedItem, LoadedResources, ProviderPack, SourceInfo, SourceKind};
    use std::path::PathBuf;

    fn provider(id: &str, display_name: &str, models_yaml: &str) -> LoadedItem<ProviderPack> {
        LoadedItem {
            value: serde_yaml::from_str::<ProviderPack>(&format!(
                "id: {id}\n\
display_name: {display_name}\n\
base_url: https://{id}.example.invalid\n\
default_api: openai-responses\n\
auth_modes:\n\
  - api_key\n\
models:\n{models_yaml}"
            ))
            .expect("parse provider"),
            source_info: SourceInfo {
                path: PathBuf::from(format!("{id}.yaml")),
                kind: SourceKind::Builtin,
            },
        }
    }

    #[test]
    fn config_prompt_lists_user_workspace_and_session_settings() {
        let resources = LoadedResources::default();
        let rendered = render_config_tool_description(&resources);
        assert!(rendered.contains("### User Settings"));
        assert!(rendered.contains("### Workspace Settings"));
        assert!(rendered.contains("### Session Settings"));
        assert!(rendered.contains("- copy_full_response: true/false"));
        assert!(rendered.contains("- openai_headers: {\"key\":\"value\"}"));
        assert!(rendered.contains("- statuslineEnabled: true/false"));
    }

    #[test]
    fn config_prompt_lists_available_provider_models() {
        let resources = LoadedResources {
            providers: vec![
                provider(
                    "anthropic",
                    "Anthropic",
                    "  - provider: anthropic\n    id: claude-sonnet-4-5\n    display_name: Claude Sonnet 4.5\n    api: anthropic-messages\n    context_window: 200000\n    max_output_tokens: 8192\n    supports_reasoning: true\n",
                ),
                provider(
                    "openai",
                    "OpenAI",
                    "  - provider: openai\n    id: gpt-5\n    display_name: GPT-5\n    api: openai-responses\n    context_window: 272000\n    max_output_tokens: 16384\n    supports_reasoning: true\n",
                ),
            ],
            ..LoadedResources::default()
        };
        let rendered = render_config_tool_description(&resources);
        assert!(rendered.contains("\"anthropic/claude-sonnet-4-5\": Claude Sonnet 4.5"));
        assert!(rendered.contains("\"openai/gpt-5\": GPT-5"));
    }
}
