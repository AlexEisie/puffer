use puffer_resources::{LoadedResources, PluginSpec, SourceKind};
use std::fmt::Write as _;

const DISABLED_PLUGIN_PLACEHOLDER_PREFIX: &str =
    "Disabled plugin placeholder created by `puffer plugin disable`.";

/// Returns the user-facing `/plugin` help text.
pub(super) fn plugin_help_text() -> String {
    "Usage: /plugin [show|manage|help|list|marketplace|marketplace list|marketplace add <path|url>|marketplace remove <name>|marketplace update [name]|install <id|id@marketplace|path>|uninstall <id>|update <id|id@marketplace>|errors|validate [id|path]|path|open [id]|edit [id]|enable <id>|disable <id>|reload]".to_string()
}

/// Renders the builtin-only plugin marketplace summary.
pub(super) fn render_builtin_plugin_marketplace(resources: &LoadedResources) -> String {
    let mut plugins = resources
        .plugins
        .iter()
        .filter(|plugin| {
            plugin.source_info.kind == SourceKind::Builtin
                && !is_disabled_placeholder(&plugin.value)
        })
        .collect::<Vec<_>>();
    plugins.sort_by(|left, right| left.value.id.cmp(&right.value.id));
    if plugins.is_empty() {
        return "Plugin marketplace\nplugins=0\nNo builtin plugins are currently available."
            .to_string();
    }

    let mut text = format!(
        "Plugin marketplace\nplugins={}\nUse `/plugin install <id>` to install an editable copy into this workspace.\n",
        plugins.len()
    );
    for plugin in plugins {
        let _ = writeln!(
            &mut text,
            "\n- {} [{}] {} • {}",
            plugin.value.id,
            source_kind_label(plugin.source_info.kind),
            plugin_description(&plugin.value),
            format_plugin_counts(&plugin.value)
        );
    }
    text.trim_end().to_string()
}

/// Returns the enabled or disabled status label for one plugin.
pub(super) fn plugin_status(plugin: &PluginSpec) -> &'static str {
    if is_disabled_placeholder(plugin) {
        "disabled"
    } else {
        "enabled"
    }
}

/// Returns the display description for one plugin.
pub(super) fn plugin_description(plugin: &PluginSpec) -> String {
    plugin
        .description
        .strip_prefix(DISABLED_PLUGIN_PLACEHOLDER_PREFIX)
        .map(str::trim)
        .and_then(|value| value.strip_prefix("Original description:").map(str::trim))
        .unwrap_or(plugin.description.as_str())
        .to_string()
}

/// Formats the command, skill, agent, MCP, and LSP counts for one plugin.
pub(super) fn format_plugin_counts(plugin: &PluginSpec) -> String {
    format!(
        "commands={} skills={} agents={} mcp_servers={} lsp_servers={}",
        plugin.commands.len(),
        plugin.skills.len(),
        plugin.agents.len(),
        plugin.mcp_servers.len(),
        plugin.lsp_servers.len()
    )
}

/// Returns true when the plugin manifest is a disabled placeholder.
pub(super) fn is_disabled_placeholder(plugin: &PluginSpec) -> bool {
    plugin
        .description
        .starts_with(DISABLED_PLUGIN_PLACEHOLDER_PREFIX)
}

/// Builds a disabled placeholder manifest for one plugin.
pub(super) fn disabled_placeholder_for(plugin: &PluginSpec) -> PluginSpec {
    PluginSpec {
        id: plugin.id.clone(),
        display_name: plugin.display_name.clone(),
        description: format!(
            "{DISABLED_PLUGIN_PLACEHOLDER_PREFIX} Original description: {}",
            plugin_description(plugin)
        ),
        commands: Vec::new(),
        skills: Vec::new(),
        agents: Vec::new(),
        mcp_servers: Vec::new(),
        lsp_servers: Vec::new(),
    }
}

/// Returns the default workspace plugin manifest contents.
pub(super) fn default_plugin_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace Plugin\n\
description: Customize plugin commands for this workspace.\n"
}

/// Returns the short label for one plugin source kind.
pub(crate) fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Builtin => "builtin",
        SourceKind::User => "user",
        SourceKind::Workspace => "workspace",
    }
}
