use super::*;
use crate::approval_overlay::ApprovalOverlay;
use crate::permission_prompt_flow::handle_permission_prompt_key;
use crate::state::{PendingPermissionRequest, PendingSubmit, PendingSubmitEvent};
use crossterm::event::{KeyCode, KeyEvent};
use puffer_core::{PermissionPromptAction, PermissionPromptRequest};
use ratatui::backend::TestBackend;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn poll_pending_submit_opens_permission_prompt_overlay() {
    let tempdir = tempdir().unwrap();
    let paths = ConfigPaths::discover(tempdir.path());
    ensure_workspace_dirs(&paths).unwrap();
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let auth_path = paths.user_config_dir.join("auth.json");

    let request = PermissionPromptRequest {
        tool_id: "Bash".to_string(),
        summary: "git push origin master".to_string(),
        reason: Some("shell command matches sandbox exclusion `git push`".to_string()),
        browser: None,
        review: None,
    };
    let (event_tx, event_rx) = mpsc::channel();
    let (response_tx, _response_rx) = mpsc::channel();
    event_tx
        .send(PendingSubmitEvent::PermissionRequest(
            request.clone(),
            response_tx,
        ))
        .unwrap();

    let mut tui = TuiState {
        pending_submit: Some(PendingSubmit {
            prompt: "hi".to_string(),
            receiver: event_rx,
            rendered_tool_invocations: 0,
            pending_tool_calls: Vec::new(),
            started_at: std::time::Instant::now(),
            thinking_active: false,
            status_hint: None,
            cancel: puffer_core::CancelToken::new(),
        }),
        ..TuiState::default()
    };
    let mut state = sample_state();
    let mut auth_store = sample_auth_store();

    let completed = poll_pending_submit(
        &mut state,
        &mut auth_store,
        &auth_path,
        &session_store,
        &mut tui,
    )
    .unwrap();

    assert!(!completed);
    assert!(tui.pending_permission_request.is_some());
    assert_eq!(
        tui.overlay,
        Some(OverlayState::PermissionPrompt {
            overlay: ApprovalOverlay::new(request),
        })
    );
}

#[test]
fn permission_prompt_shortcuts_send_response() {
    let request = PermissionPromptRequest {
        tool_id: "Config".to_string(),
        summary: "Set theme to \"dark\"".to_string(),
        reason: Some("config writes require approval".to_string()),
        browser: None,
        review: None,
    };
    let (response_tx, response_rx) = mpsc::channel();
    let mut tui = TuiState {
        overlay: Some(OverlayState::PermissionPrompt {
            overlay: ApprovalOverlay::new(request),
        }),
        pending_permission_request: Some(PendingPermissionRequest { response_tx }),
        ..TuiState::default()
    };

    assert!(handle_permission_prompt_key(
        KeyEvent::from(KeyCode::Char('a')),
        &mut tui
    ));
    assert_eq!(
        response_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        PermissionPromptAction::AllowSession
    );
    assert!(tui.overlay.is_none());
    assert!(tui.pending_permission_request.is_none());
}

#[test]
fn render_permission_prompt_shows_codex_style_options() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = sample_state();
    let resources = sample_resources();
    let providers = sample_providers();
    let auth_store = sample_auth_store();
    let overlay = OverlayState::PermissionPrompt {
        overlay: ApprovalOverlay::new(PermissionPromptRequest {
            tool_id: "Bash".to_string(),
            summary: "git push origin master".to_string(),
            reason: Some("shell command matches sandbox exclusion `git push`".to_string()),
            browser: None,
            review: None,
        }),
    };

    terminal
        .draw(|frame| {
            render::set_active_overlay(Some(overlay.clone()));
            render::render(
                frame,
                &state,
                &resources,
                &providers,
                &auth_store,
                "",
                0,
                0,
                0,
                &supported_commands(),
            );
            render::set_active_overlay(None);
        })
        .unwrap();
    let rendered = buffer_to_string(terminal.backend().buffer());
    assert!(rendered.contains("Would you like to grant these permissions?"));
    assert!(rendered.contains("Yes, grant these permissions"));
    assert!(rendered.contains("Yes, grant these permissions for this session"));
    assert!(rendered.contains("Yes, allow ALL tools for this session"));
    assert!(rendered.contains("No, continue without permissions"));
}

#[test]
fn render_browser_permission_prompt_shows_context_with_generic_options() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = sample_state();
    let resources = sample_resources();
    let providers = sample_providers();
    let auth_store = sample_auth_store();
    let overlay = OverlayState::PermissionPrompt {
        overlay: ApprovalOverlay::new(PermissionPromptRequest {
            tool_id: "Browser".to_string(),
            summary: "Open https://docs.example.com/a".to_string(),
            reason: Some("browser navigation and interaction require approval".to_string()),
            browser: Some(puffer_core::BrowserPermissionPromptPayload {
                source: puffer_core::BrowserPermissionPromptSource::BrowserTool,
                action_set: puffer_core::BrowserPermissionPromptActionSet::Navigate,
                url: Some("https://docs.example.com/a".to_string()),
                origin: Some("https://docs.example.com".to_string()),
                host: Some("docs.example.com".to_string()),
                target_class: puffer_core::BrowserPermissionPromptTargetClass::OpenWeb,
                tab_id: Some("tab-1".to_string()),
            }),
            review: None,
        }),
    };

    terminal
        .draw(|frame| {
            render::set_active_overlay(Some(overlay.clone()));
            render::render(
                frame,
                &state,
                &resources,
                &providers,
                &auth_store,
                "",
                0,
                0,
                0,
                &supported_commands(),
            );
            render::set_active_overlay(None);
        })
        .unwrap();
    let rendered = buffer_to_string(terminal.backend().buffer());
    assert!(rendered.contains("Action: "));
    assert!(rendered.contains("Open https://docs.example.com/a"));
    assert!(rendered.contains("Yes, grant these permissions"));
    assert!(rendered.contains("Yes, allow this browser context for this session"));
    assert!(!rendered.contains("Yes, allow ALL tools for this session"));
    assert!(!rendered.contains("all for session"));
    assert!(rendered.contains("context for session"));
    assert!(!rendered.contains("Source: "));
    assert!(!rendered.contains("Action Set: "));
    assert!(!rendered.contains("Reason: "));
}

