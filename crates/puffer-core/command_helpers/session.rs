use super::emit_system;
use crate::{AppState, ToolInvocation};
use anyhow::Result;
use puffer_session_store::SessionStore;
use std::fmt::Write as _;

/// Records tool invocations into task history and the visible transcript.
pub(crate) fn append_tool_invocations(
    state: &mut AppState,
    session_store: &SessionStore,
    invocations: &[ToolInvocation],
) -> Result<()> {
    for invocation in invocations {
        state.record_task(
            invocation.tool_id.clone(),
            invocation.input.clone(),
            invocation.success,
        );
        emit_system(state, session_store, format_tool_invocation(invocation))?;
    }
    Ok(())
}

/// Handles `/memory` session note, slug, and tag operations.
pub(crate) fn handle_memory_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let trimmed = args.trim();
    if trimmed.is_empty() || trimmed == "show" {
        return emit_system(state, session_store, render_memory_summary(state));
    }

    if trimmed == "clear" {
        let tags = state.session.tags.clone();
        session_store.set_note(state.session.id, None)?;
        session_store.set_slug(state.session.id, None)?;
        for tag in &tags {
            session_store.remove_tag(state.session.id, tag)?;
        }
        state.session.note = None;
        state.session.slug = None;
        state.session.tags.clear();
        return emit_system(
            state,
            session_store,
            "Cleared session note, slug, and tags.".to_string(),
        );
    }

    if let Some(rest) = trimmed.strip_prefix("note ") {
        if matches!(rest, "clear" | "none" | "off") {
            session_store.set_note(state.session.id, None)?;
            state.session.note = None;
            return emit_system(state, session_store, "Cleared session note.".to_string());
        }
        session_store.set_note(state.session.id, Some(rest.to_string()))?;
        state.session.note = Some(rest.to_string());
        return emit_system(
            state,
            session_store,
            format!("Session note set to `{rest}`."),
        );
    }

    if let Some(rest) = trimmed.strip_prefix("slug ") {
        if matches!(rest, "clear" | "none" | "off") {
            session_store.set_slug(state.session.id, None)?;
            state.session.slug = None;
            return emit_system(state, session_store, "Cleared session slug.".to_string());
        }
        session_store.set_slug(state.session.id, Some(rest.to_string()))?;
        state.session.slug = Some(rest.to_string());
        return emit_system(
            state,
            session_store,
            format!("Session slug set to `{rest}`."),
        );
    }

    if let Some(rest) = trimmed.strip_prefix("tag add ") {
        let tag = rest.trim();
        if tag.is_empty() {
            return emit_system(
                state,
                session_store,
                "Usage: /memory tag add <tag>".to_string(),
            );
        }
        session_store.add_tag(state.session.id, tag)?;
        if !state.session.tags.iter().any(|existing| existing == tag) {
            state.session.tags.push(tag.to_string());
            state.session.tags.sort();
        }
        return emit_system(state, session_store, format!("Added session tag `{tag}`."));
    }

    if let Some(rest) = trimmed.strip_prefix("tag remove ") {
        let tag = rest.trim();
        if tag.is_empty() {
            return emit_system(
                state,
                session_store,
                "Usage: /memory tag remove <tag>".to_string(),
            );
        }
        session_store.remove_tag(state.session.id, tag)?;
        state.session.tags.retain(|existing| existing != tag);
        return emit_system(
            state,
            session_store,
            format!("Removed session tag `{tag}`."),
        );
    }

    emit_system(
        state,
        session_store,
        "Usage: /memory [show|clear|note <text>|note clear|slug <value>|slug clear|tag add <tag>|tag remove <tag>]".to_string(),
    )
}

