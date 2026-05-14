use crate::permissions::browser_action::browser_permission_value_for_tool_call;
use crate::permissions::browser_target::{
    browser_permission_context_for_tool, BrowserActionCategory, BrowserTargetClass,
};
use crate::tool_names::canonical_tool_name;
use puffer_tools::ToolDefinition;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::path::PathBuf;

/// Describes one runtime permission request that may need user approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPromptRequest {
    pub tool_id: String,
    pub summary: String,
    pub reason: Option<String>,
    pub browser: Option<BrowserPermissionPromptPayload>,
}

/// Carries the structured Browser approval payload shared across all Browser prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserPermissionPromptPayload {
    pub source: BrowserPermissionPromptSource,
    pub action_set: BrowserPermissionPromptActionSet,
    pub url: Option<String>,
    pub origin: Option<String>,
    pub host: Option<String>,
    pub target_class: BrowserPermissionPromptTargetClass,
    pub tab_id: Option<String>,
    pub is_cross_session: bool,
}

/// Identifies how one Browser approval request reached the prompt layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserPermissionPromptSource {
    BrowserTool,
    BrowserCliViaShell,
}

/// Identifies one stable Browser action-set bucket shown in permission prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserPermissionPromptActionSet {
    Inspect,
    Navigate,
    Interact,
    Evaluate,
}

/// Identifies one stable Browser target class shown in permission prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserPermissionPromptTargetClass {
    LocalDev,
    WorkspaceFile,
    NonWorkspaceFile,
    DataUrl,
    OpenWeb,
    Unknown,
}

/// Describes how the user responded to a runtime permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionPromptAction {
    AllowOnce,
    AllowSession,
    AllowAllSession,
    Deny,
}

/// Describes one `AskUserQuestion` request that may need user answers.
#[derive(Debug, Clone, PartialEq)]
pub struct UserQuestionPromptRequest {
    pub questions: Value,
}

/// Describes the answers collected for one `AskUserQuestion` request.
#[derive(Debug, Clone, PartialEq)]
pub struct UserQuestionPromptResponse {
    pub answers: Map<String, Value>,
    pub annotations: Map<String, Value>,
}

thread_local! {
    static PERMISSION_PROMPT_HANDLER: RefCell<Option<Box<dyn FnMut(PermissionPromptRequest) -> PermissionPromptAction>>> =
        const { RefCell::new(None) };
    static USER_QUESTION_PROMPT_HANDLER: RefCell<Option<Box<dyn FnMut(UserQuestionPromptRequest) -> UserQuestionPromptResponse>>> =
        const { RefCell::new(None) };
}

/// Runs a closure while the current thread has an active permission prompt handler.
pub fn with_permission_prompt_handler<R>(
    handler: impl FnMut(PermissionPromptRequest) -> PermissionPromptAction + 'static,
    run: impl FnOnce() -> R,
) -> R {
    PERMISSION_PROMPT_HANDLER.with(|slot| {
        let previous = slot.borrow_mut().take();
        *slot.borrow_mut() = Some(Box::new(handler));
        let result = run();
        let _ = slot.borrow_mut().take();
        *slot.borrow_mut() = previous;
        result
    })
}

/// Runs a closure while the current thread can answer `AskUserQuestion` prompts.
pub fn with_user_question_prompt_handler<R>(
    handler: impl FnMut(UserQuestionPromptRequest) -> UserQuestionPromptResponse + 'static,
    run: impl FnOnce() -> R,
) -> R {
    USER_QUESTION_PROMPT_HANDLER.with(|slot| {
        let previous = slot.borrow_mut().take();
        *slot.borrow_mut() = Some(Box::new(handler));
        let result = run();
        let _ = slot.borrow_mut().take();
        *slot.borrow_mut() = previous;
        result
    })
}

pub(crate) fn prompt_for_permission(request: PermissionPromptRequest) -> PermissionPromptAction {
    PERMISSION_PROMPT_HANDLER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(handler) = borrowed.as_mut() else {
            return PermissionPromptAction::Deny;
        };
        handler(request)
    })
}

