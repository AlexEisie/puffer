use super::emit_system;
use crate::AppState;
use anyhow::Result;
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_resources::{
    load_resources, plugin_by_id, plugin_mcp_servers, LoadedResources, SourceKind,
};
use puffer_session_store::SessionStore;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Describes loaded plugin metadata or a specific plugin manifest.
pub(crate) fn describe_plugin(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    if args.is_empty() {
        if resources.plugins.is_empty() {
            return emit_system(
                state,
                session_store,
                "No plugins are installed.".to_string(),
            );
        }
        let mut text = String::from("Plugins:\n");
        for plugin in &resources.plugins {
            let _ = writeln!(
                &mut text,
                "{} - {}",
                plugin.value.id, plugin.value.description
            );
        }
        return emit_system(state, session_store, text);
    }
    let Some(plugin) = plugin_by_id(resources, args) else {
        return emit_system(state, session_store, format!("Unknown plugin {args}."));
    };
    let mut text = format!("Plugin {}\n{}\n", plugin.value.id, plugin.value.description);
    if !plugin.value.commands.is_empty() {
        let commands = plugin
            .value
            .commands
            .iter()
            .map(|command| command.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(&mut text, "Commands: {commands}");
    }
    if !plugin.value.skills.is_empty() {
        let _ = writeln!(&mut text, "Skills: {}", plugin.value.skills.join(", "));
    }
    if !plugin.value.mcp_servers.is_empty() {
        let ids = plugin
            .value
            .mcp_servers
            .iter()
            .map(|server| server.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(&mut text, "MCP servers: {ids}");
    }
    emit_system(state, session_store, text)
}

/// Lists loaded MCP servers from both resource packs and plugins.
#[allow(dead_code)]
pub(crate) fn list_mcp_servers(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    let servers = plugin_mcp_servers(resources);
    if servers.is_empty() && resources.mcp_servers.is_empty() {
        return emit_system(
            state,
            session_store,
            "No MCP servers are configured.".to_string(),
        );
    }
    let mut text = String::from("MCP servers:\n");
    for server in &resources.mcp_servers {
        let _ = writeln!(
            &mut text,
            "{} [{}] -> {}",
            server.value.id, server.value.transport, server.value.endpoint
        );
    }
    for (plugin, server) in servers {
        let target = if server.target.is_empty() {
            server.endpoint.as_str()
        } else {
            server.target.as_str()
        };
        let _ = writeln!(
            &mut text,
            "{}:{} [{}] -> {}",
            plugin.id, server.id, server.transport, target
        );
    }
    emit_system(state, session_store, text)
}

/// Lists loaded IDE integration manifests.
pub(crate) fn list_ides(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    if resources.ides.is_empty() {
        return emit_system(
            state,
            session_store,
            "No IDE integrations are configured.".to_string(),
        );
    }
    let mut text = String::from("IDE integrations:\n");
    for ide in &resources.ides {
        let _ = writeln!(
            &mut text,
            "{} - {}",
            ide.value.display_name, ide.value.description
        );
    }
    emit_system(state, session_store, text)
}

/// Shows or materializes the workspace agents file and agent presets.
pub(crate) fn handle_agents_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let agents_path = paths.workspace_config_dir.join("agents.yaml");
    if !agents_path.exists() {
        fs::write(
            &agents_path,
            default_agents_contents(state.current_model.as_deref()),
        )?;
    }
    let trimmed = args.trim();
    if trimmed == "path" {
        return emit_system(
            state,
            session_store,
            format!("Agents file: {}", agents_path.display()),
        );
    }
    let contents = fs::read_to_string(&agents_path)?;
    let parsed = parse_agents_file(&contents)?;
    match trimmed {
        "" | "show" => emit_system(
            state,
            session_store,
            format!("Agents file: {}\n{}", agents_path.display(), contents),
        ),
        "list" => {
            let mut text = String::from("Agents:\n");
            for agent in parsed.agents {
                let _ = writeln!(
                    &mut text,
                    "- {} role={} model={}",
                    agent.id, agent.role, agent.model
                );
            }
            emit_system(state, session_store, text)
        }
        _ if trimmed.starts_with("show ") => {
            let agent_id = trimmed.trim_start_matches("show ").trim();
            if let Some(agent) = parsed.agents.iter().find(|agent| agent.id == agent_id) {
                emit_system(
                    state,
                    session_store,
                    format!(
                        "Agent {}\nrole={}\nmodel={}",
                        agent.id, agent.role, agent.model
                    ),
                )
            } else {
                emit_system(state, session_store, format!("Unknown agent {agent_id}."))
            }
        }
        _ if trimmed.starts_with("use ") => {
            let agent_id = trimmed.trim_start_matches("use ").trim();
            if let Some(agent) = parsed.agents.iter().find(|agent| agent.id == agent_id) {
                state.current_model = Some(agent.model.clone());
                state.current_provider = agent
                    .model
                    .split_once('/')
                    .map(|(provider, _)| provider.to_string())
                    .or_else(|| state.current_provider.clone());
                emit_system(
                    state,
                    session_store,
                    format!(
                        "Selected agent {}.\nrole={}\nmodel={}",
                        agent.id, agent.role, agent.model
                    ),
                )
            } else {
                emit_system(state, session_store, format!("Unknown agent {agent_id}."))
            }
        }
        _ => emit_system(
            state,
            session_store,
            "Usage: /agents [path|list|show <id>|use <id>]".to_string(),
        ),
    }
}

/// Shows or materializes the workspace plugin directory.
pub(crate) fn handle_plugin_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let plugin_path = plugins_dir.join("workspace.yaml");
    if !plugin_path.exists() {
        fs::write(&plugin_path, default_plugin_contents())?;
    }
    if args.trim() == "path" {
        return emit_system(
            state,
            session_store,
            format!("Plugins directory: {}", plugins_dir.display()),
        );
    }
    if args.trim() == "list" {
        return describe_plugin(state, resources, session_store, "");
    }
    if !args.trim().is_empty() && args.trim() != "show" {
        return describe_plugin(state, resources, session_store, args);
    }
    emit_system(
        state,
        session_store,
        format!(
            "Plugins directory: {}\nloaded_plugins={}\n{}{}",
            plugins_dir.display(),
            resources.plugins.len(),
            if resources.plugins.is_empty() {
                format!("Example plugin file: {}\n", plugin_path.display())
            } else {
                let mut summary = String::from("Loaded plugins:\n");
                for plugin in &resources.plugins {
                    let _ = writeln!(
                        &mut summary,
                        "- {} -> {}",
                        plugin.value.id, plugin.value.display_name
                    );
                }
                summary
            },
            fs::read_to_string(&plugin_path)?
        ),
    )
}

