use crate::{AppState, MessageRole};
use anyhow::Result;
use arboard::Clipboard;
use puffer_provider_registry::{AuthStore, ProviderRegistry};
use puffer_resources::{skill_by_name, LoadedResources};
use puffer_session_store::{SessionStore, TranscriptEvent};
use puffer_tools::ToolRegistry;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

/// Lists loaded skills in slash-command form.
pub(crate) fn list_skills(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    if resources.skills.is_empty() {
        return emit_system(state, session_store, "No skills are available.".to_string());
    }
    let mut text = String::from("Available skills:\n");
    for skill in &resources.skills {
        let _ = writeln!(
            &mut text,
            "/skill:{} - {}",
            skill.value.name, skill.value.description
        );
    }
    emit_system(state, session_store, text)
}

/// Summarizes workspace health, loaded resources, and auth state.
pub(crate) fn run_doctor(
    state: &mut AppState,
    resources: &LoadedResources,
    providers: &ProviderRegistry,
    auth_store: &AuthStore,
    session_store: &SessionStore,
) -> Result<()> {
    let registry = ToolRegistry::from_resources(resources);
    let mut text = String::from("Puffer doctor summary:\n");
    let _ = writeln!(
        &mut text,
        "provider={} model={}",
        state.current_provider.as_deref().unwrap_or("<unset>"),
        state.current_model.as_deref().unwrap_or("<unset>")
    );
    let _ = writeln!(&mut text, "tool_count={}", registry.tools().count());
    let _ = writeln!(
        &mut text,
        "provider_count={}",
        providers.providers().count()
    );
    let discovery_count = providers
        .provider_entries()
        .filter(|provider| provider.descriptor.discovery.is_some())
        .count();
    let _ = writeln!(&mut text, "providers_with_discovery={discovery_count}");
    let _ = writeln!(
        &mut text,
        "stored_auth_providers={}",
        auth_store.provider_ids().count()
    );
    let _ = writeln!(&mut text, "hooks={}", resources.hooks.len());
    let _ = writeln!(
        &mut text,
        "resource_diagnostics={}",
        resources.diagnostics.len()
    );
    let _ = writeln!(&mut text, "recorded_tasks={}", state.tasks().len());
    let _ = writeln!(&mut text, "working_dirs={}", state.working_dirs.len());
    let _ = writeln!(&mut text, "transcript_messages={}", state.transcript.len());
    if !resources.diagnostics.is_empty() {
        let _ = writeln!(&mut text, "Diagnostics:");
        for diagnostic in &resources.diagnostics {
            let _ = writeln!(&mut text, "- {diagnostic}");
        }
    }
    emit_system(state, session_store, text)
}

/// Copies the latest assistant message or echoes it when clipboard access fails.
pub(crate) fn copy_last_message(state: &mut AppState, session_store: &SessionStore) -> Result<()> {
    let last = state
        .transcript
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .map(|message| message.text.clone())
        .unwrap_or_default();
    if last.is_empty() {
        return emit_system(
            state,
            session_store,
            "No assistant response is available to copy.".to_string(),
        );
    }

    match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(last.clone())) {
        Ok(()) => emit_system(
            state,
            session_store,
            "Copied the latest assistant response.".to_string(),
        ),
        Err(_) => emit_system(
            state,
            session_store,
            format!("Latest assistant response:\n{last}"),
        ),
    }
}

/// Prints a compact summary of transcript and loaded-resource context.
pub(crate) fn describe_context(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
) -> Result<()> {
    emit_system(
        state,
        session_store,
        format!(
            "Context summary:\ntranscript_messages={}\nworking_dirs={}\nprompts={}\nskills={}\nplugins={}",
            state.transcript.len(),
            state.working_dirs.len(),
            resources.prompts.len(),
            resources.skills.len(),
            resources.plugins.len()
        ),
    )
}

/// Shows the current git status summary for the workspace.
pub(crate) fn describe_git_diff(state: &mut AppState, session_store: &SessionStore) -> Result<()> {
    emit_system(state, session_store, render_git_diff_summary(&state.cwd))
}

/// Expands a `/skill:<name>` command into the loaded skill contents.
pub(crate) fn execute_skill_command(
    state: &mut AppState,
    resources: &LoadedResources,
    session_store: &SessionStore,
    skill_name: &str,
) -> Result<()> {
    if let Some(skill) = skill_by_name(resources, skill_name) {
        emit_system(
            state,
            session_store,
            format!(
                "Skill {}\n{}\n\n{}",
                skill.value.name, skill.value.description, skill.value.content
            ),
        )
    } else {
        emit_system(state, session_store, format!("Unknown skill {skill_name}."))
    }
}

/// Appends a system message to the in-memory transcript and session log.
pub(crate) fn emit_system(
    state: &mut AppState,
    session_store: &SessionStore,
    text: String,
) -> Result<()> {
    state.push_message(MessageRole::System, text.clone());
    session_store.append_event(state.session.id, TranscriptEvent::SystemMessage { text })?;
    Ok(())
}

/// Removes the last rendered transcript item.
pub(crate) fn rewind_transcript(state: &mut AppState, session_store: &SessionStore) -> Result<()> {
    if state.transcript.is_empty() {
        return emit_system(
            state,
            session_store,
            "Transcript is already empty.".to_string(),
        );
    }
    state.transcript.pop();
    emit_system(
        state,
        session_store,
        "Removed the latest rendered transcript item.".to_string(),
    )
}

/// Returns terminal setup guidance for the current runtime mode.
pub(crate) fn terminal_setup_advice(state: &AppState) -> String {
    format!(
        "Terminal setup:\n- current cwd: {}\n- no_alt_screen: {}\n- tmux_golden_mode: {}",
        state.cwd.display(),
        state.config.ui.no_alt_screen,
        state.config.ui.tmux_golden_mode
    )
}

fn render_git_diff_summary(cwd: &PathBuf) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["status", "--short"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                "Working tree is clean.".to_string()
            } else {
                format!("Git status:\n{}", stdout.trim_end())
            }
        }
        Ok(output) => format!(
            "Failed to read git status: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => format!("Failed to run git status: {error}"),
    }
}
