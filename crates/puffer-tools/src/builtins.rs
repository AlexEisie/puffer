use crate::model::TypedToolInput;
use crate::{
    BashToolInput, ReadFileToolInput, ToolExecutionResult, ToolInput, ToolKind, ToolOutput,
    WriteFileToolInput,
};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Returns the built-in tool kind for a declarative handler id.
pub fn builtin_tool_kind(handler: &str) -> Option<ToolKind> {
    match handler {
        "bash" => Some(ToolKind::Bash),
        "read_file" => Some(ToolKind::ReadFile),
        "write_file" => Some(ToolKind::WriteFile),
        _ => None,
    }
}

/// Parses raw JSON input into the typed payload expected by one built-in tool.
pub fn parse_builtin_input(kind: ToolKind, input: Value) -> Result<ToolInput> {
    match kind {
        ToolKind::Bash => {
            let input = serde_json::from_value::<BashToolInput>(input)?;
            Ok(ToolInput::Bash {
                command: input.command,
            })
        }
        ToolKind::ReadFile => {
            let input = serde_json::from_value::<ReadFileToolInput>(input)?;
            Ok(ToolInput::ReadFile { path: input.path })
        }
        ToolKind::WriteFile => {
            let input = serde_json::from_value::<WriteFileToolInput>(input)?;
            Ok(ToolInput::WriteFile {
                path: input.path,
                contents: input.contents,
            })
        }
    }
}

/// Executes one built-in tool with typed input under the given working directory.
pub fn execute_builtin_tool(
    tool_id: &str,
    kind: ToolKind,
    cwd: &Path,
    input: ToolInput,
) -> Result<ToolExecutionResult> {
    let (actual_kind, payload) = input.into_kind_payload();
    if actual_kind != kind {
        return Err(anyhow!(
            "tool input mismatch for {tool_id}: expected {:?}, got {:?}",
            kind,
            actual_kind
        ));
    }

    match payload {
        TypedToolInput::Bash(input) => execute_bash_tool(tool_id, cwd, input),
        TypedToolInput::ReadFile(input) => execute_read_file_tool(tool_id, cwd, input),
        TypedToolInput::WriteFile(input) => execute_write_file_tool(tool_id, cwd, input),
    }
}

/// Executes the built-in `bash` tool.
pub fn execute_bash_tool(
    tool_id: &str,
    cwd: &Path,
    input: BashToolInput,
) -> Result<ToolExecutionResult> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(&input.command)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to execute bash tool in {}", cwd.display()))?;
    Ok(ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: output.status.success(),
        output: ToolOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            metadata: serde_json::json!({
                "status_code": output.status.code(),
                "command": input.command,
            }),
        },
    })
}

/// Executes the built-in `read_file` tool.
pub fn execute_read_file_tool(
    tool_id: &str,
    cwd: &Path,
    input: ReadFileToolInput,
) -> Result<ToolExecutionResult> {
    let path = absolutize(cwd, &input.path);
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read file {}", path.display()))?;
    Ok(ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: true,
        output: ToolOutput {
            stdout: contents,
            stderr: String::new(),
            metadata: serde_json::json!({
                "path": path,
            }),
        },
    })
}

/// Executes the built-in `write_file` tool.
pub fn execute_write_file_tool(
    tool_id: &str,
    cwd: &Path,
    input: WriteFileToolInput,
) -> Result<ToolExecutionResult> {
    let path = absolutize(cwd, &input.path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir {}", parent.display()))?;
    }
    fs::write(&path, &input.contents)
        .with_context(|| format!("failed to write file {}", path.display()))?;
    Ok(ToolExecutionResult {
        tool_id: tool_id.to_string(),
        success: true,
        output: ToolOutput {
            stdout: format!("wrote {}", path.display()),
            stderr: String::new(),
            metadata: serde_json::json!({
                "path": path,
                "bytes_written": input.contents.len(),
            }),
        },
    })
}

fn absolutize(cwd: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_and_write_tools_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let path = std::path::PathBuf::from("note.txt");
        let write = execute_builtin_tool(
            "write_file",
            ToolKind::WriteFile,
            temp.path(),
            ToolInput::WriteFile {
                path: path.clone(),
                contents: "hello".to_string(),
            },
        )
        .unwrap();
        assert!(write.success);

        let read = execute_builtin_tool(
            "read_file",
            ToolKind::ReadFile,
            temp.path(),
            ToolInput::ReadFile { path },
        )
        .unwrap();
        assert_eq!(read.output.stdout, "hello");
    }

    #[test]
    fn builtin_helpers_cover_registered_handlers() {
        assert_eq!(builtin_tool_kind("bash"), Some(ToolKind::Bash));
        assert_eq!(builtin_tool_kind("read_file"), Some(ToolKind::ReadFile));
        assert_eq!(builtin_tool_kind("write_file"), Some(ToolKind::WriteFile));
        assert_eq!(builtin_tool_kind("unknown"), None);
    }

    #[test]
    fn parse_builtin_input_uses_the_same_tool_shapes_as_runtime_execution() {
        let parsed = parse_builtin_input(
            ToolKind::WriteFile,
            serde_json::json!({
                "path": "note.txt",
                "contents": "hello",
            }),
        )
        .unwrap();
        assert_eq!(
            parsed,
            ToolInput::WriteFile {
                path: "note.txt".into(),
                contents: "hello".to_string(),
            }
        );
    }

    #[test]
    fn execute_builtin_tool_rejects_mismatched_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let error = execute_builtin_tool(
            "read_file",
            ToolKind::ReadFile,
            temp.path(),
            ToolInput::Bash {
                command: "printf hi".to_string(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("tool input mismatch"));
    }
}