/// Shows or materializes the workspace MCP directory.
pub(crate) fn handle_mcp_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let mcp_dir = paths.workspace_config_dir.join("resources/mcp_servers");
    fs::create_dir_all(&mcp_dir)?;
    let server_path = mcp_dir.join("workspace.yaml");
    if !server_path.exists() {
        fs::write(&server_path, default_mcp_contents())?;
    }
    let state_path = mcp_enablement_path(&paths);
    let mut enablement = load_or_initialize_mcp_enablement(&state_path)?;
    let entries = collect_mcp_entries(resources);
    let trimmed = args.trim();

    if let Some(raw_selector) = trimmed.strip_prefix("enable ") {
        let selector = raw_selector.trim();
        if selector.is_empty() {
            return emit_system(
                state,
                session_store,
                "Usage: /mcp enable <server-name>".to_string(),
            );
        }
        if !entries.iter().any(|entry| entry.selector == selector) {
            return emit_system(
                state,
                session_store,
                format!("Unknown MCP server `{selector}`."),
            );
        }
        if enablement.enable(selector) {
            write_mcp_enablement(&state_path, &enablement)?;
            return emit_system(
                state,
                session_store,
                format!(
                    "Enabled MCP server `{selector}` in {}.",
                    state_path.display()
                ),
            );
        }
        return emit_system(
            state,
            session_store,
            format!("MCP server `{selector}` is already enabled."),
        );
    }

    if let Some(raw_selector) = trimmed.strip_prefix("disable ") {
        let selector = raw_selector.trim();
        if selector.is_empty() {
            return emit_system(
                state,
                session_store,
                "Usage: /mcp disable <server-name>".to_string(),
            );
        }
        if !entries.iter().any(|entry| entry.selector == selector) {
            return emit_system(
                state,
                session_store,
                format!("Unknown MCP server `{selector}`."),
            );
        }
        if enablement.disable(selector) {
            write_mcp_enablement(&state_path, &enablement)?;
            return emit_system(
                state,
                session_store,
                format!(
                    "Disabled MCP server `{selector}` in {}.",
                    state_path.display()
                ),
            );
        }
        return emit_system(
            state,
            session_store,
            format!("MCP server `{selector}` is already disabled."),
        );
    }

    if args.trim() == "path" {
        return emit_system(
            state,
            session_store,
            format!(
                "MCP directory: {}\nMCP enablement file: {}",
                mcp_dir.display(),
                state_path.display()
            ),
        );
    }
    if trimmed == "list" {
        return emit_system(
            state,
            session_store,
            render_mcp_listing(&entries, &enablement),
        );
    }

    if !trimmed.is_empty() && trimmed != "show" {
        return emit_system(
            state,
            session_store,
            "Usage: /mcp [show|list|path|enable <server-name>|disable <server-name>]".to_string(),
        );
    }

    let mut summary = String::new();
    let _ = writeln!(
        &mut summary,
        "{}",
        render_mcp_listing(&entries, &enablement)
    );
    let _ = writeln!(&mut summary);
    let _ = writeln!(
        &mut summary,
        "Use `/mcp enable <server-name>` or `/mcp disable <server-name>` to persist MCP status."
    );
    emit_system(
        state,
        session_store,
        format!(
            "MCP directory: {}\nMCP enablement file: {}\n{}{}",
            mcp_dir.display(),
            state_path.display(),
            summary,
            fs::read_to_string(&server_path)?
        ),
    )
}

