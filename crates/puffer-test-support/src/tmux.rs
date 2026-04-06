use crate::run_command_capture;

/// Describes tmux availability and optional version metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxInfo {
    pub available: bool,
    pub version: Option<String>,
}

/// Returns true when tmux appears to be installed and runnable.
pub fn tmux_available() -> bool {
    detect_tmux().available
}

/// Probes the local system for tmux and captures its version when available.
pub fn detect_tmux() -> TmuxInfo {
    match run_command_capture("tmux", &["-V"], None) {
        Ok(output) if output.status_code == 0 => TmuxInfo {
            available: true,
            version: Some(output.stdout.trim().to_string()),
        },
        _ => TmuxInfo {
            available: false,
            version: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_tmux_never_panics() {
        let _ = detect_tmux();
    }
}
