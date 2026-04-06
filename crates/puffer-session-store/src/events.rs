use serde::{Deserialize, Serialize};

/// Stores a transcript event in append-only session history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptEvent {
    UserMessage { text: String },
    AssistantMessage { text: String },
    SystemMessage { text: String },
    CommandInvoked { name: String, args: String },
    SessionRenamed { name: String },
}
