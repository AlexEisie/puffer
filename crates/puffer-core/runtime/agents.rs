use crate::{AppState, MessageRole};
use anyhow::{anyhow, bail, Context, Result};
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::{agent_by_id, LoadedResources};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Deserialize)]
struct AgentToolInput {
    description: String,
    prompt: String,
    #[serde(default)]
    subagent_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    run_in_background: bool,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    isolation: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

/// Executes the runtime-backed `Agent` tool by running a nested model turn.
pub(super) fn execute_agent_tool(
    state: &AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    cwd: &Path,
    input: serde_json::Value,
) -> Result<String> {
    let input: AgentToolInput = serde_json::from_value(input).context("invalid Agent input")?;
    if input.prompt.trim().is_empty() {
        bail!("Agent prompt cannot be empty");
    }
    if input.run_in_background {
        bail!("background agent execution is not implemented in this runtime");
    }
    if input.isolation.is_some() {
        bail!("agent isolation is not implemented in this runtime");
    }

    let agent_id = input
        .subagent_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("general-purpose");
    let agent = agent_by_id(resources, agent_id)
        .or_else(|| {
            resources
                .agents
                .iter()
                .find(|item| item.value.id.eq_ignore_ascii_case(agent_id))
        })
        .ok_or_else(|| {
            let available = resources
                .agents
                .iter()
                .map(|item| item.value.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow!("unknown agent `{agent_id}`. Available agents: {available}")
        })?;

    let nested_cwd = resolve_agent_cwd(cwd, input.cwd.as_deref())?;
    let nested_resources = filter_resources_for_agent(resources, &agent.value.tools);
    let mut nested_state = state.clone();
    nested_state.cwd = nested_cwd;
    nested_state.transcript.clear();
    nested_state.push_message(MessageRole::System, agent.value.prompt.trim().to_string());

    if let Some(model) = input
        .model
        .as_deref()
        .or(agent.value.model.as_deref())
        .filter(|value| !value.trim().is_empty())
    {
        let resolved = providers.resolve_model(model);
        nested_state.current_model = Some(model.to_string());
        nested_state.current_provider = resolved
            .map(|descriptor| descriptor.provider.clone())
            .or_else(|| {
                model
                    .split_once('/')
                    .map(|(provider, _)| provider.to_string())
            })
            .or_else(|| state.current_provider.clone());
    }

    let turn = super::execute_user_prompt(
        &mut nested_state,
        &nested_resources,
        providers,
        auth_store,
        &input.prompt,
    )?;

    let mut output = String::new();
    let _ = writeln!(&mut output, "Agent {} completed.", agent.value.id);
    let _ = writeln!(&mut output, "description: {}", input.description.trim());
    if let Some(name) = input
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let _ = writeln!(&mut output, "name: {}", name.trim());
    }
    if !turn.tool_invocations.is_empty() {
        let _ = writeln!(&mut output, "tool_uses: {}", turn.tool_invocations.len());
    }
    let _ = writeln!(&mut output);
    output.push_str(turn.assistant_text.trim());
    Ok(output.trim().to_string())
}

fn resolve_agent_cwd(parent_cwd: &Path, override_cwd: Option<&str>) -> Result<PathBuf> {
    let Some(override_cwd) = override_cwd.filter(|value| !value.trim().is_empty()) else {
        return Ok(parent_cwd.to_path_buf());
    };
    let requested = PathBuf::from(override_cwd.trim());
    let resolved = if requested.is_absolute() {
        requested
    } else {
        parent_cwd.join(requested)
    };
    let metadata = std::fs::metadata(&resolved)
        .with_context(|| format!("agent cwd {} does not exist", resolved.display()))?;
    if !metadata.is_dir() {
        bail!("agent cwd {} is not a directory", resolved.display());
    }
    Ok(resolved)
}

fn filter_resources_for_agent(resources: &LoadedResources, tools: &[String]) -> LoadedResources {
    let mut filtered = resources.clone();
    let wildcard = tools.is_empty() || tools.iter().any(|tool| tool == "*");
    filtered.tools.retain(|tool| {
        if tool.value.id.eq_ignore_ascii_case("Agent") {
            return false;
        }
        wildcard
            || tools
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&tool.value.id))
    });
    filtered
}
