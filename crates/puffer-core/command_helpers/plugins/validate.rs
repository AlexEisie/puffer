use super::is_disabled_placeholder;
use anyhow::{bail, Context, Result};
use puffer_resources::{LoadedItem, PluginSpec};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Carries one plugin-manifest validation result for command rendering.
#[derive(Debug, Clone)]
pub(super) struct PluginValidationReport {
    pub(super) plugin_id: String,
    pub(super) path: PathBuf,
    pub(super) issues: Vec<String>,
    pub(super) commands: usize,
    pub(super) skills: usize,
    pub(super) mcp_servers: usize,
    pub(super) lsp_servers: usize,
}

/// Validates one already-loaded plugin manifest and returns a renderable report.
pub(super) fn validate_loaded_plugin(plugin: &LoadedItem<PluginSpec>) -> PluginValidationReport {
    report_for_spec(plugin.value.clone(), plugin.source_info.path.clone())
}

/// Validates a plugin manifest file or directory relative to the current cwd.
pub(super) fn validate_manifest_target(cwd: &Path, target: &str) -> Result<String> {
    let resolved = resolve_target(cwd, target);
    if !resolved.exists() {
        bail!(
            "Plugin validation target `{}` does not exist.",
            resolved.display()
        );
    }

    let reports = if resolved.is_file() {
        vec![validate_manifest_file(&resolved)]
    } else if resolved.is_dir() {
        validate_manifest_directory(&resolved)?
    } else {
        bail!(
            "Plugin validation target `{}` is neither a file nor a directory.",
            resolved.display()
        );
    };

    let mut text = String::new();
    let _ = writeln!(
        &mut text,
        "Plugin validation\ntarget={}\nvalidated_manifests={}",
        resolved.display(),
        reports.len()
    );
    for report in reports {
        append_report(&mut text, &report);
    }
    Ok(text.trim_end().to_string())
}

fn validate_manifest_directory(directory: &Path) -> Result<Vec<PluginValidationReport>> {
    let manifests = discover_manifest_files(directory)?;
    if manifests.is_empty() {
        bail!(
            "No plugin manifests were found in `{}`.\nChecked `plugin.yaml`, `plugin.yml`, `resources/plugins/*.yaml`, `.puffer/resources/plugins/*.yaml`, `plugins/*.yaml`, and direct `*.yaml` files.",
            directory.display()
        );
    }
    Ok(manifests
        .into_iter()
        .map(|path| validate_manifest_file(&path))
        .collect())
}

fn discover_manifest_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut manifests = BTreeSet::new();

    for name in ["plugin.yaml", "plugin.yml"] {
        let candidate = directory.join(name);
        if candidate.is_file() {
            manifests.insert(candidate);
        }
    }

    for relative_dir in ["resources/plugins", ".puffer/resources/plugins", "plugins"] {
        let nested = directory.join(relative_dir);
        if nested.is_dir() {
            for entry in read_directory_yaml_files(&nested)? {
                manifests.insert(entry);
            }
        }
    }

    if manifests.is_empty() {
        for entry in read_directory_yaml_files(directory)? {
            manifests.insert(entry);
        }
    }

    Ok(manifests.into_iter().collect())
}

fn read_directory_yaml_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to read {}", directory.display()))?
    {
        let entry = entry.with_context(|| format!("failed to inspect {}", directory.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| matches!(value, "yaml" | "yml"))
        {
            entries.push(path);
        }
    }
    entries.sort();
    Ok(entries)
}

fn validate_manifest_file(path: &Path) -> PluginValidationReport {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            return PluginValidationReport {
                plugin_id: path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("<unknown>")
                    .to_string(),
                path: path.to_path_buf(),
                issues: vec![format!("failed to read manifest: {error}")],
                commands: 0,
                skills: 0,
                mcp_servers: 0,
                lsp_servers: 0,
            };
        }
    };

    match serde_yaml::from_str::<PluginSpec>(&raw) {
        Ok(spec) => report_for_spec(spec, path.to_path_buf()),
        Err(error) => PluginValidationReport {
            plugin_id: path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("<unknown>")
                .to_string(),
            path: path.to_path_buf(),
            issues: vec![format!("failed to parse manifest: {error}")],
            commands: 0,
            skills: 0,
            mcp_servers: 0,
            lsp_servers: 0,
        },
    }
}

fn report_for_spec(spec: PluginSpec, path: PathBuf) -> PluginValidationReport {
    PluginValidationReport {
        plugin_id: spec.id.clone(),
        path,
        issues: validate_plugin_spec(&spec),
        commands: spec.commands.len(),
        skills: spec.skills.len(),
        mcp_servers: spec.mcp_servers.len(),
        lsp_servers: spec.lsp_servers.len(),
    }
}

fn validate_plugin_spec(plugin: &PluginSpec) -> Vec<String> {
    let mut issues = Vec::new();
    if plugin.id.trim().is_empty() {
        issues.push("plugin id must not be empty".to_string());
    }
    if plugin.display_name.trim().is_empty() {
        issues.push("display_name must not be empty".to_string());
    }
    collect_duplicates(
        plugin.commands.iter().map(|command| command.name.as_str()),
        "command",
        &mut issues,
    );
    collect_duplicates(
        plugin.skills.iter().map(|skill| skill.as_str()),
        "skill",
        &mut issues,
    );
    collect_duplicates(
        plugin.agents.iter().map(|agent| agent.id.as_str()),
        "agent",
        &mut issues,
    );
    collect_duplicates(
        plugin.mcp_servers.iter().map(|server| server.id.as_str()),
        "mcp server",
        &mut issues,
    );
    collect_duplicates(
        plugin.lsp_servers.iter().map(|server| server.id.as_str()),
        "lsp server",
        &mut issues,
    );
    if is_disabled_placeholder(plugin)
        && (!plugin.commands.is_empty()
            || !plugin.skills.is_empty()
            || !plugin.agents.is_empty()
            || !plugin.mcp_servers.is_empty()
            || !plugin.lsp_servers.is_empty())
    {
        issues.push(
            "disabled placeholder should not retain commands, skills, agents, MCP servers, or LSP servers"
                .to_string(),
        );
    }
    issues
}

fn collect_duplicates<'a, I>(values: I, label: &str, issues: &mut Vec<String>)
where
    I: IntoIterator<Item = &'a str>,
{
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for value in values {
        let normalized = value.trim();
        if normalized.is_empty() {
            duplicates.insert("<empty>".to_string());
            continue;
        }
        if !seen.insert(normalized.to_string()) {
            duplicates.insert(normalized.to_string());
        }
    }
    for duplicate in duplicates {
        issues.push(format!("duplicate {label} `{duplicate}`"));
    }
}

fn append_report(text: &mut String, report: &PluginValidationReport) {
    let status = if report.issues.is_empty() {
        "ok"
    } else {
        "issues"
    };
    let _ = writeln!(
        text,
        "\n- {} [{}] path={}",
        report.plugin_id,
        status,
        report.path.display()
    );
    if report.issues.is_empty() {
        let _ = writeln!(
            text,
            "  commands={} skills={} mcp_servers={} lsp_servers={}",
            report.commands, report.skills, report.mcp_servers, report.lsp_servers
        );
    } else {
        for issue in &report.issues {
            let _ = writeln!(text, "  issue: {issue}");
        }
    }
}

fn resolve_target(cwd: &Path, target: &str) -> PathBuf {
    let path = Path::new(target);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}
