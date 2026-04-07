mod manage;
mod marketplace;
mod support;
mod validate;

use self::manage::{
    disable_workspace_plugin, enable_workspace_plugin, install_workspace_plugin,
    uninstall_workspace_plugin, update_workspace_plugin,
};
use self::marketplace::{
    add_marketplace, remove_marketplace, render_plugin_marketplace, update_marketplaces,
};
use self::support::{
    default_plugin_contents, format_plugin_counts, is_disabled_placeholder, plugin_description,
    plugin_help_text, plugin_status, source_kind_label,
};
use self::validate::{validate_loaded_plugin, validate_manifest_target};
use super::common::open_text_file_in_editor;
use super::{emit_system, CommandActionEntry};
use crate::AppState;
use anyhow::Result;
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_resources::{
    plugin_lsp_servers, plugin_mcp_servers, LoadedItem, LoadedResources, PluginSpec, SourceInfo,
    SourceKind,
};
use puffer_session_store::SessionStore;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Backward-compatible alias for plugin action picker rows.
pub type PluginActionEntry = CommandActionEntry;

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
    let trimmed = args.trim();
    let inventory = plugin_inventory(&paths, resources)?;

    match trimmed {
        "help" | "-h" | "--help" => emit_system(state, session_store, plugin_help_text()),
        "" | "show" | "manage" => emit_system(
            state,
            session_store,
            render_plugin_summary(state, resources)?,
        ),
        "marketplace" | "market" | "marketplace list" | "market list" => emit_system(
            state,
            session_store,
            render_plugin_marketplace(resources, &paths)?,
        ),
        "marketplace update" | "market update" => {
            emit_system(state, session_store, update_marketplaces(&paths, None)?)
        }
        "errors" => emit_system(
            state,
            session_store,
            render_plugin_errors(state, resources)?,
        ),
        "path" => emit_system(
            state,
            session_store,
            format!(
                "Plugins directory: {}\nWorkspace plugin manifest: {}",
                plugins_dir.display(),
                plugin_path.display()
            ),
        ),
        "list" => emit_system(state, session_store, render_plugin_listing(&inventory)),
        "validate" => emit_system(
            state,
            session_store,
            render_plugin_validation(&inventory, None),
        ),
        "install" | "i" => emit_system(
            state,
            session_store,
            format!(
                "{}\n\n{}",
                plugin_help_text(),
                render_plugin_marketplace(resources, &paths)?
            ),
        ),
        "uninstall" | "remove" | "update" => emit_system(state, session_store, plugin_help_text()),
        "reload" => {
            state.reload_resources_requested = true;
            emit_system(
                state,
                session_store,
                "Reloading plugin changes from disk for this session...".to_string(),
            )
        }
        "open" | "edit" => open_plugin_file(state, session_store, &plugin_path),
        _ if trimmed.starts_with("show ") => {
            let plugin_id = trimmed.trim_start_matches("show ").trim();
            describe_plugin(state, session_store, &inventory, plugin_id)
        }
        _ if trimmed.starts_with("install ") => {
            let plugin_ref = trimmed.trim_start_matches("install ").trim();
            install_workspace_plugin(state, resources, session_store, &paths, plugin_ref)
        }
        _ if trimmed.starts_with("i ") => {
            let plugin_ref = trimmed.trim_start_matches("i ").trim();
            install_workspace_plugin(state, resources, session_store, &paths, plugin_ref)
        }
        _ if trimmed.starts_with("uninstall ") || trimmed.starts_with("remove ") => {
            let plugin_id = trimmed
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            uninstall_workspace_plugin(state, resources, session_store, &paths, plugin_id)
        }
        _ if trimmed.starts_with("update ") => {
            let plugin_id = trimmed.trim_start_matches("update ").trim();
            update_workspace_plugin(state, resources, session_store, &paths, plugin_id)
        }
        _ if trimmed.starts_with("marketplace add ") || trimmed.starts_with("market add ") => {
            let source = trimmed
                .split_once(' ')
                .and_then(|(_, rest)| rest.split_once(' '))
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            emit_system(state, session_store, add_marketplace(&paths, source)?)
        }
        _ if trimmed.starts_with("marketplace remove ")
            || trimmed.starts_with("market remove ")
            || trimmed.starts_with("marketplace rm ")
            || trimmed.starts_with("market rm ") =>
        {
            let name = trimmed
                .rsplit_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            emit_system(state, session_store, remove_marketplace(&paths, name)?)
        }
        _ if trimmed.starts_with("marketplace update ")
            || trimmed.starts_with("market update ") =>
        {
            let name = trimmed
                .rsplit_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            emit_system(
                state,
                session_store,
                update_marketplaces(&paths, Some(name))?,
            )
        }
        _ if trimmed.starts_with("validate ") => {
            let selector = trimmed.trim_start_matches("validate ").trim();
            if selector.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /plugin validate <id|path>".to_string(),
                );
            }
            if should_treat_plugin_validation_target_as_path(&state.cwd, selector) {
                let text = match validate_manifest_target(&state.cwd, selector) {
                    Ok(text) => text,
                    Err(error) => format!("Plugin validation failed: {error}"),
                };
                emit_system(state, session_store, text)
            } else {
                emit_system(
                    state,
                    session_store,
                    render_plugin_validation(&inventory, Some(selector)),
                )
            }
        }
        _ if trimmed.starts_with("open ") || trimmed.starts_with("edit ") => {
            let plugin_id = trimmed
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            open_named_plugin_file(state, session_store, &inventory, plugin_id)
        }
        _ if trimmed.starts_with("enable ") => {
            let plugin_id = trimmed.trim_start_matches("enable ").trim();
            enable_workspace_plugin(state, resources, session_store, &paths, plugin_id)
        }
        _ if trimmed.starts_with("disable ") => {
            let plugin_id = trimmed.trim_start_matches("disable ").trim();
            disable_workspace_plugin(state, resources, session_store, &paths, plugin_id)
        }
        _ if inventory.iter().any(|plugin| plugin.value.id == trimmed) => {
            describe_plugin(state, session_store, &inventory, trimmed)
        }
        _ => emit_system(state, session_store, plugin_help_text()),
    }
}

