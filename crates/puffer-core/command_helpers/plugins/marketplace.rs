use anyhow::{anyhow, bail, Context, Result};
use puffer_config::{ensure_workspace_dirs, ConfigPaths};
use puffer_resources::{
    AgentSpec, LoadedResources, LspServerSpec, McpServerSpec, PluginCommandSpec, PluginSpec,
    SourceKind,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use super::support::{
    format_plugin_counts, is_disabled_placeholder, plugin_description,
    source_kind_label as support_source_kind_label,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum MarketplaceSource {
    File { path: String },
    Directory { path: String },
    Url { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct KnownMarketplace {
    source: MarketplaceSource,
    cache_path: PathBuf,
    last_updated_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MarketplaceOwner {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MarketplaceMetadata {
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MarketplaceManifest {
    name: String,
    #[serde(default)]
    owner: MarketplaceOwner,
    #[serde(default)]
    metadata: MarketplaceMetadata,
    #[serde(default)]
    plugins: Vec<MarketplacePluginEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MarketplacePluginEntry {
    #[serde(alias = "id")]
    name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    commands: Vec<PluginCommandSpec>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    agents: Vec<AgentSpec>,
    #[serde(default)]
    mcp_servers: Vec<McpServerSpec>,
    #[serde(default)]
    lsp_servers: Vec<LspServerSpec>,
}

/// Carries one marketplace-resolved plugin manifest plus the user-facing source label.
#[derive(Debug, Clone)]
pub(super) struct ResolvedMarketplacePlugin {
    pub plugin: PluginSpec,
    pub raw: String,
    pub source_display: String,
}

/// Adds one plugin marketplace from a local file, directory, or HTTP(S) URL.
pub(super) fn add_marketplace(paths: &ConfigPaths, source_text: &str) -> Result<String> {
    let source = parse_marketplace_source(paths, source_text)?;
    let (manifest, known) = refresh_marketplace(paths, &source, None)?;
    let mut marketplaces = load_known_marketplaces(paths)?;
    let action = if marketplaces
        .insert(manifest.name.clone(), known.clone())
        .is_some()
    {
        "Updated"
    } else {
        "Added"
    };
    save_known_marketplaces(paths, &marketplaces)?;
    Ok(format!(
        "{action} plugin marketplace `{}`.\nsource={}\nplugins={}\ncache={}",
        manifest.name,
        known.source.display_label(),
        manifest.plugins.len(),
        known.cache_path.display()
    ))
}

/// Removes one previously added plugin marketplace without touching installed workspace copies.
pub(super) fn remove_marketplace(paths: &ConfigPaths, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Usage: /plugin marketplace remove <name>");
    }
    let mut marketplaces = load_known_marketplaces(paths)?;
    let Some(known) = marketplaces.remove(name) else {
        bail!("Unknown plugin marketplace `{name}`.");
    };
    save_known_marketplaces(paths, &marketplaces)?;
    if known.cache_path.exists() {
        fs::remove_file(&known.cache_path).with_context(|| {
            format!(
                "failed to remove cached marketplace {}",
                known.cache_path.display()
            )
        })?;
    }
    Ok(format!(
        "Removed plugin marketplace `{name}`.\nsource={}",
        known.source.display_label()
    ))
}

/// Refreshes the cached marketplace manifests for one named marketplace or for all known entries.
pub(super) fn update_marketplaces(paths: &ConfigPaths, name: Option<&str>) -> Result<String> {
    let mut marketplaces = load_known_marketplaces(paths)?;
    if marketplaces.is_empty() {
        return Ok("No custom plugin marketplaces are configured.".to_string());
    }
    let targets = if let Some(name) = name.map(str::trim).filter(|value| !value.is_empty()) {
        let known = marketplaces
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("Unknown plugin marketplace `{name}`."))?;
        vec![(name.to_string(), known)]
    } else {
        marketplaces
            .iter()
            .map(|(name, known)| (name.clone(), known.clone()))
            .collect::<Vec<_>>()
    };

    let mut text = String::new();
    let _ = writeln!(&mut text, "Updated plugin marketplaces");
    for (name, known) in targets {
        let (manifest, refreshed) = refresh_marketplace(paths, &known.source, Some(name.as_str()))?;
        marketplaces.insert(name.clone(), refreshed.clone());
        let _ = writeln!(
            &mut text,
            "- {} source={} plugins={} cache={}",
            manifest.name,
            refreshed.source.display_label(),
            manifest.plugins.len(),
            refreshed.cache_path.display()
        );
    }
    save_known_marketplaces(paths, &marketplaces)?;
    Ok(text.trim_end().to_string())
}

/// Renders the builtin and custom plugin marketplace inventory used by `/plugin marketplace`.
pub(super) fn render_plugin_marketplace(
    resources: &LoadedResources,
    paths: &ConfigPaths,
) -> Result<String> {
    let mut builtin_plugins = resources
        .plugins
        .iter()
        .filter(|plugin| {
            plugin.source_info.kind == SourceKind::Builtin
                && !is_disabled_placeholder(&plugin.value)
        })
        .collect::<Vec<_>>();
    builtin_plugins.sort_by(|left, right| left.value.id.cmp(&right.value.id));

    let marketplaces = load_known_marketplaces(paths)?;
    let mut custom_manifests = marketplaces
        .iter()
        .map(|(name, known)| load_cached_manifest(name, known))
        .collect::<Result<Vec<_>>>()?;
    custom_manifests.sort_by(|left, right| left.0.cmp(&right.0));
    let marketplace_plugin_count = custom_manifests
        .iter()
        .map(|(_, _, manifest)| manifest.plugins.len())
        .sum::<usize>();

    let mut text = format!(
        "Plugin marketplace\nbuiltin_plugins={}\ncustom_marketplaces={}\nmarketplace_plugins={}\nUse `/plugin install <id>` for builtin plugins, `/plugin install <id@marketplace>` for marketplace plugins, `/plugin marketplace add <path|url>`, `/plugin marketplace remove <name>`, or `/plugin marketplace update [name]`.\n",
        builtin_plugins.len(),
        custom_manifests.len(),
        marketplace_plugin_count
    );

    if builtin_plugins.is_empty() {
        text.push_str("\nBuiltin plugins:\n<none>\n");
    } else {
        text.push_str("\nBuiltin plugins:\n");
        for plugin in builtin_plugins {
            let _ = writeln!(
                &mut text,
                "- {} [{}] {} • {}",
                plugin.value.id,
                support_source_kind_label(plugin.source_info.kind),
                plugin_description(&plugin.value),
                format_plugin_counts(&plugin.value)
            );
        }
    }

    if custom_manifests.is_empty() {
        text.push_str("\nCustom marketplaces:\n<none>");
        return Ok(text.trim_end().to_string());
    }

    text.push_str("\nCustom marketplaces:\n");
    for (name, known, manifest) in custom_manifests {
        let _ = writeln!(
            &mut text,
            "- {} owner={} source={} plugins={} updated_at_ms={} cache={}",
            name,
            display_owner(&manifest.owner),
            known.source.display_label(),
            manifest.plugins.len(),
            known.last_updated_ms,
            known.cache_path.display()
        );
        if !manifest.metadata.description.trim().is_empty() {
            let _ = writeln!(
                &mut text,
                "  description: {}",
                manifest.metadata.description
            );
        }
        if !manifest.metadata.version.trim().is_empty() {
            let _ = writeln!(&mut text, "  version: {}", manifest.metadata.version);
        }
        for plugin in &manifest.plugins {
            let mut line = format!("- {}@{}", plugin.name, name);
            if !plugin.display_name.trim().is_empty() && plugin.display_name != plugin.name {
                line.push_str(&format!(" ({})", plugin.display_name));
            }
            if !plugin.description.trim().is_empty() {
                line.push_str(&format!(" {}", plugin.description));
            }
            if let Some(source) = plugin.source.as_deref() {
                line.push_str(&format!(" • source={source}"));
            } else {
                line.push_str(&format!(
                    " • {}",
                    format_plugin_counts(&plugin.inline_plugin_spec())
                ));
            }
            let _ = writeln!(&mut text, "  {line}");
        }
    }

    Ok(text.trim_end().to_string())
}

/// Resolves a marketplace plugin reference like `name@marketplace` into an installable manifest.
pub(super) fn resolve_marketplace_plugin(
    paths: &ConfigPaths,
    plugin_ref: &str,
) -> Result<Option<ResolvedMarketplacePlugin>> {
    let plugin_ref = plugin_ref.trim();
    if plugin_ref.is_empty() {
        return Ok(None);
    }
    let (plugin_name, requested_marketplace) = parse_plugin_ref(plugin_ref);
    let marketplaces = load_known_marketplaces(paths)?;
    if marketplaces.is_empty() {
        return Ok(None);
    }

    if let Some(marketplace_name) = requested_marketplace {
        let known = marketplaces
            .get(marketplace_name)
            .cloned()
            .ok_or_else(|| anyhow!("Unknown plugin marketplace `{marketplace_name}`."))?;
        let manifest = load_cached_manifest(marketplace_name, &known)?.2;
        let entry = manifest
            .plugins
            .iter()
            .find(|entry| entry.name == plugin_name)
            .cloned()
            .ok_or_else(|| {
                anyhow!("Marketplace `{marketplace_name}` does not provide plugin `{plugin_name}`.")
            })?;
        return Ok(Some(resolve_marketplace_entry(
            marketplace_name,
            &known,
            &entry,
        )?));
    }

    let mut matches = Vec::new();
    for (marketplace_name, known) in marketplaces {
        let manifest = load_cached_manifest(&marketplace_name, &known)?.2;
        if let Some(entry) = manifest
            .plugins
            .iter()
            .find(|entry| entry.name == plugin_name)
            .cloned()
        {
            matches.push((marketplace_name, known, entry));
        }
    }

    match matches.len() {
        0 => Ok(None),
        1 => {
            let (marketplace_name, known, entry) = matches.remove(0);
            Ok(Some(resolve_marketplace_entry(
                &marketplace_name,
                &known,
                &entry,
            )?))
        }
        _ => {
            let selectors = matches
                .iter()
                .map(|(marketplace_name, _, _)| format!("{plugin_name}@{marketplace_name}"))
                .collect::<Vec<_>>()
                .join(", ");
            bail!("Multiple marketplaces provide `{plugin_name}`. Use one of: {selectors}")
        }
    }
}

fn parse_marketplace_source(paths: &ConfigPaths, source_text: &str) -> Result<MarketplaceSource> {
    let trimmed = source_text.trim();
    if trimmed.is_empty() {
        bail!("Usage: /plugin marketplace add <path|url>");
    }
    if let Ok(url) = Url::parse(trimmed) {
        if matches!(url.scheme(), "http" | "https") {
            return Ok(MarketplaceSource::Url {
                url: url.to_string(),
            });
        }
    }

    let candidate = if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        paths.workspace_root.join(trimmed)
    };
    let resolved = candidate.canonicalize().with_context(|| {
        format!(
            "plugin marketplace source `{trimmed}` must be an existing file, directory, or http(s) URL"
        )
    })?;
    if resolved.is_dir() {
        Ok(MarketplaceSource::Directory {
            path: resolved.display().to_string(),
        })
    } else {
        Ok(MarketplaceSource::File {
            path: resolved.display().to_string(),
        })
    }
}

fn refresh_marketplace(
    paths: &ConfigPaths,
    source: &MarketplaceSource,
    expected_name: Option<&str>,
) -> Result<(MarketplaceManifest, KnownMarketplace)> {
    let raw = load_marketplace_source(source)?;
    let manifest = parse_marketplace_manifest(&raw)?;
    if let Some(expected_name) = expected_name {
        if manifest.name != expected_name {
            bail!(
                "Marketplace source now reports `{}` instead of `{expected_name}`. Remove and re-add it if you intend to rename the marketplace.",
                manifest.name
            );
        }
    }

    let state = marketplace_state_paths(paths);
    ensure_workspace_dirs(paths)?;
    fs::create_dir_all(&state.root)?;
    fs::create_dir_all(&state.cache_dir)?;
    let cache_path = state.cache_dir.join(format!(
        "{}.yaml",
        sanitize_marketplace_name(&manifest.name)
    ));
    fs::write(&cache_path, raw)
        .with_context(|| format!("failed to write marketplace cache {}", cache_path.display()))?;
    Ok((
        manifest,
        KnownMarketplace {
            source: source.clone(),
            cache_path,
            last_updated_ms: now_ms(),
        },
    ))
}

fn load_known_marketplaces(paths: &ConfigPaths) -> Result<BTreeMap<String, KnownMarketplace>> {
    let state = marketplace_state_paths(paths);
    if !state.known_file.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = fs::read_to_string(&state.known_file).with_context(|| {
        format!(
            "failed to read marketplace registry {}",
            state.known_file.display()
        )
    })?;
    let parsed = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse marketplace registry {}",
            state.known_file.display()
        )
    })?;
    Ok(parsed)
}

fn save_known_marketplaces(
    paths: &ConfigPaths,
    marketplaces: &BTreeMap<String, KnownMarketplace>,
) -> Result<()> {
    let state = marketplace_state_paths(paths);
    ensure_workspace_dirs(paths)?;
    fs::create_dir_all(&state.root)?;
    let raw = serde_json::to_string_pretty(marketplaces)?;
    fs::write(&state.known_file, raw).with_context(|| {
        format!(
            "failed to write marketplace registry {}",
            state.known_file.display()
        )
    })
}

fn load_cached_manifest(
    name: &str,
    known: &KnownMarketplace,
) -> Result<(String, KnownMarketplace, MarketplaceManifest)> {
    let raw = fs::read_to_string(&known.cache_path).with_context(|| {
        format!(
            "failed to read cached marketplace `{name}` from {}",
            known.cache_path.display()
        )
    })?;
    let manifest = parse_marketplace_manifest(&raw)?;
    Ok((name.to_string(), known.clone(), manifest))
}

fn load_marketplace_source(source: &MarketplaceSource) -> Result<String> {
    match source {
        MarketplaceSource::File { path } => {
            let path = PathBuf::from(path);
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read marketplace file {}", path.display()))?;
            Ok(raw)
        }
        MarketplaceSource::Directory { path } => {
            let dir = PathBuf::from(path);
            let manifest_path = find_marketplace_manifest_file(&dir)?;
            let raw = fs::read_to_string(&manifest_path).with_context(|| {
                format!(
                    "failed to read marketplace manifest {}",
                    manifest_path.display()
                )
            })?;
            Ok(raw)
        }
        MarketplaceSource::Url { url } => {
            let response = Client::new()
                .get(url)
                .send()
                .with_context(|| format!("failed to fetch marketplace {url}"))?
                .error_for_status()
                .with_context(|| format!("marketplace request failed for {url}"))?;
            let raw = response
                .text()
                .with_context(|| format!("failed to read marketplace body for {url}"))?;
            Ok(raw)
        }
    }
}

fn parse_marketplace_manifest(raw: &str) -> Result<MarketplaceManifest> {
    let manifest: MarketplaceManifest =
        serde_yaml::from_str(raw).context("failed to parse plugin marketplace manifest")?;
    if manifest.name.trim().is_empty() {
        bail!("plugin marketplace manifest is missing `name`");
    }
    Ok(manifest)
}

fn find_marketplace_manifest_file(dir: &Path) -> Result<PathBuf> {
    for name in ["marketplace.yaml", "marketplace.yml", "marketplace.json"] {
        let path = dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    bail!(
        "directory {} does not contain marketplace.yaml, marketplace.yml, or marketplace.json",
        dir.display()
    )
}

fn resolve_marketplace_entry(
    marketplace_name: &str,
    known: &KnownMarketplace,
    entry: &MarketplacePluginEntry,
) -> Result<ResolvedMarketplacePlugin> {
    if let Some(source) = entry
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let resolved = resolve_plugin_source(&known.source, source)?;
        let (raw, source_display) = match resolved {
            ResolvedPluginSource::Local(path) => {
                let manifest_path = if path.is_dir() {
                    find_plugin_manifest_file(&path)?
                } else {
                    path
                };
                (
                    fs::read_to_string(&manifest_path).with_context(|| {
                        format!("failed to read plugin manifest {}", manifest_path.display())
                    })?,
                    manifest_path.display().to_string(),
                )
            }
            ResolvedPluginSource::Remote(url) => {
                let raw = Client::new()
                    .get(url.clone())
                    .send()
                    .with_context(|| format!("failed to fetch plugin manifest {}", url))?
                    .error_for_status()
                    .with_context(|| format!("plugin request failed for {}", url))?
                    .text()
                    .with_context(|| format!("failed to read plugin body {}", url))?;
                (raw, url.to_string())
            }
        };
        let mut plugin: PluginSpec = serde_yaml::from_str(&raw)
            .context("failed to parse plugin manifest from marketplace source")?;
        if plugin.id.trim().is_empty() {
            plugin.id = entry.name.clone();
        }
        if plugin.id != entry.name {
            bail!(
                "plugin source for `{}` resolved to manifest id `{}`",
                entry.name,
                plugin.id
            );
        }
        if plugin.display_name.trim().is_empty() {
            plugin.display_name = fallback_display_name(entry);
        }
        return Ok(ResolvedMarketplacePlugin {
            plugin,
            raw,
            source_display: format!("marketplace `{marketplace_name}` ({source_display})"),
        });
    }

    let plugin = entry.inline_plugin_spec();
    let raw = serde_yaml::to_string(&plugin)?;
    Ok(ResolvedMarketplacePlugin {
        plugin,
        raw,
        source_display: format!("marketplace `{marketplace_name}`"),
    })
}

fn resolve_plugin_source(
    marketplace_source: &MarketplaceSource,
    plugin_source: &str,
) -> Result<ResolvedPluginSource> {
    if let Ok(url) = Url::parse(plugin_source) {
        if matches!(url.scheme(), "http" | "https") {
            return Ok(ResolvedPluginSource::Remote(url));
        }
    }
    match marketplace_source {
        MarketplaceSource::File { path } => {
            let base = PathBuf::from(path)
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| anyhow!("marketplace source {} has no parent", path))?;
            resolve_local_plugin_source(base, plugin_source)
        }
        MarketplaceSource::Directory { path } => {
            resolve_local_plugin_source(PathBuf::from(path), plugin_source)
        }
        MarketplaceSource::Url { url } => {
            let base = Url::parse(url)?;
            Ok(ResolvedPluginSource::Remote(base.join(plugin_source)?))
        }
    }
}

fn resolve_local_plugin_source(base: PathBuf, plugin_source: &str) -> Result<ResolvedPluginSource> {
    let path = Path::new(plugin_source);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let resolved = candidate.canonicalize().with_context(|| {
        format!(
            "plugin source `{plugin_source}` does not exist under {}",
            base.display()
        )
    })?;
    Ok(ResolvedPluginSource::Local(resolved))
}

fn find_plugin_manifest_file(dir: &Path) -> Result<PathBuf> {
    for name in ["plugin.yaml", "plugin.yml", "plugin.json"] {
        let path = dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    bail!(
        "directory {} does not contain plugin.yaml, plugin.yml, or plugin.json",
        dir.display()
    )
}

fn parse_plugin_ref(plugin_ref: &str) -> (&str, Option<&str>) {
    match plugin_ref.rsplit_once('@') {
        Some((plugin_name, marketplace_name))
            if !plugin_name.is_empty() && !marketplace_name.is_empty() =>
        {
            (plugin_name, Some(marketplace_name))
        }
        _ => (plugin_ref, None),
    }
}

fn display_owner(owner: &MarketplaceOwner) -> &str {
    if owner.name.trim().is_empty() {
        "<unknown>"
    } else {
        owner.name.trim()
    }
}

fn fallback_display_name(entry: &MarketplacePluginEntry) -> String {
    if entry.display_name.trim().is_empty() {
        entry.name.clone()
    } else {
        entry.display_name.clone()
    }
}

fn marketplace_state_paths(paths: &ConfigPaths) -> MarketplaceStatePaths {
    let root = paths.user_config_dir.join("plugins");
    MarketplaceStatePaths {
        known_file: root.join("known_marketplaces.json"),
        cache_dir: root.join("marketplaces"),
        root,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn sanitize_marketplace_name(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '-',
        })
        .collect()
}

struct MarketplaceStatePaths {
    root: PathBuf,
    known_file: PathBuf,
    cache_dir: PathBuf,
}

enum ResolvedPluginSource {
    Local(PathBuf),
    Remote(Url),
}

impl MarketplaceSource {
    fn display_label(&self) -> String {
        match self {
            Self::File { path } => format!("file:{path}"),
            Self::Directory { path } => format!("dir:{path}"),
            Self::Url { url } => url.clone(),
        }
    }
}

impl MarketplacePluginEntry {
    fn inline_plugin_spec(&self) -> PluginSpec {
        PluginSpec {
            id: self.name.clone(),
            display_name: fallback_display_name(self),
            description: self.description.clone(),
            commands: self.commands.clone(),
            skills: self.skills.clone(),
            agents: self.agents.clone(),
            mcp_servers: self.mcp_servers.clone(),
            lsp_servers: self.lsp_servers.clone(),
        }
    }
}
