use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionListItemDto {
    pub(crate) session_id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) title: String,
    pub(crate) cwd: String,
    pub(crate) folder_path: String,
    pub(crate) updated_at_ms: u64,
    pub(crate) created_at_ms: u64,
    pub(crate) event_count: usize,
    pub(crate) slug: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) note: Option<String>,
    pub(crate) parent_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FolderGroupDto {
    pub(crate) folder_id: String,
    pub(crate) folder_label: String,
    pub(crate) folder_path: String,
    pub(crate) session_count: usize,
    pub(crate) sessions: Vec<SessionListItemDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffSummaryDto {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) command_label: String,
    pub(crate) status_text: String,
    pub(crate) unstaged_diffstat: String,
    pub(crate) staged_diffstat: String,
    pub(crate) patch_excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepoPullRequestDto {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) is_draft: bool,
    pub(crate) merge_state_status: Option<String>,
    pub(crate) head_ref_name: Option<String>,
    pub(crate) base_ref_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepoStatusDto {
    pub(crate) session_id: String,
    pub(crate) cwd: String,
    pub(crate) repo_root: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) head_sha: Option<String>,
    pub(crate) is_clean: bool,
    pub(crate) status_lines: Vec<String>,
    pub(crate) has_gh: bool,
    pub(crate) gh_authenticated: bool,
    pub(crate) can_create_pull_request: bool,
    pub(crate) can_merge_pull_request: bool,
    pub(crate) create_pull_request_reason: Option<String>,
    pub(crate) merge_pull_request_reason: Option<String>,
    pub(crate) open_pull_request: Option<RepoPullRequestDto>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepoActionResultDto {
    pub(crate) ok: bool,
    pub(crate) action: String,
    pub(crate) message: String,
    pub(crate) repo_status: RepoStatusDto,
    pub(crate) pull_request: Option<RepoPullRequestDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum TimelineItemDto {
    UserMessage {
        id: String,
        text: String,
    },
    AssistantMessage {
        id: String,
        text: String,
    },
    SystemMessage {
        id: String,
        text: String,
    },
    Command {
        id: String,
        command_name: String,
        command_args: String,
    },
    ToolCall {
        id: String,
        tool_id: String,
        status: String,
        summary: Option<String>,
        input_text: String,
        input_json: Option<Value>,
        output_text: String,
    },
    PermissionDialog {
        id: String,
        tool_id: String,
        state: String,
        summary: Option<String>,
        reason: String,
        input_text: Option<String>,
    },
    DiffSnapshot {
        id: String,
        snapshot: DiffSummaryDto,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionDetailDto {
    pub(crate) session_id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) title: String,
    pub(crate) cwd: String,
    pub(crate) folder_path: String,
    pub(crate) updated_at_ms: u64,
    pub(crate) created_at_ms: u64,
    pub(crate) slug: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) note: Option<String>,
    pub(crate) parent_session_id: Option<String>,
    pub(crate) timeline: Vec<TimelineItemDto>,
    pub(crate) latest_diff: Option<DiffSummaryDto>,
    pub(crate) diff_history: Vec<DiffSummaryDto>,
    pub(crate) repo_status: RepoStatusDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsSnapshotDto {
    pub(crate) workspace_root: String,
    pub(crate) workspace_config_file: String,
    pub(crate) user_config_file: String,
    pub(crate) auth_store_file: String,
    pub(crate) builtin_resources_dir: String,
    pub(crate) config: SettingsConfigDto,
    pub(crate) resources: ResourceCountsDto,
    pub(crate) sessions: SettingsSessionSummaryDto,
    pub(crate) auth: Vec<AuthProviderStatusDto>,
    pub(crate) providers: Vec<ProviderSummaryDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsConfigDto {
    pub(crate) app_name: String,
    pub(crate) default_provider: Option<String>,
    pub(crate) default_model: Option<String>,
    pub(crate) openai_base_url: Option<String>,
    pub(crate) theme: String,
    pub(crate) mascot_id: String,
    pub(crate) mascot_display_name: String,
    pub(crate) mascot_enabled: bool,
    pub(crate) ui_no_alt_screen: bool,
    pub(crate) ui_tmux_golden_mode: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResourceCountsDto {
    pub(crate) providers: usize,
    pub(crate) tools: usize,
    pub(crate) agents: usize,
    pub(crate) prompts: usize,
    pub(crate) hooks: usize,
    pub(crate) skills: usize,
    pub(crate) mascots: usize,
    pub(crate) plugins: usize,
    pub(crate) mcp_servers: usize,
    pub(crate) ides: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsSessionSummaryDto {
    pub(crate) total_sessions: usize,
    pub(crate) folder_groups: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthProviderStatusDto {
    pub(crate) provider_id: String,
    pub(crate) kind: String,
    pub(crate) email: Option<String>,
    pub(crate) expires_at_ms: Option<u64>,
    pub(crate) scopes: Vec<String>,
    pub(crate) plan_type: Option<String>,
    pub(crate) organization_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderSummaryDto {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) base_url: String,
    pub(crate) default_api: String,
    pub(crate) model_count: usize,
    pub(crate) auth_modes: Vec<String>,
    pub(crate) source_kind: String,
    pub(crate) source_path: Option<String>,
}