pub(crate) fn prompt_for_user_question(
    request: UserQuestionPromptRequest,
) -> Option<UserQuestionPromptResponse> {
    USER_QUESTION_PROMPT_HANDLER.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        borrowed.as_mut().map(|handler| handler(request))
    })
}

pub(crate) fn build_permission_prompt_request(
    definition: &ToolDefinition,
    input: &Value,
    reason: Option<&str>,
    current_session_id: &str,
    workspace_roots: &[PathBuf],
) -> PermissionPromptRequest {
    PermissionPromptRequest {
        tool_id: definition.id.clone(),
        summary: permission_request_summary(definition, input),
        reason: reason.map(str::to_string),
        browser: browser_prompt_payload(definition, input, current_session_id, workspace_roots),
    }
}

fn browser_prompt_payload(
    definition: &ToolDefinition,
    input: &Value,
    current_session_id: &str,
    workspace_roots: &[PathBuf],
) -> Option<BrowserPermissionPromptPayload> {
    browser_permission_value_for_tool_call(&definition.id, input)?;
    let context = browser_permission_context_for_tool(
        &definition.id,
        input,
        current_session_id,
        workspace_roots,
    );
    let source = match canonical_tool_name(&definition.id).as_str() {
        "browser" => BrowserPermissionPromptSource::BrowserTool,
        _ => BrowserPermissionPromptSource::BrowserCliViaShell,
    };
    let action_set = match context.action {
        Some(BrowserActionCategory::Inspect) => BrowserPermissionPromptActionSet::Inspect,
        Some(BrowserActionCategory::Navigate) => BrowserPermissionPromptActionSet::Navigate,
        Some(BrowserActionCategory::Interact) => BrowserPermissionPromptActionSet::Interact,
        Some(BrowserActionCategory::Evaluate) => BrowserPermissionPromptActionSet::Evaluate,
        None => BrowserPermissionPromptActionSet::Inspect,
    };
    let (url, origin, host, target_class) = context
        .target
        .as_ref()
        .map(|target| {
            (
                Some(target.raw_url.clone()),
                target.origin.clone(),
                target.host.clone(),
                match target.target_class {
                    BrowserTargetClass::LocalDev => BrowserPermissionPromptTargetClass::LocalDev,
                    BrowserTargetClass::WorkspaceFile => {
                        BrowserPermissionPromptTargetClass::WorkspaceFile
                    }
                    BrowserTargetClass::NonWorkspaceFile => {
                        BrowserPermissionPromptTargetClass::NonWorkspaceFile
                    }
                    BrowserTargetClass::DataUrl => BrowserPermissionPromptTargetClass::DataUrl,
                    BrowserTargetClass::OpenWeb => BrowserPermissionPromptTargetClass::OpenWeb,
                },
            )
        })
        .unwrap_or((
            None,
            None,
            None,
            BrowserPermissionPromptTargetClass::Unknown,
        ));
    Some(BrowserPermissionPromptPayload {
        source,
        action_set,
        url,
        origin,
        host,
        target_class,
        tab_id: context.tab_id.clone(),
        is_cross_session: context.is_cross_session,
    })
}

fn permission_request_summary(definition: &ToolDefinition, input: &Value) -> String {
    match definition.id.as_str() {
        "Bash" | "PowerShell" => input
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| definition.id.clone()),
        "Config" => {
            let setting = input
                .get("setting")
                .and_then(Value::as_str)
                .unwrap_or("setting");
            match input.get("value") {
                Some(value) => format!("Set {setting} to {}", value),
                None => format!("Read {setting}"),
            }
        }
        "WebSearch" => input
            .get("query")
            .and_then(Value::as_str)
            .map(|query| format!("Search the web for: {query}"))
            .unwrap_or_else(|| definition.id.clone()),
        "SendMessage" => input
            .get("to")
            .and_then(Value::as_str)
            .map(|to| format!("Send a message to {to}"))
            .unwrap_or_else(|| definition.id.clone()),
        "AskUserQuestion" => "Answer questions?".to_string(),
        "ExitPlanMode" => "Exit plan mode?".to_string(),
        _ => definition.id.clone(),
    }
}
