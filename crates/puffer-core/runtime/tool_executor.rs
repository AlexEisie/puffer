use super::browser_auto_review::{
    build_browser_auto_review_request, run_browser_auto_review, BrowserAutoReviewRuntimeResult,
    BrowserAutoReviewSessionTargeting,
};
use super::agents::execute_agent_tool;
use super::claude_tools::{self, ProviderToolContext};
use super::hook_support::{run_tool_end_hooks, run_tool_start_hooks};
use super::local_tools::{
    enrich_browser_permission_input, read_current_tab_context, BrowserCurrentTabStatus,
};
use super::permission_prompt::{
    build_permission_prompt_request, prompt_for_permission, PermissionPromptAction,
};
use super::structured_output_support::{
    requested_structured_output_definition_for_request, StructuredOutputConfig,
};
use super::RequestToolFilter;
use crate::permissions::browser_action::{
    attach_browser_permission_value, browser_permission_value_for_tool_call,
};
use crate::permissions::browser_grants::BrowserGrantScopeKind;
use crate::permissions::browser_target::browser_permission_context_for_tool;
use crate::permissions::{
    load_runtime_permission_context_with_inputs, FilesystemPermissionPolicy,
    RuntimePermissionInputs, ToolPermissionBehavior,
};
use crate::tool_names::canonical_tool_name;
use crate::AppState;
use anyhow::{anyhow, Result};
use puffer_provider_openai::OpenAIRequestConfig;
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::LoadedResources;
use puffer_tools::{ToolExecutionResult, ToolOutput, ToolRegistry};
use puffer_transport_anthropic::AnthropicRequestConfig;
use serde_json::Value;
use std::path::Path;

const BROWSER_REVIEW_METADATA_KEY: &str = "__pufferBrowserReview";

/// Identifies which provider loop is currently executing a tool call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ToolExecutionBackend<'a> {
    Anthropic {
        request_config: &'a AnthropicRequestConfig,
        structured_output: Option<&'a StructuredOutputConfig>,
    },
    OpenAi {
        request_config: &'a OpenAIRequestConfig,
        structured_output: Option<&'a StructuredOutputConfig>,
    },
}

/// Executes one tool call with access to the full conversation runtime context.
pub(super) fn execute_tool_call(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    registry: &ToolRegistry,
    model_id: &str,
    cwd: &Path,
    backend: ToolExecutionBackend<'_>,
    tool_filter: Option<&RequestToolFilter>,
    tool_id: &str,
    input: Value,
) -> Result<ToolExecutionResult> {
    let structured_output = match backend {
        ToolExecutionBackend::Anthropic {
            structured_output, ..
        }
        | ToolExecutionBackend::OpenAi {
            structured_output, ..
        } => structured_output,
    };
    let definition = match registry.definition(tool_id) {
        Some(definition) => definition.clone(),
        None => requested_structured_output_definition_for_request(registry, structured_output)?
            .filter(|definition| definition.id == tool_id)
            .ok_or_else(|| anyhow!("unknown tool {tool_id}"))?,
    };
    let input = prepare_browser_permission_input(state, cwd, &definition, input)?;
    let permission_context = load_runtime_permission_context_with_inputs(
        cwd,
        resources,
        state,
        RuntimePermissionInputs {
            request_tool_filter: tool_filter.cloned(),
        },
    )?;
    let filesystem_policy = permission_context.derived_policy().filesystem().clone();
    let permission_decision = permission_context.decision_for_tool_call(&definition, &input);
    match permission_decision.behavior {
        ToolPermissionBehavior::Allow => {}
        ToolPermissionBehavior::Deny => {
            return Ok(blocked_runtime_tool(
                tool_id,
                ToolPermissionBehavior::Deny,
                permission_decision.reason,
            ));
        }
        ToolPermissionBehavior::Ask => {
            match resolve_ask_behavior(
                state,
                resources,
                providers,
                auth_store,
                cwd,
                tool_filter,
                &definition,
                &input,
                permission_decision.reason.as_deref(),
                &permission_context.effective_profile().current_session_id,
                &permission_context.effective_profile().workspace_roots,
            )? {
                AskResolution::AllowOnce => {}
                AskResolution::AllowSession => {}
                AskResolution::Deny => {
                    return Ok(blocked_runtime_tool(
                        tool_id,
                        ToolPermissionBehavior::Deny,
                        Some("permission denied by user".to_string()),
                    ));
                }
            }
        }
    }
    let provider_context = match backend {
        ToolExecutionBackend::Anthropic {
            request_config,
            structured_output,
        } => ProviderToolContext::Anthropic {
            request_config,
            model_id,
            structured_output,
        },
        ToolExecutionBackend::OpenAi {
            request_config,
            structured_output,
        } => ProviderToolContext::OpenAI {
            request_config,
            model_id,
            structured_output,
        },
    };
    let hook_input = input.clone();
    run_tool_start_hooks(resources, cwd, tool_id, &hook_input);
    let result = if definition.handler == "runtime:agent" {
        let output =
            execute_agent_tool(state, resources, providers, auth_store, cwd, input.clone())?;
        successful_runtime_tool(tool_id, output)
    } else {
        claude_tools::execute_tool(
            state,
            resources,
            registry,
            &definition,
            cwd,
            &filesystem_policy,
            input.clone(),
            provider_context,
        )?
    };
    remember_browser_target(state, &definition, &input);
    run_tool_end_hooks(
        resources,
        cwd,
        tool_id,
        &hook_input,
        result.success,
        &result.output.stdout,
        &result.output.stderr,
    );
    Ok(result)
}