/// Summarizes the current plugin registry after a reload request.
pub(crate) fn reload_plugins_summary(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<String> {
    let paths = ConfigPaths::discover(&state.cwd);
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    Ok(format!(
        "Reloaded plugin registry for this session.\nplugins={}\nskills={}\nmcp_servers={}\nlsp_servers={}\nsource_dir={}",
        resources.plugins.len(),
        resources.skills.len(),
        resources.mcp_servers.len() + plugin_mcp_servers(resources).len(),
        plugin_lsp_servers(resources).len(),
        plugins_dir.display()
    ))
}

/// Renders the plugin summary shown by `/plugin` with no arguments.
pub(crate) fn render_plugin_summary(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<String> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let plugin_path = plugins_dir.join("workspace.yaml");
    if !plugin_path.exists() {
        fs::write(&plugin_path, default_plugin_contents())?;
    }
    let inventory = plugin_inventory(&paths, resources)?;
    Ok(format!(
        "Plugins directory: {}\nworkspace_plugin_manifest={}\nloaded_plugins={}\n{}\nUse `/plugin marketplace`, `/plugin marketplace add <path|url>`, `/plugin marketplace remove <name>`, `/plugin marketplace update [name]`, `/plugin install <id|id@marketplace|path>`, `/plugin update <id|id@marketplace>`, `/plugin uninstall <id>`, `/plugin enable <id>`, `/plugin disable <id>`, `/plugin open <id>`, `/plugin validate [id|path]`, `/plugin errors`, or `/reload-plugins`.\n\n{}",
        plugins_dir.display(),
        plugin_path.display(),
        inventory.iter().filter(|plugin| !is_disabled_placeholder(&plugin.value)).count(),
        render_plugin_listing(&inventory),
        fs::read_to_string(&plugin_path)?
    ))
}

/// Builds the interactive `/plugin` action list used by the TUI picker.
pub(crate) fn render_plugin_actions(
    state: &AppState,
    resources: &LoadedResources,
) -> Result<Vec<PluginActionEntry>> {
    let paths = ConfigPaths::discover(&state.cwd);
    let inventory = plugin_inventory(&paths, resources)?;
    let mut actions = vec![
        PluginActionEntry {
            command: "/plugin marketplace".to_string(),
            description: "Browse builtin and custom plugin marketplaces".to_string(),
        },
        PluginActionEntry {
            command: "/plugin open".to_string(),
            description: format!(
                "Edit workspace plugin manifest ({})",
                paths
                    .workspace_config_dir
                    .join("resources/plugins/workspace.yaml")
                    .display()
            ),
        },
        PluginActionEntry {
            command: "/reload-plugins".to_string(),
            description: "Reload plugin changes from disk for this session".to_string(),
        },
        PluginActionEntry {
            command: "/plugin errors".to_string(),
            description: "Show plugin-specific resource diagnostics".to_string(),
        },
        PluginActionEntry {
            command: "/plugin validate".to_string(),
            description: "Validate loaded plugin manifests or one manifest path".to_string(),
        },
    ];
    for plugin in &inventory {
        if plugin.value.id == "workspace" {
            actions.push(PluginActionEntry {
                command: format!("/plugin open {}", plugin.value.id),
                description: format!("Open manifest {}", plugin.source_info.path.display()),
            });
            actions.push(PluginActionEntry {
                command: format!("/plugin validate {}", plugin.value.id),
                description: format!("Validate plugin {}", plugin.value.id),
            });
            continue;
        }
        let status = plugin_status(&plugin.value);
        let counts = format_plugin_counts(&plugin.value);
        let label = if plugin.value.display_name == plugin.value.id {
            plugin.value.display_name.clone()
        } else {
            format!("{} ({})", plugin.value.id, plugin.value.display_name)
        };
        actions.push(PluginActionEntry {
            command: format!(
                "/plugin {} {}",
                if is_disabled_placeholder(&plugin.value) {
                    "enable"
                } else {
                    "disable"
                },
                plugin.value.id
            ),
            description: format!(
                "{} [{}] {} • {}",
                label,
                status,
                source_kind_label(plugin.source_info.kind),
                counts
            ),
        });
        actions.push(PluginActionEntry {
            command: format!("/plugin open {}", plugin.value.id),
            description: format!("Open manifest {}", plugin.source_info.path.display()),
        });
        actions.push(PluginActionEntry {
            command: format!("/plugin validate {}", plugin.value.id),
            description: format!("Validate plugin {}", plugin.value.id),
        });
        if plugin.source_info.kind == SourceKind::Workspace {
            actions.push(PluginActionEntry {
                command: format!("/plugin uninstall {}", plugin.value.id),
                description: format!("Remove workspace override for {}", plugin.value.id),
            });
            actions.push(PluginActionEntry {
                command: format!("/plugin update {}", plugin.value.id),
                description: format!("Refresh {} from builtin/user source", plugin.value.id),
            });
        } else if !is_disabled_placeholder(&plugin.value) {
            actions.push(PluginActionEntry {
                command: format!("/plugin install {}", plugin.value.id),
                description: format!("Install an editable workspace copy of {}", plugin.value.id),
            });
        }
    }
    Ok(actions)
}

fn render_plugin_errors(state: &AppState, resources: &LoadedResources) -> Result<String> {
    let paths = ConfigPaths::discover(&state.cwd);
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    let diagnostics = resources
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.contains(" plugin `")
                || diagnostic.contains("/plugins/")
                || diagnostic.contains("\\plugins\\")
        })
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        return Ok(format!(
            "Plugin diagnostics\nsource_dir={}\nerrors=0\nNo plugin-specific resource diagnostics are currently recorded.",
            plugins_dir.display()
        ));
    }
    let mut text = format!(
        "Plugin diagnostics\nsource_dir={}\nerrors={}",
        plugins_dir.display(),
        diagnostics.len()
    );
    for diagnostic in diagnostics {
        let _ = writeln!(&mut text, "\n- {diagnostic}");
    }
    Ok(text)
}