/// Shows or materializes the workspace IDE integration directory.
pub(crate) fn handle_ide_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let ide_dir = paths.workspace_config_dir.join("resources/ides");
    fs::create_dir_all(&ide_dir)?;
    let ide_path = ide_dir.join("workspace.yaml");
    if !ide_path.exists() {
        fs::write(&ide_path, default_ide_contents())?;
    }
    if args.trim() == "path" {
        return emit_system(
            state,
            session_store,
            format!("IDE directory: {}", ide_dir.display()),
        );
    }
    if args.trim() == "list" {
        return list_ides(state, resources, session_store);
    }
    if args.trim() == "open" {
        return emit_system(
            state,
            session_store,
            format!("Open your IDE integration from {}.", ide_dir.display()),
        );
    }
    emit_system(
        state,
        session_store,
        format!(
            "IDE directory: {}\nloaded_ides={}\n{}{}",
            ide_dir.display(),
            resources.ides.len(),
            if resources.ides.is_empty() {
                format!("Example IDE file: {}\n", ide_path.display())
            } else {
                let mut summary = String::from("Loaded IDE integrations:\n");
                for ide in &resources.ides {
                    let _ = writeln!(
                        &mut summary,
                        "- {} -> {}",
                        ide.value.id, ide.value.display_name
                    );
                }
                summary
            },
            fs::read_to_string(&ide_path)?
        ),
    )
}

/// Summarizes the current plugin registry after a reload request.
pub(crate) fn reload_plugins_summary(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<String> {
    let paths = ConfigPaths::discover(&state.cwd);
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    Ok(format!(
        "Reloaded plugin registry for this session.\nplugins={}\nskills={}\nmcp_servers={}\nsource_dir={}",
        resources.plugins.len(),
        resources.skills.len(),
        resources.mcp_servers.len(),
        plugins_dir.display()
    ))
}

/// Reloads declarative resources from disk and applies workspace MCP enablement.
#[allow(dead_code)]
pub(crate) fn reload_resources_from_disk(state: &AppState) -> Result<LoadedResources> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    reload_resources_from_paths(&paths)
}

