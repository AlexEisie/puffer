use super::emit_system;
use crate::AppState;
use anyhow::Result;
use puffer_config::{ensure_workspace_dirs, save_user_config, ConfigPaths};
use puffer_resources::{hook_by_id, LoadedResources};
use puffer_session_store::SessionStore;
use puffer_tools::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Summarizes loaded tool approval and sandbox metadata.
pub(crate) fn describe_permissions(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    let registry = ToolRegistry::from_resources(resources);
    if registry.tools().count() == 0 {
        return emit_system(
            state,
            session_store,
            "No tool metadata is loaded.".to_string(),
        );
    }

    let mut text = String::from("Tool permission summary:\n");
    for tool in registry.tools() {
        let _ = writeln!(
            &mut text,
            "- {} [{}]: approval={} sandbox={}",
            tool.spec.name,
            tool.spec.handler,
            tool.spec
                .policy
                .approval_policy
                .as_deref()
                .unwrap_or("<unspecified>"),
            tool.spec
                .policy
                .sandbox_policy
                .as_deref()
                .unwrap_or("<unspecified>")
        );
    }
    emit_system(state, session_store, text)
}

/// Shows or materializes the workspace permissions file.
pub(crate) fn handle_permissions_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let permissions_path = paths.workspace_config_dir.join("permissions.toml");
    let mut settings = load_or_initialize_permissions(&permissions_path, resources)?;
    let trimmed = args.trim();
    match trimmed {
        "path" => {
            emit_system(
                state,
                session_store,
                format!("Permissions file: {}", permissions_path.display()),
            )
        }
        "" | "show" | "list" => emit_system(
            state,
            session_store,
            render_permissions_summary(&permissions_path, &settings),
        ),
        _ if trimmed.starts_with("allow ") => {
            let tool = trimmed.trim_start_matches("allow ").trim();
            if tool.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /permissions allow <tool-id>".to_string(),
                );
            }
            set_permission_level(&mut settings, tool, "allow");
            write_permissions(&permissions_path, &settings)?;
            emit_system(
                state,
                session_store,
                format!("Set {} to `allow` in {}.", tool, permissions_path.display()),
            )
        }
        _ if trimmed.starts_with("deny ") => {
            let tool = trimmed.trim_start_matches("deny ").trim();
            if tool.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /permissions deny <tool-id>".to_string(),
                );
            }
            set_permission_level(&mut settings, tool, "deny");
            write_permissions(&permissions_path, &settings)?;
            emit_system(
                state,
                session_store,
                format!("Set {} to `deny` in {}.", tool, permissions_path.display()),
            )
        }
        _ if trimmed.starts_with("ask ") => {
            let tool = trimmed.trim_start_matches("ask ").trim();
            if tool.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /permissions ask <tool-id>".to_string(),
                );
            }
            set_permission_level(&mut settings, tool, "ask");
            write_permissions(&permissions_path, &settings)?;
            emit_system(
                state,
                session_store,
                format!("Set {} to `ask` in {}.", tool, permissions_path.display()),
            )
        }
        _ if trimmed.starts_with("remove ") => {
            let tool = trimmed.trim_start_matches("remove ").trim();
            if tool.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /permissions remove <tool-id>".to_string(),
                );
            }
            settings.tools.remove(&normalize_tool_id(tool));
            write_permissions(&permissions_path, &settings)?;
            emit_system(
                state,
                session_store,
                format!(
                    "Removed explicit rule for {} in {}.",
                    tool,
                    permissions_path.display()
                ),
            )
        }
        "summary" => describe_permissions(state, resources, session_store),
        _ => emit_system(
            state,
            session_store,
            "Usage: /permissions [show|list|path|summary|allow <tool-id>|deny <tool-id>|ask <tool-id>|remove <tool-id>]".to_string(),
        ),
    }
}