fn render_plugin_validation(
    inventory: &[LoadedItem<PluginSpec>],
    plugin_id: Option<&str>,
) -> String {
    let selected = if let Some(plugin_id) = plugin_id {
        let Some(plugin) = inventory.iter().find(|plugin| plugin.value.id == plugin_id) else {
            return format!("Unknown plugin `{plugin_id}`.");
        };
        vec![plugin]
    } else {
        inventory.iter().collect::<Vec<_>>()
    };
    let mut text = String::from("Plugin validation\n");
    for plugin in selected {
        let report = validate_loaded_plugin(plugin);
        let status = if report.issues.is_empty() {
            "ok"
        } else {
            "issues"
        };
        let _ = writeln!(
            &mut text,
            "- {} [{}] path={}",
            report.plugin_id,
            status,
            report.path.display()
        );
        if report.issues.is_empty() {
            let _ = writeln!(
                &mut text,
                "  commands={} skills={} mcp_servers={} lsp_servers={}",
                report.commands, report.skills, report.mcp_servers, report.lsp_servers
            );
        } else {
            for issue in report.issues {
                let _ = writeln!(&mut text, "  issue: {issue}");
            }
        }
    }
    text.trim_end().to_string()
}

fn should_treat_plugin_validation_target_as_path(cwd: &Path, selector: &str) -> bool {
    let candidate = if Path::new(selector).is_absolute() {
        PathBuf::from(selector)
    } else {
        cwd.join(selector)
    };
    candidate.exists()
        || selector == "."
        || selector == ".."
        || selector.contains(std::path::MAIN_SEPARATOR)
        || selector.contains('/')
        || selector.contains('\\')
        || selector.ends_with(".yaml")
        || selector.ends_with(".yml")
}

