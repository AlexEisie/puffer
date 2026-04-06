use crate::model::{
    IdeSpec, LoadedItem, LoadedResources, MascotSpec, McpServerSpec, PluginSpec, PromptTemplate,
    ProviderPack, SkillSpec, SourceInfo, SourceKind, ToolSpec,
};
use anyhow::{Context, Result};
use puffer_config::ConfigPaths;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Loads bundled, user, and workspace resources into one in-memory registry.
pub fn load_resources(paths: &ConfigPaths) -> Result<LoadedResources> {
    let mut loaded = LoadedResources::default();
    for (root, kind) in resource_roots(paths) {
        loaded.providers.extend(load_yaml_dir::<ProviderPack>(
            &root.join("providers"),
            kind,
        )?);
        loaded
            .tools
            .extend(load_yaml_dir::<ToolSpec>(&root.join("tools"), kind)?);
        loaded.prompts.extend(load_yaml_dir::<PromptTemplate>(
            &root.join("prompts"),
            kind,
        )?);
        loaded
            .skills
            .extend(load_skill_dir(&root.join("skills"), kind)?);
        loaded
            .mascots
            .extend(load_yaml_dir::<MascotSpec>(&root.join("mascots"), kind)?);
        loaded
            .plugins
            .extend(load_yaml_dir::<PluginSpec>(&root.join("plugins"), kind)?);
        loaded.mcp_servers.extend(load_yaml_dir::<McpServerSpec>(
            &root.join("mcp_servers"),
            kind,
        )?);
        loaded
            .ides
            .extend(load_yaml_dir::<IdeSpec>(&root.join("ides"), kind)?);
    }
    Ok(loaded)
}

/// Looks up a prompt template by id.
pub fn prompt_by_id<'a>(
    resources: &'a LoadedResources,
    id: &str,
) -> Option<&'a LoadedItem<PromptTemplate>> {
    resources
        .prompts
        .iter()
        .find(|prompt| prompt.value.id == id)
}

/// Looks up a skill specification by its stable name.
pub fn skill_by_name<'a>(
    resources: &'a LoadedResources,
    name: &str,
) -> Option<&'a LoadedItem<SkillSpec>> {
    resources
        .skills
        .iter()
        .find(|skill| skill.value.name == name)
}

/// Looks up a plugin manifest by id.
pub fn plugin_by_id<'a>(
    resources: &'a LoadedResources,
    plugin_id: &str,
) -> Option<&'a LoadedItem<PluginSpec>> {
    resources
        .plugins
        .iter()
        .find(|plugin| plugin.value.id == plugin_id)
}

/// Collects every MCP server declared by loaded plugins.
pub fn plugin_mcp_servers(resources: &LoadedResources) -> Vec<(&PluginSpec, &McpServerSpec)> {
    resources
        .plugins
        .iter()
        .flat_map(|plugin| {
            plugin
                .value
                .mcp_servers
                .iter()
                .map(move |server| (&plugin.value, server))
        })
        .collect()
}

fn resource_roots(paths: &ConfigPaths) -> Vec<(PathBuf, SourceKind)> {
    vec![
        (paths.builtin_resources_dir.clone(), SourceKind::Builtin),
        (paths.user_config_dir.join("resources"), SourceKind::User),
        (
            paths.workspace_config_dir.join("resources"),
            SourceKind::Workspace,
        ),
    ]
}

fn load_yaml_dir<T>(dir: &Path, kind: SourceKind) -> Result<Vec<LoadedItem<T>>>
where
    T: DeserializeOwned,
{
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(dir)
        .with_context(|| format!("failed to read resource dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yaml" | "yml")
        ) {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read resource file {}", path.display()))?;
        let value = serde_yaml::from_str::<T>(&raw)
            .with_context(|| format!("failed to parse resource file {}", path.display()))?;
        items.push(LoadedItem {
            value,
            source_info: SourceInfo { path, kind },
        });
    }
    Ok(items)
}

fn load_skill_dir(dir: &Path, kind: SourceKind) -> Result<Vec<LoadedItem<SkillSpec>>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| format!("failed to read skills dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let skill_path = path.join("SKILL.md");
        if !skill_path.exists() {
            continue;
        }
        let raw = fs::read_to_string(&skill_path)
            .with_context(|| format!("failed to read skill file {}", skill_path.display()))?;
        let (frontmatter, body) = split_frontmatter(&raw);
        let name = frontmatter
            .get("name")
            .cloned()
            .unwrap_or_else(|| path.file_name().unwrap().to_string_lossy().to_string());
        let description = frontmatter
            .get("description")
            .cloned()
            .unwrap_or_else(|| first_descriptive_line(&body).to_string());
        let disable_model_invocation = frontmatter
            .get("disable-model-invocation")
            .map(|value| matches!(value.as_str(), "true" | "1" | "yes"))
            .unwrap_or(false);

        items.push(LoadedItem {
            value: SkillSpec {
                name,
                description,
                content: body,
                disable_model_invocation,
            },
            source_info: SourceInfo {
                path: skill_path,
                kind,
            },
        });
    }
    Ok(items)
}

fn split_frontmatter(raw: &str) -> (BTreeMap<String, String>, String) {
    let normalized = raw.replace("\r\n", "\n");
    let mut lines = normalized.lines();
    if lines.next() != Some("---") {
        return (BTreeMap::new(), normalized);
    }

    let mut frontmatter = BTreeMap::new();
    let mut offset = 4usize;
    for line in normalized.lines().skip(1) {
        offset += line.len() + 1;
        if line == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            frontmatter.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    (
        frontmatter,
        normalized
            .get(offset..)
            .map(str::trim_start)
            .unwrap_or_default()
            .to_string(),
    )
}

fn first_descriptive_line(raw: &str) -> &str {
    raw.lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .unwrap_or("Skill")
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_resources_reads_skill_markdown_and_plugin_yaml() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("workspace");
        let resources_dir = root.join("resources");
        fs::create_dir_all(resources_dir.join("prompts")).unwrap();
        fs::create_dir_all(resources_dir.join("skills/reviewer")).unwrap();
        fs::create_dir_all(resources_dir.join("plugins")).unwrap();
        fs::write(
            resources_dir.join("prompts/plan.yaml"),
            "id: plan\ndescription: Plan\ntemplate: body\n",
        )
        .unwrap();
        fs::write(
            resources_dir.join("skills/reviewer/SKILL.md"),
            "---\nname: reviewer\ndescription: Review changes\n---\nBody\n",
        )
        .unwrap();
        fs::write(
            resources_dir.join("plugins/example.yaml"),
            "id: example\ndisplay_name: Example\ncommands:\n  - name: demo\n    description: Demo\n",
        )
        .unwrap();

        let paths = ConfigPaths::discover(&root);
        let loaded = load_resources(&paths).unwrap();
        assert_eq!(loaded.prompts.len(), 1);
        assert_eq!(loaded.skills.len(), 1);
        assert_eq!(loaded.plugins.len(), 1);
        assert_eq!(loaded.skills[0].value.name, "reviewer");
        assert_eq!(loaded.plugins[0].value.id, "example");
    }
}