/// Shows or updates the workspace config file.
pub(crate) fn handle_config_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let config_path = paths.workspace_config_file();
    let trimmed = args.trim();

    if trimmed.is_empty() || trimmed == "show" {
        return emit_system(
            state,
            session_store,
            format!(
                "Config summary:\npath={}\napp_name={}\ndefault_provider={}\ndefault_model={}\nopenai_base_url={}\ntheme={}\nno_alt_screen={}\ntmux_golden_mode={}",
                config_path.display(),
                state.config.app_name,
                state.config.default_provider.as_deref().unwrap_or("<unset>"),
                state.config.default_model.as_deref().unwrap_or("<unset>"),
                state.config.openai_base_url.as_deref().unwrap_or("<unset>"),
                state.config.theme,
                state.config.ui.no_alt_screen,
                state.config.ui.tmux_golden_mode,
            ),
        );
    }

    if trimmed == "path" {
        return emit_system(
            state,
            session_store,
            format!("Workspace config path: {}", config_path.display()),
        );
    }

    if trimmed == "open" {
        return emit_system(
            state,
            session_store,
            format!(
                "Open your workspace config file at {}.",
                config_path.display()
            ),
        );
    }

    let Some(rest) = trimmed.strip_prefix("set ") else {
        return emit_system(
            state,
            session_store,
            "Usage: /config [show|path|set <theme|default_provider|default_model|openai_base_url|no_alt_screen|tmux_golden_mode> <value>]".to_string(),
        );
    };
    let Some((key, value)) = rest.split_once(' ') else {
        return emit_system(
            state,
            session_store,
            "Usage: /config set <key> <value>".to_string(),
        );
    };
    let value = value.trim();
    match key {
        "theme" => state.config.theme = value.to_string(),
        "default_provider" => state.config.default_provider = Some(value.to_string()),
        "default_model" => state.config.default_model = Some(value.to_string()),
        "openai_base_url" => {
            state.config.openai_base_url = match value {
                "none" | "default" | "<unset>" => None,
                _ => Some(value.to_string()),
            }
        }
        "no_alt_screen" => state.config.ui.no_alt_screen = parse_bool(value)?,
        "tmux_golden_mode" => state.config.ui.tmux_golden_mode = parse_bool(value)?,
        _ => {
            return emit_system(
                state,
                session_store,
                format!("Unsupported config key {key}."),
            );
        }
    }
    write_workspace_config(state, &config_path)?;
    emit_system(
        state,
        session_store,
        format!("Updated {key} in {}.", config_path.display()),
    )
}

/// Persists the currently selected provider and model to the user config file.
pub(crate) fn persist_user_model_selection(state: &AppState) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    save_user_config(&paths, &state.config)
}

/// Shows or materializes the workspace keybindings file.
pub(crate) fn handle_keybindings_command(
    state: &mut AppState,
    session_store: &SessionStore,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let keybindings_path = paths.workspace_config_dir.join("keybindings.toml");
    if !keybindings_path.exists() {
        fs::write(&keybindings_path, default_keybindings_contents())?;
    }
    emit_system(
        state,
        session_store,
        format!(
            "Keybindings file: {}\n{}",
            keybindings_path.display(),
            fs::read_to_string(&keybindings_path)?
        ),
    )
}

/// Shows or materializes the workspace hooks directory and example hook.
pub(crate) fn handle_hooks_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let hooks_dir = paths.workspace_config_dir.join("resources/hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hooks_path = hooks_dir.join("tool_end.yaml");
    if !hooks_path.exists() {
        fs::write(&hooks_path, default_hooks_contents())?;
    }
    let trimmed = args.trim();
    if trimmed == "path" {
        return emit_system(
            state,
            session_store,
            format!("Hooks directory: {}", hooks_dir.display()),
        );
    }

    if trimmed == "list" {
        if resources.hooks.is_empty() {
            return emit_system(
                state,
                session_store,
                "No hook configurations are loaded.".to_string(),
            );
        }
        let mut summary = String::from("Loaded hooks:\n");
        for hook in &resources.hooks {
            let _ = writeln!(
                &mut summary,
                "- {} [{}] -> {}",
                hook.value.id, hook.value.event, hook.value.command
            );
        }
        return emit_system(state, session_store, summary);
    }

    if let Some(hook_id) = trimmed.strip_prefix("show ") {
        let hook_id = hook_id.trim();
        if hook_id.is_empty() {
            return emit_system(
                state,
                session_store,
                "Usage: /hooks show <hook-id>".to_string(),
            );
        }
        if let Some(hook) = hook_by_id(resources, hook_id) {
            return emit_system(
                state,
                session_store,
                format!(
                    "Hook {}\nevent={}\ncommand={}\nsource={}",
                    hook.value.id,
                    hook.value.event,
                    hook.value.command,
                    hook.source_info.path.display()
                ),
            );
        }
        return emit_system(state, session_store, format!("Unknown hook `{hook_id}`."));
    }

    if trimmed == "open" {
        return emit_system(
            state,
            session_store,
            format!("Open your hooks directory at {}.", hooks_dir.display()),
        );
    }

    emit_system(
        state,
        session_store,
        format!(
            "Hooks directory: {}\nloaded_hooks={}\n{}{}",
            hooks_dir.display(),
            resources.hooks.len(),
            if resources.hooks.is_empty() {
                format!("Example hook file: {}\n", hooks_path.display())
            } else {
                let mut summary = String::from("Loaded hooks:\n");
                for hook in &resources.hooks {
                    let _ = writeln!(
                        &mut summary,
                        "- {} [{}] -> {}",
                        hook.value.id, hook.value.event, hook.value.command
                    );
                }
                summary
            },
            fs::read_to_string(&hooks_path)?
        ),
    )
}

