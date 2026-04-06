use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Captures the result of a child process invocation for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs a command and captures stdout, stderr, and exit status.
pub fn run_command_capture(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
) -> Result<CommandOutput> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run command `{program}`"))?;
    Ok(CommandOutput {
        status_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_command_output() {
        let output = run_command_capture("sh", &["-lc", "printf 'hello'"], None).unwrap();
        assert_eq!(output.status_code, 0);
        assert_eq!(output.stdout, "hello");
    }
}