fn successful_runtime_tool(tool_id: &str, stdout: String) -> ToolExecutionResult {
    ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: true,
        output: ToolOutput {
            stdout,
            stderr: String::new(),
            metadata: Value::Null,
        },
    }
}

/// Returns `true` when a tool can be executed without `&mut AppState`.
///
/// These tools perform pure IO (filesystem reads, HTTP requests, process spawning)
/// and don't read or write any mutable application state. This classification
/// enables parallel execution when the model requests multiple tool calls.
pub(super) fn is_parallel_safe_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        "Glob" | "Grep" | "WebFetch" | "WebSearch" | "ToolSearch" | "Skill" | "Bash"
    )
}

/// The result of pre-resolving permission for a tool call.
pub(super) enum PermissionOutcome {
    /// Tool execution is permitted.
    Allowed(FilesystemPermissionPolicy),
    /// Tool execution was denied; carry the pre-built denial result.
    Denied(ToolExecutionResult),
}

/// Pre-resolves permission for one tool call.
///
/// This is separated from `execute_tool_call` so that permissions can be
/// resolved serially (may prompt the user) before tools are dispatched in
/// parallel.
pub(super) fn resolve_tool_permission(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    registry: &ToolRegistry,
    cwd: &Path,
    tool_id: &str,
    input: &Value,
    tool_filter: Option<&super::RequestToolFilter>,
) -> Result<PermissionOutcome> {
    let definition = match registry.definition(tool_id) {
        Some(d) => d.clone(),
        None => {
            return Ok(PermissionOutcome::Denied(blocked_runtime_tool(
                tool_id,
                ToolPermissionBehavior::Deny,
                Some(format!("unknown tool {tool_id}")),
            )));
        }
    };
    let input = prepare_browser_permission_input(state, cwd, &definition, input.clone())?;
    let permission_context = load_runtime_permission_context_with_inputs(
        cwd,
        resources,
        state,
        RuntimePermissionInputs {
            request_tool_filter: tool_filter.cloned(),
        },
    )?;
    let permission_decision = permission_context.decision_for_tool_call(&definition, &input);
    match permission_decision.behavior {
        ToolPermissionBehavior::Allow => Ok(PermissionOutcome::Allowed(
            permission_context.derived_policy().filesystem().clone(),
        )),
        ToolPermissionBehavior::Deny => Ok(PermissionOutcome::Denied(blocked_runtime_tool(
            tool_id,
            ToolPermissionBehavior::Deny,
            permission_decision.reason,
        ))),
        ToolPermissionBehavior::Ask => {
            match resolve_ask_behavior(
                state,
                resources,
                providers,
                auth_store,
                cwd,
                tool_filter,
                &definition,
                &input,
                permission_decision.reason.as_deref(),
                &permission_context.effective_profile().current_session_id,
                &permission_context.effective_profile().workspace_roots,
            )? {
                AskResolution::AllowOnce => Ok(PermissionOutcome::Allowed(
                    permission_context.derived_policy().filesystem().clone(),
                )),
                AskResolution::AllowSession => {
                    remember_browser_target(state, &definition, &input);
                    Ok(PermissionOutcome::Allowed(runtime_filesystem_policy(
                        cwd,
                        resources,
                        state,
                        tool_filter,
                    )?))
                }
                AskResolution::Deny => Ok(PermissionOutcome::Denied(blocked_runtime_tool(
                    tool_id,
                    ToolPermissionBehavior::Deny,
                    Some("permission denied by user".to_string()),
                ))),
            }
        }
    }
}

enum AskResolution {
    AllowOnce,
    AllowSession,
    Deny,
}

