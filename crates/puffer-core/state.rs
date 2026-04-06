use puffer_config::PufferConfig;
use puffer_session_store::SessionMetadata;
use serde::Serialize;
use std::path::PathBuf;

/// Describes the role of a rendered transcript message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Represents one rendered transcript message in the interactive UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderedMessage {
    pub role: MessageRole,
    pub text: String,
}

/// Stores the mutable session and UI state for one interactive Puffer run.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: PufferConfig,
    pub cwd: PathBuf,
    pub working_dirs: Vec<PathBuf>,
    pub session: SessionMetadata,
    pub transcript: Vec<RenderedMessage>,
    pub current_model: Option<String>,
    pub current_provider: Option<String>,
    pub prompt_color: String,
    pub effort_level: String,
    pub fast_mode: bool,
    pub sandbox_mode: String,
    pub remote_name: Option<String>,
    pub remote_environment: Option<String>,
    pub statusline_enabled: bool,
    pub vim_mode: bool,
    pub should_exit: bool,
}

impl AppState {
    /// Creates a new application state for the active session.
    pub fn new(config: PufferConfig, cwd: PathBuf, session: SessionMetadata) -> Self {
        Self {
            current_model: config.default_model.clone(),
            current_provider: config.default_provider.clone(),
            config,
            cwd,
            working_dirs: Vec::new(),
            session,
            transcript: Vec::new(),
            prompt_color: "default".to_string(),
            effort_level: "medium".to_string(),
            fast_mode: false,
            sandbox_mode: "workspace-write".to_string(),
            remote_name: None,
            remote_environment: None,
            statusline_enabled: true,
            vim_mode: false,
            should_exit: false,
        }
    }

    /// Appends a rendered message to the in-memory transcript.
    pub fn push_message(&mut self, role: MessageRole, text: impl Into<String>) {
        self.transcript.push(RenderedMessage {
            role,
            text: text.into(),
        });
    }
}