fn normalize_tool_id(tool: &str) -> String {
    tool.trim().replace('-', "_")
}

fn set_permission_level(settings: &mut PermissionsSettings, tool: &str, level: &str) {
    settings
        .tools
        .insert(normalize_tool_id(tool), level.to_string());
}

fn load_or_initialize_permissions(
    path: &PathBuf,
    resources: &LoadedResources,
) -> Result<PermissionsSettings> {
    if path.exists() {
        return Ok(toml::from_str(&fs::read_to_string(path)?)?);
    }
    let default = default_permissions_contents(resources);
    fs::write(path, default)?;
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

fn write_permissions(path: &PathBuf, settings: &PermissionsSettings) -> Result<()> {
    fs::write(path, toml::to_string_pretty(settings)?)?;
    Ok(())
}

fn render_permissions_summary(path: &PathBuf, settings: &PermissionsSettings) -> String {
    let mut body = String::from("Tool rules:\n");
    if settings.tools.is_empty() {
        body.push_str("- <none>\n");
    } else {
        for (tool, level) in &settings.tools {
            let _ = writeln!(&mut body, "- {tool}: {level}");
        }
    }
    format!("Permissions file: {}\n{}", path.display(), body.trim_end())
}

/// Shows or updates the workspace sandbox configuration file.
pub(crate) fn handle_sandbox_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let paths = ConfigPaths::discover(&state.cwd);
    ensure_workspace_dirs(&paths)?;
    let sandbox_path = paths.workspace_config_dir.join("sandbox.toml");
    let mut settings = load_or_initialize_sandbox_settings(&sandbox_path, state)?;
    let trimmed = args.trim();

    if trimmed.is_empty() || trimmed == "show" {
        return emit_system(
            state,
            session_store,
            render_sandbox_summary(&sandbox_path, &settings),
        );
    }

    if trimmed == "path" {
        return emit_system(
            state,
            session_store,
            format!("Sandbox config path: {}", sandbox_path.display()),
        );
    }

    if let Some(pattern) = trimmed.strip_prefix("exclude ") {
        let pattern = pattern.trim().trim_matches('"');
        if pattern.is_empty() {
            anyhow::bail!("expected a command pattern after `exclude`");
        }
        if !settings
            .excluded_commands
            .iter()
            .any(|existing| existing == pattern)
        {
            settings.excluded_commands.push(pattern.to_string());
        }
        write_sandbox_settings(&sandbox_path, &settings)?;
        return emit_system(
            state,
            session_store,
            format!(
                "Added sandbox exclusion `{pattern}` in {}.",
                sandbox_path.display()
            ),
        );
    }

    if trimmed == "clear-excludes" {
        settings.excluded_commands.clear();
        write_sandbox_settings(&sandbox_path, &settings)?;
        return emit_system(
            state,
            session_store,
            format!("Cleared sandbox exclusions in {}.", sandbox_path.display()),
        );
    }

    if let Some(value) = trimmed.strip_prefix("allow-unsandboxed ") {
        settings.allow_unsandboxed_fallback = parse_bool(value.trim())?;
        write_sandbox_settings(&sandbox_path, &settings)?;
        return emit_system(
            state,
            session_store,
            format!(
                "allow_unsandboxed_fallback set to {} in {}.",
                settings.allow_unsandboxed_fallback,
                sandbox_path.display()
            ),
        );
    }

    if let Some(value) = trimmed.strip_prefix("auto-allow ") {
        settings.auto_allow = parse_bool(value.trim())?;
        write_sandbox_settings(&sandbox_path, &settings)?;
        return emit_system(
            state,
            session_store,
            format!(
                "auto_allow set to {} in {}.",
                settings.auto_allow,
                sandbox_path.display()
            ),
        );
    }

    let mode = trimmed
        .strip_prefix("mode ")
        .map(str::trim)
        .unwrap_or(trimmed)
        .to_string();
    settings.mode = mode.clone();
    state.sandbox_mode = mode;
    write_sandbox_settings(&sandbox_path, &settings)?;
    emit_system(
        state,
        session_store,
        format!(
            "Sandbox mode set to {} in {}.",
            state.sandbox_mode,
            sandbox_path.display()
        ),
    )
}