fn resolve_ask_behavior(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &mut AuthStore,
    cwd: &Path,
    tool_filter: Option<&RequestToolFilter>,
    definition: &puffer_tools::ToolDefinition,
    input: &Value,
    reason: Option<&str>,
    current_session_id: &str,
    workspace_roots: &[std::path::PathBuf],
) -> Result<AskResolution> {
    let carries_browser_permission = browser_permission_value_for_tool_call(&definition.id, input).is_some();
    let browser_session_grant = carries_browser_permission.then(|| {
        browser_grant_scope_for_prompt_action(
            definition,
            input,
            current_session_id,
            workspace_roots,
        )
    });
    let prompt_request = build_permission_prompt_request(
        definition,
        input,
        reason,
        current_session_id,
        workspace_roots,
    );
    if let Some(browser) = prompt_request.browser.as_ref() {
        let resolved_root_session_id = input
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "current")
            .unwrap_or(current_session_id)
            .to_string();
        let session_targeting = if resolved_root_session_id == current_session_id {
            BrowserAutoReviewSessionTargeting::CurrentSession
        } else {
            BrowserAutoReviewSessionTargeting::ExplicitSession
        };
        let review_request = build_browser_auto_review_request(
            &definition.id,
            input,
            prompt_request.summary.clone(),
            prompt_request.reason.clone(),
            browser,
            resolved_root_session_id,
            session_targeting,
            browser_session_grant.unwrap_or(BrowserGrantScopeKind::AllowOnce),
        );
        match run_browser_auto_review(state, resources, providers, auth_store, &review_request) {
            BrowserAutoReviewRuntimeResult::AllowOnce => return Ok(AskResolution::AllowOnce),
            BrowserAutoReviewRuntimeResult::AllowSession => {
                state.allow_browser_permission_for_tool_call(
                    definition,
                    input,
                    browser_session_grant.unwrap_or(BrowserGrantScopeKind::AllowOnce),
                );
                return Ok(AskResolution::AllowSession);
            }
            BrowserAutoReviewRuntimeResult::Deny => return Ok(AskResolution::Deny),
            BrowserAutoReviewRuntimeResult::NeedsUser
            | BrowserAutoReviewRuntimeResult::Unavailable => {}
        }
    }
    match prompt_for_permission(prompt_request) {
        PermissionPromptAction::AllowOnce => Ok(AskResolution::AllowOnce),
        PermissionPromptAction::AllowSession => {
            if carries_browser_permission {
                state.allow_browser_permission_for_tool_call(
                    definition,
                    input,
                    browser_session_grant.unwrap_or(BrowserGrantScopeKind::AllowOnce),
                );
            } else {
                state.allow_permission_for_tool_call(definition, input);
            }
            Ok(AskResolution::AllowSession)
        }
        PermissionPromptAction::AllowAllSession => {
            if carries_browser_permission {
                state.allow_browser_permission_for_tool_call(
                    definition,
                    input,
                    browser_session_grant.unwrap_or(BrowserGrantScopeKind::AllowOnce),
                );
            } else {
                state.grant_all_tools_for_session();
            }
            Ok(AskResolution::AllowSession)
        }
        PermissionPromptAction::Deny => Ok(AskResolution::Deny),
    }
}

fn browser_grant_scope_for_prompt_action(
    definition: &puffer_tools::ToolDefinition,
    input: &Value,
    current_session_id: &str,
    workspace_roots: &[std::path::PathBuf],
) -> BrowserGrantScopeKind {
    let context = browser_permission_context_for_tool(
        &definition.id,
        input,
        current_session_id,
        workspace_roots,
    );
    crate::permissions::browser_grants::suggested_browser_grant_scope(&context)
}

fn prepare_browser_permission_input(
    state: &AppState,
    cwd: &Path,
    definition: &puffer_tools::ToolDefinition,
    mut input: Value,
) -> Result<Value> {
    if let Some(browser_input) = browser_permission_value_for_tool_call(&definition.id, &input) {
        let raw_action = browser_input
            .get("action")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_ascii_lowercase);
        let explicit_requested_url = browser_input
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let enriched = enrich_browser_permission_input(cwd, &state.session.id, browser_input)?;
        let current_session_id = state.session.id.to_string();
        let root_session_id = enriched
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "current")
            .map(ToString::to_string)
            .unwrap_or(current_session_id);
        let enriched = apply_browser_url_fallback(state, cwd, &root_session_id, enriched)?;
        let current_tab_url = if explicit_requested_url.is_some()
            && matches!(raw_action.as_deref(), Some("open" | "new"))
        {
            read_current_tab_context(cwd, &root_session_id)
                .ok()
                .and_then(|context| context.url)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("about:blank"))
        } else {
            None
        };
        let enriched = attach_browser_review_metadata(
            enriched,
            explicit_requested_url.as_deref(),
            current_tab_url.as_deref(),
        );
        if canonical_tool_name(&definition.id) == "browser" {
            return Ok(enriched);
        }
        let _ = attach_browser_permission_value(&mut input, enriched);
        return Ok(input);
    }
    Ok(input)
}

