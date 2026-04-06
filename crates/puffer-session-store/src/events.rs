use serde::{Deserialize, Serialize};

/// Stores a transcript event in append-only session history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptEvent {
    UserMessage {
        text: String,
    },
    AssistantMessage {
        text: String,
    },
    SystemMessage {
        text: String,
    },
    CommandInvoked {
        name: String,
        args: String,
    },
    SessionRenamed {
        name: String,
    },
    StateSnapshot {
        current_model: Option<String>,
        current_provider: Option<String>,
        theme: String,
        prompt_color: String,
        effort_level: String,
        fast_mode: bool,
        sandbox_mode: String,
        remote_name: Option<String>,
        remote_environment: Option<String>,
        statusline_enabled: bool,
        working_dirs: Vec<String>,
    },
}