fn parse_bool(value: &str) -> Result<bool> {
    match value {
        "true" | "on" | "1" => Ok(true),
        "false" | "off" | "0" => Ok(false),
        _ => anyhow::bail!("expected a boolean value, got `{value}`"),
    }
}

fn write_workspace_config(state: &AppState, path: &PathBuf) -> Result<()> {
    fs::write(path, toml::to_string_pretty(&state.config)?)?;
    Ok(())
}

fn default_keybindings_contents() -> &'static str {
    "submit = \"enter\"\nclear_input = \"esc\"\nexit = \"ctrl+c\"\n"
}

fn default_permissions_contents(resources: &LoadedResources) -> String {
    let mut text = String::from("[tools]\n");
    for tool in &resources.tools {
        let key = tool.value.id.replace('-', "_");
        let _ = writeln!(&mut text, "{key} = \"ask\"");
    }
    if resources.tools.is_empty() {
        text.push_str("bash = \"ask\"\n");
    }
    text
}

fn default_hooks_contents() -> &'static str {
    "id: tool-end\n\
event: tool_end\n\
command: echo \"$PUFFER_TOOL_ID:$PUFFER_TOOL_SUCCESS\"\n"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PermissionsSettings {
    #[serde(default)]
    tools: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SandboxSettings {
    mode: String,
    #[serde(default)]
    auto_allow: bool,
    #[serde(default)]
    allow_unsandboxed_fallback: bool,
    #[serde(default)]
    excluded_commands: Vec<String>,
}

fn load_or_initialize_sandbox_settings(
    path: &PathBuf,
    state: &AppState,
) -> Result<SandboxSettings> {
    if path.exists() {
        return Ok(toml::from_str(&fs::read_to_string(path)?)?);
    }
    let settings = SandboxSettings {
        mode: state.sandbox_mode.clone(),
        auto_allow: false,
        allow_unsandboxed_fallback: false,
        excluded_commands: Vec::new(),
    };
    write_sandbox_settings(path, &settings)?;
    Ok(settings)
}

fn write_sandbox_settings(path: &PathBuf, settings: &SandboxSettings) -> Result<()> {
    fs::write(path, toml::to_string_pretty(settings)?)?;
    Ok(())
}

fn render_sandbox_summary(path: &PathBuf, settings: &SandboxSettings) -> String {
    let exclusions = if settings.excluded_commands.is_empty() {
        String::from("<none>")
    } else {
        settings.excluded_commands.join(", ")
    };
    format!(
        "Sandbox summary:\npath={}\nmode={}\nauto_allow={}\nallow_unsandboxed_fallback={}\nexcluded_commands={}",
        path.display(),
        settings.mode,
        settings.auto_allow,
        settings.allow_unsandboxed_fallback,
        exclusions
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn permissions_round_trip_supports_allow_and_remove() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("permissions.toml");
        let mut settings = PermissionsSettings::default();
        set_permission_level(&mut settings, "read-file", "allow");
        write_permissions(&path, &settings).unwrap();
        let loaded: PermissionsSettings =
            toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            loaded.tools.get("read_file").map(String::as_str),
            Some("allow")
        );
    }
}