/// Handles `/session` summary and metadata subcommands.
pub(crate) fn handle_session_command(
    state: &mut AppState,
    session_store: &SessionStore,
    args: &str,
) -> Result<()> {
    let trimmed = args.trim();
    match trimmed {
        "" | "show" => emit_system(state, session_store, render_session_summary(state)),
        "list" => {
            let sessions = session_store.list_sessions()?;
            let mut text = String::from("Sessions:\n");
            for session in sessions.iter().take(20) {
                let _ = writeln!(
                    &mut text,
                    "{} {}",
                    session.id,
                    session.display_name.as_deref().unwrap_or("<unnamed>")
                );
            }
            emit_system(state, session_store, text)
        }
        _ if trimmed.starts_with("rename ") => {
            let name = trimmed.trim_start_matches("rename ").trim();
            if name.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /session rename <name>".to_string(),
                );
            }
            session_store.rename_session(state.session.id, name.to_string())?;
            state.session.display_name = Some(name.to_string());
            emit_system(state, session_store, format!("Session renamed to `{name}`."))
        }
        _ if trimmed.starts_with("note ") => {
            let note = trimmed.trim_start_matches("note ").trim();
            if matches!(note, "clear" | "none" | "off") {
                session_store.set_note(state.session.id, None)?;
                state.session.note = None;
                return emit_system(state, session_store, "Cleared session note.".to_string());
            }
            session_store.set_note(state.session.id, Some(note.to_string()))?;
            state.session.note = Some(note.to_string());
            emit_system(state, session_store, "Updated session note.".to_string())
        }
        _ if trimmed.starts_with("slug ") => {
            let slug = trimmed.trim_start_matches("slug ").trim();
            if matches!(slug, "clear" | "none" | "off") {
                session_store.set_slug(state.session.id, None)?;
                state.session.slug = None;
                return emit_system(state, session_store, "Cleared session slug.".to_string());
            }
            session_store.set_slug(state.session.id, Some(slug.to_string()))?;
            state.session.slug = Some(slug.to_string());
            emit_system(state, session_store, format!("Session slug set to `{slug}`."))
        }
        _ if trimmed.starts_with("tag add ") => {
            let tag = trimmed.trim_start_matches("tag add ").trim();
            if tag.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /session tag add <tag>".to_string(),
                );
            }
            session_store.add_tag(state.session.id, tag)?;
            if !state.session.tags.iter().any(|existing| existing == tag) {
                state.session.tags.push(tag.to_string());
                state.session.tags.sort();
            }
            emit_system(state, session_store, format!("Added session tag `{tag}`."))
        }
        _ if trimmed.starts_with("tag remove ") => {
            let tag = trimmed.trim_start_matches("tag remove ").trim();
            if tag.is_empty() {
                return emit_system(
                    state,
                    session_store,
                    "Usage: /session tag remove <tag>".to_string(),
                );
            }
            session_store.remove_tag(state.session.id, tag)?;
            state.session.tags.retain(|existing| existing != tag);
            emit_system(state, session_store, format!("Removed session tag `{tag}`."))
        }
        _ => emit_system(
            state,
            session_store,
            "Usage: /session [show|list|rename <name>|note <text|clear>|slug <value|clear>|tag add <tag>|tag remove <tag>]".to_string(),
        ),
    }
}

fn format_tool_invocation(invocation: &ToolInvocation) -> String {
    let status = if invocation.success { "ok" } else { "error" };
    let output = invocation.output.trim();
    if output.is_empty() {
        format!(
            "Tool {} [{}]\ninput: {}",
            invocation.tool_id, status, invocation.input
        )
    } else {
        format!(
            "Tool {} [{}]\ninput: {}\n{}",
            invocation.tool_id, status, invocation.input, output
        )
    }
}

fn render_memory_summary(state: &AppState) -> String {
    format!(
        "Session memory summary:\nslug={}\nnote={}\ntags={}",
        state.session.slug.as_deref().unwrap_or("<none>"),
        state.session.note.as_deref().unwrap_or("<none>"),
        if state.session.tags.is_empty() {
            "<none>".to_string()
        } else {
            state.session.tags.join(", ")
        },
    )
}

fn render_session_summary(state: &AppState) -> String {
    format!(
        "session_id={}\ncwd={}\ndisplay_name={}\nslug={}\nparent={:?}\ntags={}\nnote={}",
        state.session.id,
        state.session.cwd.display(),
        state.session.display_name.as_deref().unwrap_or("<unnamed>"),
        state.session.slug.as_deref().unwrap_or("<none>"),
        state.session.parent_session_id,
        if state.session.tags.is_empty() {
            "<none>".to_string()
        } else {
            state.session.tags.join(", ")
        },
        state.session.note.as_deref().unwrap_or("<none>")
    )
}