fn attach_browser_review_metadata(
    input: Value,
    explicit_requested_url: Option<&str>,
    current_tab_url: Option<&str>,
) -> Value {
    let Some(payload) = input.as_object() else {
        return input;
    };
    let effective_url = payload
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let url_source = match (explicit_requested_url, effective_url.as_deref()) {
        (Some(requested), Some(effective)) if requested.eq_ignore_ascii_case(effective) => {
            "explicit"
        }
        (Some(_), Some(_)) => "current_tab",
        (Some(_), None) => "none",
        (None, Some(_)) => "current_tab",
        (None, None) => "none",
    };
    let mut enriched = payload.clone();
    enriched.insert(
        BROWSER_REVIEW_METADATA_KEY.to_string(),
        serde_json::json!({
            "urlSource": url_source,
            "requestedUrl": explicit_requested_url,
            "currentTabUrl": current_tab_url,
        }),
    );
    Value::Object(enriched)
}

fn apply_browser_url_fallback(
    state: &AppState,
    cwd: &Path,
    current_session_id: &str,
    input: Value,
) -> Result<Value> {
    let Some(payload) = input.as_object() else {
        return Ok(input);
    };
    if payload
        .get("url")
        .and_then(Value::as_str)
        .is_some_and(|url| !url.trim().is_empty() && !url.eq_ignore_ascii_case("about:blank"))
    {
        return Ok(input);
    }

    let root_session_id = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "current")
        .unwrap_or(current_session_id);
    let tab_id = payload.get("tabId").and_then(Value::as_str).map(str::trim);
    if let Ok(context) = read_current_tab_context(cwd, root_session_id) {
        if matches!(context.status, BrowserCurrentTabStatus::Available) {
            if let Some(url) = context.url.as_deref() {
                if !url.trim().is_empty() && !url.eq_ignore_ascii_case("about:blank") {
                    let mut enriched = payload.clone();
                    enriched.insert("url".to_string(), Value::String(url.trim().to_string()));
                    return Ok(Value::Object(enriched));
                }
            }
        }
    }
    let remembered = tab_id
        .and_then(|tab_id| state.remembered_browser_url(root_session_id, Some(tab_id)))
        .or_else(|| state.remembered_browser_url(root_session_id, None));
    let Some(url) = remembered else {
        return Ok(input);
    };

    let mut enriched = payload.clone();
    enriched.insert("url".to_string(), Value::String(url.to_string()));
    Ok(Value::Object(enriched))
}

fn remember_browser_target(
    state: &mut AppState,
    definition: &puffer_tools::ToolDefinition,
    input: &Value,
) {
    let Some(payload) = browser_permission_value_for_tool_call(&definition.id, input) else {
        return;
    };
    let Some(payload) = payload.as_object() else {
        return;
    };
    let Some(url) = payload.get("url").and_then(Value::as_str) else {
        return;
    };
    let default_session_id = state.session.id.to_string();
    let root_session_id = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "current")
        .unwrap_or(default_session_id.as_str());
    let tab_id = payload.get("tabId").and_then(Value::as_str).map(str::trim);
    state.remember_browser_url(root_session_id, tab_id, url);
    state.remember_browser_url(root_session_id, None, url);
}

/// Loads the effective filesystem policy for the current runtime permission state.
fn runtime_filesystem_policy(
    cwd: &Path,
    resources: &LoadedResources,
    state: &AppState,
    tool_filter: Option<&RequestToolFilter>,
) -> Result<FilesystemPermissionPolicy> {
    let permission_context = load_runtime_permission_context_with_inputs(
        cwd,
        resources,
        state,
        RuntimePermissionInputs {
            request_tool_filter: tool_filter.cloned(),
        },
    )?;
    Ok(permission_context.derived_policy().filesystem().clone())
}

fn blocked_runtime_tool(
    tool_id: &str,
    behavior: ToolPermissionBehavior,
    reason: Option<String>,
) -> ToolExecutionResult {
    let prefix = match behavior {
        ToolPermissionBehavior::Allow => "Allowed",
        ToolPermissionBehavior::Ask => "Permission required",
        ToolPermissionBehavior::Deny => "Permission denied",
    };
    let stdout = reason
        .map(|reason| format!("{prefix}: {reason}"))
        .unwrap_or_else(|| prefix.to_string());
    ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: false,
        output: ToolOutput {
            stdout,
            stderr: String::new(),
            metadata: Value::Null,
        },
    }
}
