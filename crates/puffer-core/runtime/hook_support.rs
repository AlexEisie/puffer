use crate::hooks::run_resource_hooks;
use puffer_resources::LoadedResources;
use serde_json::Value;
use std::path::Path;

/// Runs `tool_start` hooks for one tool invocation.
pub(super) fn run_tool_start_hooks(
    resources: &LoadedResources,
    cwd: &Path,
    tool_id: &str,
    input: &Value,
) {
    run_resource_hooks(
        resources,
        cwd,
        "tool_start",
        &[
            ("PUFFER_TOOL_ID", tool_id.to_string()),
            ("PUFFER_TOOL_INPUT", input.to_string()),
            ("PUFFER_TOOL_SUCCESS", String::new()),
            ("PUFFER_TOOL_STDOUT", String::new()),
            ("PUFFER_TOOL_STDERR", String::new()),
        ],
    );
}

/// Runs `tool_end` hooks for one completed tool invocation.
pub(super) fn run_tool_end_hooks(
    resources: &LoadedResources,
    cwd: &Path,
    tool_id: &str,
    input: &Value,
    success: bool,
    stdout: &str,
    stderr: &str,
) {
    run_resource_hooks(
        resources,
        cwd,
        "tool_end",
        &[
            ("PUFFER_TOOL_ID", tool_id.to_string()),
            ("PUFFER_TOOL_INPUT", input.to_string()),
            (
                "PUFFER_TOOL_SUCCESS",
                if success { "true" } else { "false" }.to_string(),
            ),
            ("PUFFER_TOOL_STDOUT", stdout.to_string()),
            ("PUFFER_TOOL_STDERR", stderr.to_string()),
        ],
    );
}

/// Runs `turn_end` hooks after a provider response completes.
pub(crate) fn run_turn_hooks(
    resources: &LoadedResources,
    cwd: &Path,
    text: &str,
    tool_count: usize,
) {
    run_resource_hooks(
        resources,
        cwd,
        "turn_end",
        &[
            ("PUFFER_TURN_TEXT", text.to_string()),
            ("PUFFER_TURN_TOOL_COUNT", tool_count.to_string()),
        ],
    );
}