#[allow(dead_code)]
fn reload_resources_from_paths(paths: &ConfigPaths) -> Result<LoadedResources> {
    let mut resources = load_resources(paths)?;
    apply_mcp_enablement_overrides(paths, &mut resources)?;
    Ok(resources)
}

fn default_agents_contents(model: Option<&str>) -> String {
    format!(
        "agents:\n  - id: default\n    role: coding\n    model: {}\n",
        model.unwrap_or("anthropic/claude-sonnet-4-5")
    )
}

fn default_plugin_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace Plugin\n\
description: Customize plugin commands for this workspace.\n\
commands:\n\
  - name: demo\n\
    description: Example command\n"
}

fn default_mcp_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace MCP\n\
transport: stdio\n\
endpoint: \"\"\n\
target: workspace\n\
description: Example MCP server\n\
enabled: true\n"
}

fn default_ide_contents() -> &'static str {
    "id: workspace\n\
display_name: Workspace IDE\n\
description: Example IDE integration\n"
}

fn parse_agents_file(raw: &str) -> Result<AgentsFile> {
    Ok(serde_yaml::from_str(raw)?)
}

#[derive(Debug, Clone, Deserialize)]
struct AgentsFile {
    agents: Vec<AgentEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentEntry {
    id: String,
    role: String,
    model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct McpEnablement {
    #[serde(default)]
    disabled: Vec<String>,
}

impl McpEnablement {
    fn is_disabled(&self, selector: &str) -> bool {
        let normalized = normalize_selector(selector);
        self.disabled.iter().any(|item| item == &normalized)
    }

    fn enable(&mut self, selector: &str) -> bool {
        let normalized = normalize_selector(selector);
        let before = self.disabled.len();
        self.disabled.retain(|item| item != &normalized);
        before != self.disabled.len()
    }

    fn disable(&mut self, selector: &str) -> bool {
        let normalized = normalize_selector(selector);
        if self.disabled.iter().any(|item| item == &normalized) {
            return false;
        }
        self.disabled.push(normalized);
        self.disabled.sort();
        self.disabled.dedup();
        true
    }
}

#[derive(Debug, Clone)]
struct McpEntry {
    selector: String,
    label: String,
    transport: String,
    target: String,
    source: String,
}

fn collect_mcp_entries(resources: &LoadedResources) -> Vec<McpEntry> {
    let mut entries = Vec::new();
    for server in &resources.mcp_servers {
        entries.push(McpEntry {
            selector: server.value.id.clone(),
            label: if server.value.display_name.is_empty() {
                server.value.id.clone()
            } else {
                server.value.display_name.clone()
            },
            transport: server.value.transport.clone(),
            target: if server.value.target.is_empty() {
                server.value.endpoint.clone()
            } else {
                server.value.target.clone()
            },
            source: format!("resource:{}", source_kind_label(server.source_info.kind)),
        });
    }
    for (plugin, server) in plugin_mcp_servers(resources) {
        entries.push(McpEntry {
            selector: format!("{}:{}", plugin.id, server.id),
            label: if server.display_name.is_empty() {
                format!("{}:{}", plugin.id, server.id)
            } else {
                server.display_name.clone()
            },
            transport: server.transport.clone(),
            target: if server.target.is_empty() {
                server.endpoint.clone()
            } else {
                server.target.clone()
            },
            source: format!("plugin:{}", plugin.id),
        });
    }
    entries.sort_by(|left, right| left.selector.cmp(&right.selector));
    entries
}

fn render_mcp_listing(entries: &[McpEntry], enablement: &McpEnablement) -> String {
    if entries.is_empty() {
        return "No MCP servers are configured.".to_string();
    }
    let mut text = String::from("MCP servers:\n");
    for entry in entries {
        let status = if enablement.is_disabled(&entry.selector) {
            "disabled"
        } else {
            "enabled"
        };
        let target = if entry.target.is_empty() {
            "<unset>"
        } else {
            entry.target.as_str()
        };
        let label = if entry.label != entry.selector {
            format!(" ({})", entry.label)
        } else {
            String::new()
        };
        let _ = writeln!(
            &mut text,
            "- {}{} [{}] {} -> {} ({})",
            entry.selector, label, status, entry.transport, target, entry.source
        );
    }
    text
}

fn mcp_enablement_path(paths: &ConfigPaths) -> PathBuf {
    paths.workspace_config_dir.join("mcp_servers.toml")
}

fn load_or_initialize_mcp_enablement(path: &PathBuf) -> Result<McpEnablement> {
    if path.exists() {
        return Ok(toml::from_str(&fs::read_to_string(path)?)?);
    }
    let default = McpEnablement::default();
    write_mcp_enablement(path, &default)?;
    Ok(default)
}

fn write_mcp_enablement(path: &PathBuf, value: &McpEnablement) -> Result<()> {
    fs::write(path, toml::to_string_pretty(value)?)?;
    Ok(())
}

fn normalize_selector(selector: &str) -> String {
    selector.trim().to_ascii_lowercase()
}

fn apply_mcp_enablement_overrides(
    paths: &ConfigPaths,
    resources: &mut LoadedResources,
) -> Result<()> {
    let settings = load_or_initialize_mcp_enablement(&mcp_enablement_path(paths))?;
    if settings.disabled.is_empty() {
        return Ok(());
    }
    resources
        .mcp_servers
        .retain(|server| !settings.is_disabled(&server.value.id));
    for plugin in &mut resources.plugins {
        let plugin_id = plugin.value.id.clone();
        plugin
            .value
            .mcp_servers
            .retain(|server| !settings.is_disabled(&format!("{}:{}", plugin_id, server.id)));
    }
    Ok(())
}

fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Builtin => "builtin",
        SourceKind::User => "user",
        SourceKind::Workspace => "workspace",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_config::ConfigPaths;
    use puffer_resources::{LoadedItem, McpServerSpec, PluginSpec, SourceInfo};
    use tempfile::tempdir;

    #[test]
    fn mcp_enablement_round_trip_and_filtering_work() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let paths = ConfigPaths::discover(root);
        ensure_workspace_dirs(&paths).unwrap();

        let mut resources = LoadedResources::default();
        resources.mcp_servers.push(LoadedItem {
            value: McpServerSpec {
                id: "docs".to_string(),
                display_name: "Docs".to_string(),
                transport: "stdio".to_string(),
                endpoint: String::new(),
                target: "docs".to_string(),
                description: String::new(),
            },
            source_info: SourceInfo {
                path: root.join("resources/mcp_servers/docs.yaml"),
                kind: SourceKind::Builtin,
            },
        });
        resources.plugins.push(LoadedItem {
            value: PluginSpec {
                id: "workspace".to_string(),
                display_name: "Workspace".to_string(),
                description: String::new(),
                commands: Vec::new(),
                skills: Vec::new(),
                mcp_servers: vec![McpServerSpec {
                    id: "logs".to_string(),
                    display_name: "Logs".to_string(),
                    transport: "stdio".to_string(),
                    endpoint: String::new(),
                    target: "logs".to_string(),
                    description: String::new(),
                }],
            },
            source_info: SourceInfo {
                path: root.join("resources/plugins/workspace.yaml"),
                kind: SourceKind::Workspace,
            },
        });

        let state_path = mcp_enablement_path(&paths);
        let mut enablement = load_or_initialize_mcp_enablement(&state_path).unwrap();
        assert!(!enablement.is_disabled("docs"));
        assert!(enablement.disable("docs"));
        assert!(enablement.disable("workspace:logs"));
        write_mcp_enablement(&state_path, &enablement).unwrap();

        apply_mcp_enablement_overrides(&paths, &mut resources).unwrap();
        assert!(resources.mcp_servers.is_empty());
        assert!(resources.plugins[0].value.mcp_servers.is_empty());
    }
}