fn plugin_inventory(
    paths: &ConfigPaths,
    resources: &LoadedResources,
) -> Result<Vec<LoadedItem<PluginSpec>>> {
    ensure_workspace_dirs(paths)?;
    let plugins_dir = paths.workspace_config_dir.join("resources/plugins");
    fs::create_dir_all(&plugins_dir)?;
    let workspace_plugin_path = plugins_dir.join("workspace.yaml");
    if !workspace_plugin_path.exists() {
        fs::write(&workspace_plugin_path, default_plugin_contents())?;
    }

    let mut inventory = resources.plugins.clone();
    if !inventory
        .iter()
        .any(|plugin| plugin.value.id == "workspace")
    {
        inventory.push(LoadedItem {
            value: serde_yaml::from_str(&fs::read_to_string(&workspace_plugin_path)?)?,
            source_info: SourceInfo {
                path: workspace_plugin_path,
                kind: SourceKind::Workspace,
            },
        });
    }
    inventory.sort_by(|left, right| left.value.id.cmp(&right.value.id));
    Ok(inventory)
}

fn describe_plugin(
    state: &mut AppState,
    session_store: &SessionStore,
    inventory: &[LoadedItem<PluginSpec>],
    plugin_id: &str,
) -> Result<()> {
    let Some(plugin) = inventory.iter().find(|plugin| plugin.value.id == plugin_id) else {
        return emit_system(
            state,
            session_store,
            format!("Unknown plugin `{plugin_id}`."),
        );
    };
    let mut text = String::new();
    let _ = writeln!(&mut text, "Plugin {}", plugin.value.id);
    let _ = writeln!(&mut text, "Name: {}", plugin.value.display_name);
    let _ = writeln!(&mut text, "Status: {}", plugin_status(&plugin.value));
    let _ = writeln!(
        &mut text,
        "Source: {} ({})",
        source_kind_label(plugin.source_info.kind),
        plugin.source_info.path.display()
    );
    let description = plugin_description(&plugin.value);
    if !description.is_empty() {
        let _ = writeln!(&mut text, "Description: {description}");
    }
    let _ = writeln!(&mut text, "Counts: {}", format_plugin_counts(&plugin.value));
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
    if !plugin.value.agents.is_empty() {
        let ids = plugin
            .value
            .agents
            .iter()
            .map(|agent| agent.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(&mut text, "Agents: {ids}");
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
    if !plugin.value.lsp_servers.is_empty() {
        let ids = plugin
            .value
            .lsp_servers
            .iter()
            .map(|server| server.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(&mut text, "LSP servers: {ids}");
    }
    emit_system(state, session_store, text)
}

fn render_plugin_listing(inventory: &[LoadedItem<PluginSpec>]) -> String {
    if inventory.is_empty() {
        return "Plugins:\n<none>".to_string();
    }
    let mut text = String::from("Plugins:\n");
    for plugin in inventory {
        let description = plugin_description(&plugin.value);
        let details = if description.is_empty() {
            format_plugin_counts(&plugin.value)
        } else {
            format!("{description} • {}", format_plugin_counts(&plugin.value))
        };
        let _ = writeln!(
            &mut text,
            "- {} [{}] source={} path={} • {}",
            plugin.value.id,
            plugin_status(&plugin.value),
            source_kind_label(plugin.source_info.kind),
            plugin.source_info.path.display(),
            details
        );
    }
    text
}

fn open_named_plugin_file(
    state: &mut AppState,
    session_store: &SessionStore,
    inventory: &[LoadedItem<PluginSpec>],
    plugin_id: &str,
) -> Result<()> {
    let Some(plugin) = inventory.iter().find(|plugin| plugin.value.id == plugin_id) else {
        return emit_system(
            state,
            session_store,
            format!("Unknown plugin `{plugin_id}`."),
        );
    };
    open_plugin_file(state, session_store, &plugin.source_info.path)
}

fn open_plugin_file(state: &mut AppState, session_store: &SessionStore, path: &Path) -> Result<()> {
    match open_text_file_in_editor(path) {
        Ok(status) => emit_system(state, session_store, status),
        Err(error) => emit_system(
            state,
            session_store,
            format!(
                "Could not open plugin manifest in an editor: {error}\nPath: {}",
                path.display()
            ),
        ),
    }
}