#[test]
fn browser_permission_prompt_shortcuts_skip_allow_all_session() {
    let request = PermissionPromptRequest {
        tool_id: "Browser".to_string(),
        summary: "Open https://docs.example.com/a".to_string(),
        reason: Some("browser navigation and interaction require approval".to_string()),
        browser: Some(puffer_core::BrowserPermissionPromptPayload {
            source: puffer_core::BrowserPermissionPromptSource::BrowserTool,
            action_set: puffer_core::BrowserPermissionPromptActionSet::Navigate,
            url: Some("https://docs.example.com/a".to_string()),
            origin: Some("https://docs.example.com".to_string()),
            host: Some("docs.example.com".to_string()),
            target_class: puffer_core::BrowserPermissionPromptTargetClass::OpenWeb,
            tab_id: Some("tab-1".to_string()),
        }),
        review: None,
    };
    let mut overlay = ApprovalOverlay::new(request);

    assert_eq!(
        overlay.activate_shortcut('A'),
        Some(PermissionPromptAction::AllowSession)
    );
}

#[test]
fn render_browser_evaluate_prompt_shows_reason_only_for_evaluate() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = sample_state();
    let resources = sample_resources();
    let providers = sample_providers();
    let auth_store = sample_auth_store();
    let overlay = OverlayState::PermissionPrompt {
        overlay: ApprovalOverlay::new(PermissionPromptRequest {
            tool_id: "Browser".to_string(),
            summary: "Run JavaScript on https://docs.example.com".to_string(),
            reason: Some(
                "browser evaluation requires explicit approval because it executes page JavaScript"
                    .to_string(),
            ),
            browser: Some(puffer_core::BrowserPermissionPromptPayload {
                source: puffer_core::BrowserPermissionPromptSource::BrowserTool,
                action_set: puffer_core::BrowserPermissionPromptActionSet::Evaluate,
                url: Some("https://docs.example.com".to_string()),
                origin: Some("https://docs.example.com".to_string()),
                host: Some("docs.example.com".to_string()),
                target_class: puffer_core::BrowserPermissionPromptTargetClass::OpenWeb,
                tab_id: Some("tab-1".to_string()),
            }),
            review: None,
        }),
    };

    terminal
        .draw(|frame| {
            render::set_active_overlay(Some(overlay.clone()));
            render::render(
                frame,
                &state,
                &resources,
                &providers,
                &auth_store,
                "",
                0,
                0,
                0,
                &supported_commands(),
            );
            render::set_active_overlay(None);
        })
        .unwrap();
    let rendered = buffer_to_string(terminal.backend().buffer());
    assert!(rendered.contains("Run JavaScript on https://docs.example.com"));
    assert!(rendered.contains("Reason: "));
    assert!(rendered.contains("executes page JavaScript"));
}

#[test]
fn render_browser_fallback_prompt_does_not_show_reviewer_payload() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = sample_state();
    let resources = sample_resources();
    let providers = sample_providers();
    let auth_store = sample_auth_store();
    let overlay = OverlayState::PermissionPrompt {
        overlay: ApprovalOverlay::new(PermissionPromptRequest {
            tool_id: "Browser".to_string(),
            summary: "Open https://docs.example.com/a".to_string(),
            reason: Some("browser navigation and interaction require approval".to_string()),
            browser: Some(puffer_core::BrowserPermissionPromptPayload {
                source: puffer_core::BrowserPermissionPromptSource::BrowserTool,
                action_set: puffer_core::BrowserPermissionPromptActionSet::Navigate,
                url: Some("https://docs.example.com/a".to_string()),
                origin: Some("https://docs.example.com".to_string()),
                host: Some("docs.example.com".to_string()),
                target_class: puffer_core::BrowserPermissionPromptTargetClass::OpenWeb,
                tab_id: Some("tab-1".to_string()),
            }),
            review: Some(puffer_core::PermissionPromptReviewPayload {
                decision: puffer_core::BrowserAutoReviewRuntimeResult::NeedsUser,
                risk: "medium".to_string(),
                rationale: "Needs user confirmation.".to_string(),
                resolved_root_session_id: "root-1".to_string(),
                session_targeting: puffer_core::BrowserAutoReviewSessionTargeting::ExplicitSession,
            }),
        }),
    };

    terminal
        .draw(|frame| {
            render::set_active_overlay(Some(overlay.clone()));
            render::render(
                frame,
                &state,
                &resources,
                &providers,
                &auth_store,
                "",
                0,
                0,
                0,
                &supported_commands(),
            );
            render::set_active_overlay(None);
        })
        .unwrap();
    let rendered = buffer_to_string(terminal.backend().buffer());
    assert!(rendered.contains("Action: "));
    assert!(rendered.contains("Open https://docs.example.com/a"));
    assert!(rendered.contains("Yes, grant these permissions"));
    assert!(rendered.contains("Yes, allow this browser context for this session"));
    assert!(!rendered.contains("Reviewer Decision: "));
    assert!(!rendered.contains("Reviewer Risk: "));
    assert!(!rendered.contains("Reviewer Rationale: "));
    assert!(!rendered.contains("Reviewer Context: "));
    assert!(!rendered.contains("Needs user confirmation."));
}
