# Generated Video Preview Playback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render generated video attachments as first-frame video cards and play them in the attachment overlay without sending video bytes through daemon RPC JSON.

**Architecture:** Keep generated videos as metadata-first timeline attachments. Add a daemon-only generated video ticket path with HTTP range serving, then have the frontend build the final media URL from the active daemon handshake and render native `<video>` elements. Images stay on the existing preview-byte path.

**Tech Stack:** Rust (`puffer-core`, `puffer-cli`, Axum), TypeScript/Svelte (`apps/puffer-desktop`), Playwright, Vitest, Cargo tests.

**Spec:** `docs/superpowers/specs/2026-06-09-generated-video-preview-playback-design.md`

---

## Recheck Outcome

The spec was tightened before planning to avoid over-design:

- Use `create_generated_video_access`, not a generic generated media access API.
- Use `/media/generated-video/<ticket>`, not a generic media route.
- Return a daemon media path from Rust and let `DaemonClient` build the final URL from the active handshake. This keeps SSH-forwarded daemon connections working.
- Do not add ffmpeg, poster sidecars, CORS changes, `HEAD`, revocation APIs, background cleanup, arbitrary file serving, or visibility scheduling.
- Do add precise range parsing, opportunistic ticket pruning, and blob-only URL revocation.

Existing dirty files outside this plan must not be reverted or mixed into plan commits:

- `apps/puffer-desktop/src/App.svelte`
- `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
- `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
- `specs/puffer-desktop/680.md`
- `specs/puffer-desktop/681.md`

If those files are still dirty when executing this plan, read them before editing and preserve user changes.

---

## File Structure

- Modify: `crates/puffer-core/media_runtime.rs`
  - Adds generated video artifact validation and canonical path resolution.
- Modify: `crates/puffer-core/media_runtime_generated_preview_tests.rs`
  - Adds tests for generated video artifact metadata validation.
- Modify: `crates/puffer-core/lib.rs`
  - Exports the generated video access metadata helper and result types.
- Modify: `crates/puffer-cli/src/daemon.rs`
  - Adds RPC params/result types, ticket registry, generated video access handler, HTTP route, and range serving.
- Modify: `crates/puffer-cli/Cargo.toml`
  - Adds `tokio-util` for streaming file bodies without loading whole videos.
- Modify: `Cargo.lock`
  - Records the new `tokio-util` daemon dependency.
- Modify: `apps/puffer-desktop/src/lib/types.ts`
  - Adds `video` attachment kind and generated video access result type.
- Modify: `apps/puffer-desktop/src/lib/api/daemonClient.ts`
  - Adds a helper for converting a daemon media path into an HTTP URL from the current handshake.
- Modify: `apps/puffer-desktop/src/lib/api/desktop.ts`
  - Adds `createGeneratedVideoAccess` and accepts backend video attachments.
- Modify: `apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts`
  - Tests video typing, daemon path to URL conversion, and blob-only revocation behavior through exported helpers.
- Modify: `apps/puffer-desktop/src/lib/agentTurnAttachments.ts`
  - Treats video attachments as `[Video: name]` and revokes only `blob:` URLs.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/MessageAttachmentPreviewStrip.svelte`
  - Requests generated video access and attaches transient video display URLs.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte`
  - Renders video thumbnail cards with a play affordance.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte`
  - Renders video playback in the overlay.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.ts`
  - Rename or generalize the helper to generated media folder actions without changing unrelated download behavior.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte`
  - Opens video attachments by requesting generated video access when needed.
- Modify: `apps/puffer-desktop/src/App.svelte`
  - Appends live `/video` results as generated video attachments.
- Modify: `apps/puffer-desktop/tests/support/fakeDaemon.ts`
  - Adds fake generated video access RPC and HTTP media route interception.
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Adds generated video card, overlay, missing access, and live `/video` coverage.
- Create: `specs/puffer-cli/166.md`
  - Documents daemon generated video access and range serving.
- Create: `specs/puffer-desktop/682.md`
  - Documents generated video card and overlay playback.

---

## Task 1: Core Generated Video Artifact Validation

**Files:**
- Modify: `crates/puffer-core/media_runtime.rs`
- Modify: `crates/puffer-core/media_runtime_generated_preview_tests.rs`
- Modify: `crates/puffer-core/lib.rs`

- [ ] **Step 1: Write failing validation tests**

Add these tests to `crates/puffer-core/media_runtime_generated_preview_tests.rs` after the existing generated media preview tests:

```rust
#[test]
fn generated_video_access_metadata_accepts_video_under_artifact_root() {
    let workspace = tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let video_path = service
        .write_artifact_bytes("artifact-video-1", "generated.mp4", b"mp4-bytes")
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-video-1".to_string(),
            job_id: "job-video-1".to_string(),
            kind: crate::runtime::media::MediaKind::Video,
            path: video_path.clone(),
            mime_type: "video/mp4".to_string(),
            byte_count: 9,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    let result = generated_video_access_metadata_by_artifact(
        workspace.path(),
        "artifact-video-1",
    );

    let GeneratedVideoAccessMetadataResult::Available(metadata) = result else {
        panic!("expected available video metadata");
    };
    assert_eq!(metadata.mime_type, "video/mp4");
    assert_eq!(metadata.byte_count, 9);
    assert_eq!(metadata.path, video_path.canonicalize().unwrap());
}

#[test]
fn generated_video_access_metadata_rejects_non_video_artifact() {
    let workspace = tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let image_path = service
        .write_image_artifact_bytes("artifact-image-1", "image.jpeg", &[0xff, 0xd8, 0xff, 0xd9])
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-image-1".to_string(),
            job_id: "job-image-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: image_path,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        generated_video_access_metadata_by_artifact(workspace.path(), "artifact-image-1"),
        GeneratedVideoAccessMetadataResult::Unsupported
    );
}

#[test]
fn generated_video_access_metadata_rejects_symlink_escape() {
    let workspace = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_video = outside.path().join("generated.mp4");
    std::fs::write(&outside_video, b"mp4-bytes").unwrap();
    let link_dir = workspace
        .path()
        .join(".puffer/media/artifacts/artifact-video-1");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link = link_dir.join("generated.mp4");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_video, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside_video, &link).unwrap();

    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-video-1".to_string(),
            job_id: "job-video-1".to_string(),
            kind: crate::runtime::media::MediaKind::Video,
            path: link,
            mime_type: "video/mp4".to_string(),
            byte_count: 9,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        generated_video_access_metadata_by_artifact(workspace.path(), "artifact-video-1"),
        GeneratedVideoAccessMetadataResult::Unsupported
    );
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
cargo test -p puffer-core generated_video_access_metadata -- --nocapture
```

Expected: FAIL because `GeneratedVideoAccessMetadataResult` and `generated_video_access_metadata_by_artifact` do not exist.

- [ ] **Step 3: Add generated video validation helper**

Add this public API near the existing `GeneratedMediaPreviewResult` in `crates/puffer-core/media_runtime.rs`:

```rust
/// Describes a trusted generated video file that can be served through a ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedVideoAccessMetadata {
    pub path: PathBuf,
    pub mime_type: String,
    pub byte_count: u64,
}

/// Describes generated video access metadata lookup state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedVideoAccessMetadataResult {
    Available(GeneratedVideoAccessMetadata),
    Missing,
    Unsupported,
}
```

Add this function near `read_generated_media_preview_by_artifact`:

```rust
/// Resolves trusted generated video metadata by artifact id.
pub fn generated_video_access_metadata_by_artifact(
    workspace_root: impl AsRef<Path>,
    artifact_id: &str,
) -> GeneratedVideoAccessMetadataResult {
    let workspace_root = workspace_root.as_ref();
    let service = MediaGenerationService::new(workspace_root);
    let artifact = match service.load_artifact(artifact_id) {
        Ok(artifact) => artifact,
        Err(_) => return GeneratedVideoAccessMetadataResult::Missing,
    };
    if artifact.kind != MediaKind::Video {
        return GeneratedVideoAccessMetadataResult::Unsupported;
    }
    let Some(mime_type) = canonical_generated_video_mime_type(&artifact.mime_type) else {
        return GeneratedVideoAccessMetadataResult::Unsupported;
    };
    let artifact_root = generated_media_artifact_root(workspace_root, &artifact.id);
    let canonical_path = match canonical_generated_media_artifact_path(&artifact_root, &artifact.path) {
        Ok(path) => path,
        Err(GeneratedMediaPathError::Missing) => {
            return GeneratedVideoAccessMetadataResult::Missing
        }
        Err(GeneratedMediaPathError::Unsupported) => {
            return GeneratedVideoAccessMetadataResult::Unsupported
        }
    };
    let byte_count = std::fs::metadata(&canonical_path)
        .map(|metadata| metadata.len())
        .unwrap_or(artifact.byte_count);
    GeneratedVideoAccessMetadataResult::Available(GeneratedVideoAccessMetadata {
        path: canonical_path,
        mime_type: mime_type.to_string(),
        byte_count,
    })
}
```

Add these private helpers below `generated_media_image_root`:

```rust
fn generated_media_artifact_root(workspace_root: &Path, artifact_id: &str) -> PathBuf {
    workspace_root
        .join(".puffer")
        .join("media")
        .join("artifacts")
        .join(artifact_id)
}

fn canonical_generated_media_artifact_path(
    artifact_root: &Path,
    path: &Path,
) -> std::result::Result<PathBuf, GeneratedMediaPathError> {
    canonical_generated_media_image_path(artifact_root, path)
}

fn canonical_generated_video_mime_type(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "video/mp4" => Some("video/mp4"),
        "video/webm" => Some("video/webm"),
        _ => None,
    }
}
```

- [ ] **Step 4: Export the new helper and types**

In `crates/puffer-core/lib.rs`, extend the `pub use media_runtime::{ ... }`
list with:

```rust
generated_video_access_metadata_by_artifact, GeneratedVideoAccessMetadata,
GeneratedVideoAccessMetadataResult,
```

- [ ] **Step 5: Run focused tests and verify they pass**

Run:

```bash
cargo test -p puffer-core generated_video_access_metadata -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit core validation**

Run:

```bash
git add crates/puffer-core/media_runtime.rs crates/puffer-core/media_runtime_generated_preview_tests.rs crates/puffer-core/lib.rs
git commit -m "feat(media): validate generated video access artifacts"
```

---

## Task 2: Daemon Ticket Registry And RPC

**Files:**
- Modify: `crates/puffer-cli/Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/puffer-cli/src/daemon.rs`

- [ ] **Step 1: Write failing RPC tests**

In the `#[cfg(test)] mod tests` import list in `crates/puffer-cli/src/daemon.rs`, add the new symbols as the tests require them:

```rust
handle_create_generated_video_access,
```

Add this helper near existing daemon media test helpers:

```rust
fn write_generated_video_artifact(
    workspace: &std::path::Path,
    artifact_id: &str,
    filename: &str,
    bytes: &[u8],
) -> std::path::PathBuf {
    let video_dir = workspace.join(".puffer/media/artifacts").join(artifact_id);
    std::fs::create_dir_all(&video_dir).unwrap();
    let video_path = video_dir.join(filename);
    std::fs::write(&video_path, bytes).unwrap();
    let sidecar_dir = workspace.join(".puffer/media/artifact-sidecars");
    std::fs::create_dir_all(&sidecar_dir).unwrap();
    std::fs::write(
        sidecar_dir.join(format!("{artifact_id}.json")),
        serde_json::to_string_pretty(&serde_json::json!({
            "id": artifact_id,
            "jobId": "job-video-1",
            "kind": "video",
            "path": video_path,
            "mimeType": "video/mp4",
            "byteCount": bytes.len(),
            "metadata": {},
            "createdAtMs": 1
        }))
        .unwrap(),
    )
    .unwrap();
    video_path
}
```

Add this test near `read_generated_media_preview_resolves_session_cwd`:

```rust
#[test]
fn create_generated_video_access_returns_ticket_path() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::discover(temp.path());
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let workspace = temp.path().join("other-workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let session = session_store.create_session(workspace.clone()).unwrap();
    write_generated_video_artifact(&workspace, "artifact-video-1", "generated.mp4", b"mp4-bytes");
    let state = test_state_with_paths(paths);

    let response = handle_create_generated_video_access(
        &state,
        &serde_json::json!({
            "sessionId": session.id.to_string(),
            "artifactId": "artifact-video-1"
        }),
    )
    .unwrap();

    assert_eq!(response["state"], "available");
    assert_eq!(response["mimeType"], "video/mp4");
    assert_eq!(response["size"], 9);
    assert!(response["expiresAtMs"].as_u64().unwrap() > daemon_now_ms());
    let path = response["path"].as_str().expect("ticket path");
    assert!(path.starts_with("/media/generated-video/"));
    assert!(response.get("url").is_none());
    assert!(!path.contains("token"));
}
```

- [ ] **Step 2: Run the failing daemon test**

Run:

```bash
cargo test -p puffer-cli create_generated_video_access_returns_ticket_path -- --nocapture
```

Expected: FAIL because the handler and state registry do not exist.

- [ ] **Step 3: Add daemon RPC structs and state fields**

Update the puffer-core imports in `crates/puffer-cli/src/daemon.rs`:

```rust
use puffer_core::{
    command_surface, default_effort_level, discover_exact_media_capabilities, dispatch_command,
    enter_plan_mode, execute_connect_flow, execute_user_turn_streaming_with_permissions_and_cancel,
    generate_exact_media_with_cache, generated_video_access_metadata_by_artifact,
    list_exact_media_capabilities_with_cache, provider_preference_family,
    read_generated_media_preview_by_artifact, supported_effort_levels,
    with_user_question_prompt_handler, AppState, BrowserPermissionPromptActionSet,
    BrowserPermissionPromptSource, BrowserPermissionPromptTargetClass, CancelToken,
    ExactMediaDiscoveryCache, ExactMediaGenerationRequest, GeneratedVideoAccessMetadataResult,
    MediaCapabilityView, MessageRole, ModelPreferenceFamily, PermissionPromptAction,
    PermissionPromptRequest, ToolCallRequest, ToolInvocation, TurnStreamEvent,
    UserQuestionPromptRequest, UserQuestionPromptResponse,
};
```

Add these structs near `GeneratedMediaPreviewParams`:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedVideoAccessParams {
    session_id: String,
    artifact_id: String,
}

#[derive(Debug, Clone)]
struct GeneratedVideoTicket {
    path: std::path::PathBuf,
    mime_type: String,
    size: u64,
    expires_at_ms: u64,
}
```

Add this field to `DaemonState`:

```rust
generated_video_tickets: Arc<Mutex<HashMap<String, GeneratedVideoTicket>>>,
```

Initialize it in `DaemonState::load`:

```rust
generated_video_tickets: Arc::new(Mutex::new(HashMap::new())),
```

- [ ] **Step 4: Add ticket creation and RPC handler**

Add these constants and helpers near other daemon helpers:

```rust
const GENERATED_VIDEO_TICKET_TTL_MS: u64 = 10 * 60 * 1000;

fn generated_video_ticket_path(ticket: &str) -> String {
    format!("/media/generated-video/{ticket}")
}

fn random_ticket() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex(&buf)
}
```

Add methods on `DaemonState`:

```rust
impl DaemonState {
    fn prune_expired_generated_video_tickets(&self, now_ms: u64) {
        self.generated_video_tickets
            .lock()
            .unwrap()
            .retain(|_, ticket| ticket.expires_at_ms > now_ms);
    }

    fn insert_generated_video_ticket(
        &self,
        metadata: puffer_core::GeneratedVideoAccessMetadata,
    ) -> (String, GeneratedVideoTicket) {
        let now_ms = daemon_now_ms();
        self.prune_expired_generated_video_tickets(now_ms);
        let token = random_ticket();
        let ticket = GeneratedVideoTicket {
            path: metadata.path,
            mime_type: metadata.mime_type,
            size: metadata.byte_count,
            expires_at_ms: now_ms + GENERATED_VIDEO_TICKET_TTL_MS,
        };
        self.generated_video_tickets
            .lock()
            .unwrap()
            .insert(token.clone(), ticket.clone());
        (token, ticket)
    }
}
```

Add the RPC handler:

```rust
fn handle_create_generated_video_access(state: &DaemonState, params: &Value) -> Result<Value> {
    let input: GeneratedVideoAccessParams =
        serde_json::from_value(params.clone()).context("invalid generated video access params")?;
    let session_store = SessionStore::from_paths(&state.paths)?;
    let cwd = desktop_api::load_session_cwd(&session_store, &input.session_id)?;
    let result = generated_video_access_metadata_by_artifact(&cwd, &input.artifact_id);
    let GeneratedVideoAccessMetadataResult::Available(metadata) = result else {
        return Ok(match result {
            GeneratedVideoAccessMetadataResult::Missing => json!({ "state": "missing" }),
            GeneratedVideoAccessMetadataResult::Unsupported => json!({ "state": "unsupported" }),
            GeneratedVideoAccessMetadataResult::Available(_) => unreachable!(),
        });
    };
    let (token, ticket) = state.insert_generated_video_ticket(metadata);
    Ok(json!({
        "state": "available",
        "path": generated_video_ticket_path(&token),
        "mimeType": ticket.mime_type,
        "size": ticket.size,
        "expiresAtMs": ticket.expires_at_ms
    }))
}
```

Wire it into the request match:

```rust
"create_generated_video_access" => {
    respond!(detached!(|s, p| handle_create_generated_video_access(&s, &p)))
}
```

- [ ] **Step 5: Run the daemon RPC test**

Run:

```bash
cargo test -p puffer-cli create_generated_video_access_returns_ticket_path -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit daemon RPC**

Run:

```bash
git add crates/puffer-cli/src/daemon.rs
git commit -m "feat(daemon): issue generated video access tickets"
```

---

## Task 3: Daemon HTTP Range Serving

**Files:**
- Modify: `crates/puffer-cli/src/daemon.rs`

- [ ] **Step 1: Write range parser and serving tests**

Extend the `#[cfg(test)] mod tests` imports in `crates/puffer-cli/src/daemon.rs` with:

```rust
generated_video_handler, parse_single_byte_range, GeneratedVideoRangeError,
GeneratedVideoTicket,
```

Add these test-only imports:

```rust
use axum::{
    extract::{Path as AxumPath, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
```

Add tests near the new generated video access test:

```rust
#[test]
fn generated_video_range_parser_supports_closed_open_and_suffix_ranges() {
    assert_eq!(parse_single_byte_range("bytes=2-5", 10).unwrap(), Some((2, 5)));
    assert_eq!(parse_single_byte_range("bytes=4-", 10).unwrap(), Some((4, 9)));
    assert_eq!(parse_single_byte_range("bytes=-3", 10).unwrap(), Some((7, 9)));
    assert_eq!(parse_single_byte_range("", 10).unwrap(), None);
}

#[test]
fn generated_video_range_parser_rejects_unsatisfiable_ranges() {
    assert!(matches!(
        parse_single_byte_range("bytes=20-30", 10),
        Err(GeneratedVideoRangeError::Unsatisfiable)
    ));
    assert!(matches!(
        parse_single_byte_range("bytes=8-2", 10),
        Err(GeneratedVideoRangeError::Unsatisfiable)
    ));
}

#[test]
fn generated_video_ticket_lookup_prunes_expired_entries() {
    let temp = tempfile::tempdir().unwrap();
    let state = test_state_with_paths(ConfigPaths::discover(temp.path()));
    state.generated_video_tickets.lock().unwrap().insert(
        "expired".to_string(),
        GeneratedVideoTicket {
            path: temp.path().join("missing.mp4"),
            mime_type: "video/mp4".to_string(),
            size: 0,
            expires_at_ms: daemon_now_ms().saturating_sub(1),
        },
    );

    assert!(state.generated_video_ticket("expired").is_none());
    assert!(state.generated_video_tickets.lock().unwrap().is_empty());
}

#[tokio::test]
async fn generated_video_handler_serves_full_body() {
    let temp = tempfile::tempdir().unwrap();
    let state = Arc::new(test_state_with_paths(ConfigPaths::discover(temp.path())));
    let video_path = temp.path().join("generated.mp4");
    std::fs::write(&video_path, b"0123456789").unwrap();
    state.generated_video_tickets.lock().unwrap().insert(
        "ticket-full".to_string(),
        GeneratedVideoTicket {
            path: video_path,
            mime_type: "video/mp4".to_string(),
            size: 10,
            expires_at_ms: daemon_now_ms() + 60_000,
        },
    );

    let response = generated_video_handler(
        State(state),
        AxumPath("ticket-full".to_string()),
        HeaderMap::new(),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "video/mp4");
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"0123456789");
}

#[tokio::test]
async fn generated_video_handler_serves_single_range() {
    let temp = tempfile::tempdir().unwrap();
    let state = Arc::new(test_state_with_paths(ConfigPaths::discover(temp.path())));
    let video_path = temp.path().join("generated.mp4");
    std::fs::write(&video_path, b"0123456789").unwrap();
    state.generated_video_tickets.lock().unwrap().insert(
        "ticket-1".to_string(),
        GeneratedVideoTicket {
            path: video_path,
            mime_type: "video/mp4".to_string(),
            size: 10,
            expires_at_ms: daemon_now_ms() + 60_000,
        },
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::RANGE, HeaderValue::from_static("bytes=2-5"));

    let response = generated_video_handler(
        State(state),
        AxumPath("ticket-1".to_string()),
        headers,
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response.headers().get(header::CONTENT_RANGE).unwrap(),
        "bytes 2-5/10"
    );
    let body = axum::body::to_bytes(response.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"2345");
}
```

- [ ] **Step 2: Run parser tests and verify failure**

Run:

```bash
cargo test -p puffer-cli generated_video_range_parser generated_video_ticket_lookup -- --nocapture
```

Expected: FAIL because range helpers and ticket lookup are not implemented.

- [ ] **Step 3: Add range parsing and ticket lookup helpers**

Add these types and functions:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeneratedVideoRangeError {
    Invalid,
    Unsatisfiable,
}

fn parse_single_byte_range(
    header: &str,
    size: u64,
) -> std::result::Result<Option<(u64, u64)>, GeneratedVideoRangeError> {
    let header = header.trim();
    if header.is_empty() {
        return Ok(None);
    }
    let Some(range) = header.strip_prefix("bytes=") else {
        return Err(GeneratedVideoRangeError::Invalid);
    };
    if range.contains(',') || size == 0 {
        return Err(GeneratedVideoRangeError::Unsatisfiable);
    }
    let Some((start, end)) = range.split_once('-') else {
        return Err(GeneratedVideoRangeError::Invalid);
    };
    if start.is_empty() {
        let suffix_len = end
            .parse::<u64>()
            .map_err(|_| GeneratedVideoRangeError::Invalid)?;
        if suffix_len == 0 {
            return Err(GeneratedVideoRangeError::Unsatisfiable);
        }
        let start = size.saturating_sub(suffix_len);
        return Ok(Some((start, size - 1)));
    }
    let start = start
        .parse::<u64>()
        .map_err(|_| GeneratedVideoRangeError::Invalid)?;
    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>()
            .map_err(|_| GeneratedVideoRangeError::Invalid)?
    };
    if start >= size || end < start {
        return Err(GeneratedVideoRangeError::Unsatisfiable);
    }
    Ok(Some((start, end.min(size - 1))))
}
```

Add this method:

```rust
impl DaemonState {
    fn generated_video_ticket(&self, token: &str) -> Option<GeneratedVideoTicket> {
        let now_ms = daemon_now_ms();
        self.prune_expired_generated_video_tickets(now_ms);
        self.generated_video_tickets
            .lock()
            .unwrap()
            .get(token)
            .filter(|ticket| ticket.expires_at_ms > now_ms)
            .cloned()
    }
}
```

- [ ] **Step 4: Add streaming dependency**

In `crates/puffer-cli/Cargo.toml`, add beside the daemon dependencies:

```toml
tokio-util = { version = "0.7", features = ["io"] }
```

- [ ] **Step 5: Add HTTP route handler**

Update imports:

```rust
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, Query, State,
    },
    http::{header, HeaderMap, HeaderValue, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::io::{AsyncSeekExt, AsyncReadExt};
use tokio_util::io::ReaderStream;
```

Add the route:

```rust
let app = Router::new()
    .route("/ws", get(ws_handler))
    .route("/media/generated-video/{ticket}", get(generated_video_handler))
    .with_state(state.clone());
```

Add the handler:

```rust
async fn generated_video_handler(
    State(state): State<Arc<DaemonState>>,
    AxumPath(ticket): AxumPath<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(ticket) = state.generated_video_ticket(&ticket) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let file = match tokio::fs::File::open(&ticket.path).await {
        Ok(file) => file,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let size = match file.metadata().await {
        Ok(metadata) => metadata.len(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let range_header = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let range = match parse_single_byte_range(range_header, size) {
        Ok(range) => range,
        Err(GeneratedVideoRangeError::Unsatisfiable) => {
            return StatusCode::RANGE_NOT_SATISFIABLE.into_response()
        }
        Err(GeneratedVideoRangeError::Invalid) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, ticket.mime_type)
        .header(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    let (body, content_length) = if let Some((start, end)) = range {
        builder = builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
        let mut file = file;
        if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
            return StatusCode::NOT_FOUND.into_response();
        }
        let length = end - start + 1;
        (Body::from_stream(ReaderStream::new(file.take(length))), length)
    } else {
        builder = builder.status(StatusCode::OK);
        (Body::from_stream(ReaderStream::new(file)), size)
    };
    builder
        .header(header::CONTENT_LENGTH, content_length.to_string())
        .body(body)
        .unwrap()
        .into_response()
}
```

- [ ] **Step 6: Run daemon tests**

Run:

```bash
cargo test -p puffer-cli generated_video_range_parser generated_video_ticket_lookup create_generated_video_access_returns_ticket_path -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit HTTP serving**

Run:

```bash
git add crates/puffer-cli/Cargo.toml Cargo.lock crates/puffer-cli/src/daemon.rs
git commit -m "feat(daemon): stream generated video tickets"
```

---

## Task 4: Frontend API And Type Contract

**Files:**
- Modify: `apps/puffer-desktop/src/lib/types.ts`
- Modify: `apps/puffer-desktop/src/lib/api/daemonClient.ts`
- Modify: `apps/puffer-desktop/src/lib/api/desktop.ts`
- Modify: `apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts`
- Modify: `apps/puffer-desktop/src/lib/agentTurnAttachments.ts`

- [ ] **Step 1: Add failing frontend API tests**

In `apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts`, add:

```ts
const generatedVideoAttachment: MessageAttachment = {
  id: "generated-video:artifact-video-1",
  name: "Generated video",
  mimeType: "video/mp4",
  size: 9,
  extension: "MP4",
  kind: "video",
  state: "available",
  source: {
    kind: "generated_media",
    jobId: "job-video-1",
    artifactId: "artifact-video-1",
    index: 0,
    localPath: "/tmp/puffer/.puffer/media/artifacts/artifact-video-1/generated.mp4"
  }
};

test("creates generated video access URLs from daemon paths", async () => {
  const { createGeneratedVideoAccess } = await import("./desktop");
  request.mockResolvedValueOnce({
    state: "available",
    path: "/media/generated-video/ticket-1",
    mimeType: "video/mp4",
    size: 9,
    expiresAtMs: 1234
  });

  await expect(
    createGeneratedVideoAccess("session-1", "artifact-video-1")
  ).resolves.toEqual({
    state: "available",
    url: "http://127.0.0.1:1421/media/generated-video/ticket-1",
    mimeType: "video/mp4",
    size: 9,
    expiresAtMs: 1234
  });

  expect(request).toHaveBeenCalledWith("create_generated_video_access", {
    sessionId: "session-1",
    artifactId: "artifact-video-1"
  });
});

test("formats video attachment prompt lines", async () => {
  const { formatAgentTurnAttachmentLine } = await import("../agentTurnAttachments");
  expect(formatAgentTurnAttachmentLine(generatedVideoAttachment)).toBe("[Video: Generated video]");
});

test("revokes only blob attachment preview URLs", async () => {
  const { revokeMessageAttachmentPreviews } = await import("../agentTurnAttachments");
  const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});

  revokeMessageAttachmentPreviews([
    { ...generatedVideoAttachment, previewUrl: "http://127.0.0.1:1421/media/generated-video/ticket-1" },
    { ...attachment, previewUrl: "blob:image-preview" }
  ]);

  expect(revoke).toHaveBeenCalledTimes(1);
  expect(revoke).toHaveBeenCalledWith("blob:image-preview");
  revoke.mockRestore();
});
```

Update the `daemonClient` mock in that test to return a client with a media URL helper:

```ts
ensureLocalDaemonClient: async () => ({
  request,
  httpUrl: (path: string) => `http://127.0.0.1:1421${path}`
}),
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/api/desktop.attachment-types.test.ts
```

Expected: FAIL because `video`, `createGeneratedVideoAccess`, and `httpUrl` are missing.

- [ ] **Step 3: Update frontend types**

In `apps/puffer-desktop/src/lib/types.ts`, change:

```ts
export type AgentTurnAttachmentKind = "image" | "file";
```

to:

```ts
export type AgentTurnAttachmentKind = "image" | "file" | "video";
```

Add:

```ts
export type GeneratedVideoAccessResult =
  | { state: "available"; url: string; mimeType: string; size: number; expiresAtMs: number }
  | { state: "missing" }
  | { state: "unsupported" };
```

- [ ] **Step 4: Add daemon HTTP URL helper**

In `apps/puffer-desktop/src/lib/api/daemonClient.ts`, add this method to `DaemonClient`:

```ts
  httpUrl(path: string): string {
    if (!this.useWebSocket) {
      throw new Error("Daemon HTTP media URLs require a WebSocket daemon handshake.");
    }
    const url = new URL(this.handshake.url);
    url.protocol = url.protocol === "wss:" ? "https:" : "http:";
    url.pathname = path.startsWith("/") ? path : `/${path}`;
    url.search = "";
    url.hash = "";
    return url.toString();
  }
```

- [ ] **Step 5: Add desktop API function**

In `apps/puffer-desktop/src/lib/api/desktop.ts`, add a daemon response type near other media types:

```ts
type BackendGeneratedVideoAccessResult =
  | { state: "available"; path: string; mimeType: string; size: number; expiresAtMs: number }
  | { state: "missing" }
  | { state: "unsupported" };
```

Add:

```ts
export async function createGeneratedVideoAccess(
  sessionId: string,
  artifactId: string
): Promise<GeneratedVideoAccessResult> {
  const client = await ensureLocalDaemonClient();
  const result = await client.request<BackendGeneratedVideoAccessResult>(
    "create_generated_video_access",
    { sessionId, artifactId }
  );
  if (result.state !== "available") return result;
  return {
    state: "available",
    url: client.httpUrl(result.path),
    mimeType: result.mimeType,
    size: result.size,
    expiresAtMs: result.expiresAtMs
  };
}
```

Import `GeneratedVideoAccessResult` from `../types`.

- [ ] **Step 6: Update attachment formatting and URL cleanup**

In `apps/puffer-desktop/src/lib/agentTurnAttachments.ts`, change:

```ts
const label = attachment.kind === "image" ? "Image" : "File";
```

to:

```ts
const label =
  attachment.kind === "image" ? "Image" : attachment.kind === "video" ? "Video" : "File";
```

Change preview revocation:

```ts
if (attachment.previewUrl) URL.revokeObjectURL(attachment.previewUrl);
```

to:

```ts
if (attachment.previewUrl?.startsWith("blob:")) URL.revokeObjectURL(attachment.previewUrl);
```

- [ ] **Step 7: Run frontend API tests**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/api/desktop.attachment-types.test.ts
```

Expected: PASS.

- [ ] **Step 8: Commit frontend API contract**

Run:

```bash
git add apps/puffer-desktop/src/lib/types.ts apps/puffer-desktop/src/lib/api/daemonClient.ts apps/puffer-desktop/src/lib/api/desktop.ts apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts apps/puffer-desktop/src/lib/agentTurnAttachments.ts
git commit -m "feat(desktop): add generated video access API"
```

---

## Task 5: Generated Video Card Rendering

**Files:**
- Modify: `apps/puffer-desktop/src/lib/screens/agent/MessageAttachmentPreviewStrip.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte`

- [ ] **Step 1: Add failing Playwright coverage for a persisted generated video card**

In `apps/puffer-desktop/tests/chat-session-ui.spec.ts`, add:

```ts
test("generated video attachment renders a playable card", async ({ page }) => {
  const sessionId = "session-generated-video-card";
  const daemon = new FakeDaemon({
    sessions: [
      {
        sessionId,
        displayName: "Generated video card",
        title: "Generated video card",
        cwd: "/tmp/puffer",
        folderPath: "/tmp/puffer",
        updatedAtMs: baseTime,
        createdAtMs: baseTime - 60_000,
        eventCount: 1,
        timeline: [
          {
            kind: "assistant_message",
            id: "generated-video-message",
            text: "Generated a video.",
            createdAtMs: baseTime - 30_000,
            attachments: [
              {
                id: "generated-video:artifact-video-card",
                name: "Generated video",
                mimeType: "video/mp4",
                size: 9,
                extension: "MP4",
                kind: "video",
                state: "available",
                source: {
                  kind: "generated_media",
                  jobId: "job-video-card",
                  artifactId: "artifact-video-card",
                  index: 0,
                  localPath: "/tmp/puffer/.puffer/media/artifacts/artifact-video-card/generated.mp4"
                }
              }
            ]
          }
        ]
      }
    ]
  });
  daemon.seedGeneratedVideoAccess(sessionId, "artifact-video-card", {
    state: "available",
    path: "/media/generated-video/fake-video-card",
    mimeType: "video/mp4",
    size: 9,
    expiresAtMs: baseTime + 60_000,
    bytes: Buffer.from("mp4-bytes")
  });
  await daemon.install(page);
  await daemon.open(page);
  await openSession(page, /Generated video card/);

  const card = page.getByRole("button", { name: "Open video attachment Generated video" });
  await expect(card).toBeVisible();
  await expect(card.locator("video")).toHaveAttribute("preload", "metadata");
  await expect(card.locator('[data-testid="video-play-indicator"]')).toBeVisible();
  await expect(page.locator(".pf-msg").filter({ has: card })).not.toContainText("/tmp/puffer");
});
```

- [ ] **Step 2: Run the test and verify failure**

Run:

```bash
cd apps/puffer-desktop
npx playwright test tests/chat-session-ui.spec.ts -g "generated video attachment renders a playable card"
```

Expected: FAIL because fake daemon video access and video card rendering are missing.

- [ ] **Step 3: Request generated video access in preview strip**

In `MessageAttachmentPreviewStrip.svelte`, import:

```ts
import { createGeneratedVideoAccess, readMessageAttachmentPreview } from "../../api/desktop";
```

Change `needsPreview`:

```ts
function needsPreview(attachment: MessageAttachment): boolean {
  return (
    (attachment.kind === "image" || attachment.kind === "video") &&
    !attachment.previewUrl &&
    attachment.state !== "missing"
  );
}
```

Change `loadPreview` so video generated media uses video access:

```ts
const preview =
  attachment.kind === "video" && attachment.source.kind === "generated_media"
    ? await createGeneratedVideoAccess(targetSessionId, attachment.source.artifactId)
    : await readMessageAttachmentPreview(targetSessionId, attachment);
if (destroyed || !previewStillNeeded(targetSessionId, attachment.id)) return;
if (preview.state !== "available") {
  previewMisses.set(key, missState);
  return;
}

const previewUrl =
  "url" in preview
    ? preview.url
    : URL.createObjectURL(new Blob([new Uint8Array(preview.bytes)], { type: preview.mimeType }));
const previous = previewUrls[key];
if (previous?.startsWith("blob:")) URL.revokeObjectURL(previous);
previewMisses.delete(key);
previewUrls = { ...previewUrls, [key]: previewUrl };
```

Update `attachmentsForDisplay` to skip only attachments that already have `previewUrl`:

```ts
if (attachment.previewUrl) return attachment;
```

Keep `revokePreviewUrls` blob-only:

```ts
if (previewUrl.startsWith("blob:")) URL.revokeObjectURL(previewUrl);
```

- [ ] **Step 4: Render video cards**

In `AttachmentPreviewStrip.svelte`, change `attachmentOpenLabel`:

```ts
function attachmentOpenLabel(attachment: AttachmentPreviewItem): string {
  return attachment.kind === "image"
    ? `Open image attachment ${attachment.name}`
    : attachment.kind === "video"
      ? `Open video attachment ${attachment.name}`
      : `Open attachment details for ${attachment.name}`;
}
```

Extend the snippet:

```svelte
{:else if attachment.previewUrl && attachment.kind === "video"}
  <div class="pf-attachment-video-thumb">
    <video src={attachment.previewUrl} preload="metadata" muted playsinline aria-label={attachment.name}></video>
    <span class="pf-attachment-video-play" data-testid="video-play-indicator" aria-hidden="true">
      <Icon name="play" size={18} />
    </span>
  </div>
```

Add CSS:

```css
.pf-attachment-video-thumb {
  position: relative;
  width: 112px;
  height: 64px;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--muted);
}
.pf-attachment-video-thumb video {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: cover;
}
.pf-attachment-video-play {
  position: absolute;
  inset: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: white;
  background: color-mix(in oklab, black 24%, transparent);
}
```

- [ ] **Step 5: Update fake daemon support**

In `apps/puffer-desktop/tests/support/fakeDaemon.ts`, add:

```ts
type GeneratedVideoAccessFixture =
  | { state: "available"; path: string; mimeType: string; size: number; expiresAtMs: number; bytes?: Buffer }
  | { state: "missing" }
  | { state: "unsupported" };
```

Add a map:

```ts
private generatedVideoAccesses = new Map<string, GeneratedVideoAccessFixture>();
```

Add method:

```ts
seedGeneratedVideoAccess(
  sessionId: string,
  artifactId: string,
  access: GeneratedVideoAccessFixture
): void {
  this.generatedVideoAccesses.set(this.generatedMediaPreviewKey(sessionId, artifactId), access);
}
```

Handle RPC:

```ts
case "create_generated_video_access":
  return this.createGeneratedVideoAccess(request.params);
```

Add:

```ts
private createGeneratedVideoAccess(params: JsonRecord): GeneratedVideoAccessFixture {
  const sessionId = String(params.sessionId ?? "");
  const artifactId = String(params.artifactId ?? "");
  const access = this.generatedVideoAccesses.get(this.generatedMediaPreviewKey(sessionId, artifactId));
  if (!access) return { state: "missing" };
  if (access.state !== "available") return access;
  const { bytes: _bytes, ...wire } = access;
  return wire;
}
```

In `install(page)`, after `routeWebSocket`, add:

```ts
const httpOrigin = expectedUrl.origin.replace(/^ws/, "http");
await page.route(`${httpOrigin}/media/generated-video/**`, async (route) => {
  const path = new URL(route.request().url()).pathname;
  const access = Array.from(this.generatedVideoAccesses.values()).find(
    (entry) => entry.state === "available" && entry.path === path
  );
  if (!access || access.state !== "available") {
    await route.fulfill({ status: 404, body: "" });
    return;
  }
  await route.fulfill({
    status: 200,
    contentType: access.mimeType,
    body: access.bytes ?? Buffer.from("mp4-bytes")
  });
});
```

- [ ] **Step 6: Run the card test**

Run:

```bash
cd apps/puffer-desktop
npx playwright test tests/chat-session-ui.spec.ts -g "generated video attachment renders a playable card"
```

Expected: PASS.

- [ ] **Step 7: Commit generated video card**

Run:

```bash
git add apps/puffer-desktop/src/lib/screens/agent/MessageAttachmentPreviewStrip.svelte apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte apps/puffer-desktop/tests/support/fakeDaemon.ts apps/puffer-desktop/tests/chat-session-ui.spec.ts
git commit -m "feat(desktop): render generated video cards"
```

---

## Task 6: Overlay Video Playback

**Files:**
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.ts`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.test.ts`
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`

- [ ] **Step 1: Add failing overlay test**

Extend the Playwright test from Task 5 after the card assertions:

```ts
await card.click();
const dialog = page.getByRole("dialog", { name: "Generated video" });
await expect(dialog).toBeVisible();
const video = dialog.locator("video");
await expect(video).toBeVisible();
await expect(video).toHaveAttribute("controls", "");
await expect(video).toHaveAttribute("autoplay", "");
```

- [ ] **Step 2: Run overlay test and verify failure**

Run:

```bash
cd apps/puffer-desktop
npx playwright test tests/chat-session-ui.spec.ts -g "generated video attachment renders a playable card"
```

Expected: FAIL because overlay still shows unavailable state for videos.

- [ ] **Step 3: Render video in the overlay**

In `AttachmentOverlay.svelte`, add:

```ts
let canPreviewVideo = $derived(Boolean(attachment?.kind === "video" && attachment.previewUrl));
```

Add a branch after image rendering:

```svelte
{:else if canPreviewVideo && attachment.previewUrl}
  <div class="pf-attachment-video-frame">
    <video src={attachment.previewUrl} controls autoplay playsinline></video>
  </div>
```

Add CSS:

```css
.pf-attachment-video-frame {
  min-height: 0;
  display: grid;
  place-items: center;
  background: black;
}
.pf-attachment-video-frame video {
  width: 100%;
  max-height: calc(90vh - 82px);
  display: block;
  background: black;
}
```

- [ ] **Step 4: Request video URL when opening attachments**

In `AgentDetail.svelte`, import:

```ts
import {
  createGeneratedVideoAccess,
  readChatAttachmentPreview,
  type AgentTurnSubmitOptions
} from "../../api/desktop";
```

Update `openAttachmentIntent` so generated videos request access:

```ts
if (attachment.kind === "video" && attachment.source.kind === "generated_media" && !attachment.previewUrl) {
  const sessionId = session?.id ?? null;
  if (!sessionId) {
    openAttachment = attachment;
    return;
  }
  const access = await createGeneratedVideoAccess(sessionId, attachment.source.artifactId).catch(() => ({
    state: "missing" as const
  }));
  if ((session?.id ?? null) !== sessionId) return;
  openAttachment = access.state === "available" ? { ...attachment, previewUrl: access.url } : attachment;
  return;
}
```

Keep the existing image path unchanged after this branch.

- [ ] **Step 5: Generalize overlay action helper narrowly**

In `imageOverlayAction.ts`, keep the exported function name if changing callers would add churn, but allow generated video local paths:

```ts
if (!attachment || (attachment.kind !== "image" && attachment.kind !== "video")) return null;
```

For `remote_url`, only images should download:

```ts
case "remote_url":
  return attachment.kind === "image" && attachment.source.url
    ? { kind: "download", url: attachment.source.url, suggestedName: attachment.name }
    : null;
```

For generated media, return open folder for image or video local paths.

Add a test in `imageOverlayAction.test.ts`:

```ts
test("returns open folder for generated video with a local path", () => {
  expect(
    imageOverlayAction({
      id: "generated-video:artifact-1",
      name: "Generated video",
      mimeType: "video/mp4",
      size: 9,
      extension: "MP4",
      kind: "video",
      state: "available",
      source: {
        kind: "generated_media",
        jobId: "job-1",
        artifactId: "artifact-1",
        index: 0,
        localPath: "/tmp/puffer/.puffer/media/artifacts/artifact-1/generated.mp4"
      }
    })
  ).toEqual({
    kind: "open_folder",
    path: "/tmp/puffer/.puffer/media/artifacts/artifact-1/generated.mp4"
  });
});
```

- [ ] **Step 6: Run overlay tests**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/screens/agent/imageOverlayAction.test.ts
npx playwright test tests/chat-session-ui.spec.ts -g "generated video attachment renders a playable card"
```

Expected: PASS.

- [ ] **Step 7: Commit overlay playback**

Run:

```bash
git add apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.ts apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.test.ts apps/puffer-desktop/tests/chat-session-ui.spec.ts
git commit -m "feat(desktop): play generated videos in overlay"
```

---

## Task 7: Live `/video` Result Attachments

**Files:**
- Modify: `apps/puffer-desktop/src/App.svelte`
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`

- [ ] **Step 1: Add failing live `/video` test**

Add this test near existing `/video` tests:

```ts
test("video slash success appends a generated video attachment", async ({ page }) => {
  const sessionId = "session-video-preview-success";
  const daemon = new FakeDaemon({
    sessions: [
      {
        sessionId,
        displayName: "Video preview success",
        title: "Video preview success",
        cwd: "/tmp/puffer",
        folderPath: "/tmp/puffer",
        updatedAtMs: baseTime,
        createdAtMs: baseTime - 60_000,
        eventCount: 1,
        timeline: [
          {
            kind: "assistant_message",
            id: "video-preview-seed",
            text: "Generate videos here.",
            createdAtMs: baseTime - 30_000
          }
        ]
      }
    ]
  });
  daemon.setSettingsConfig({
    media: {
      image: null,
      video: {
        providerId: "runway",
        modelId: "gen-4",
        operation: "generate",
        adapter: "replicate_video",
        parameters: { duration: "8", aspect_ratio: "16:9" }
      }
    }
  });
  daemon.setGeneratedMediaResult({
    jobId: "media-job-video-success",
    requestedCount: 1,
    kind: "video",
    artifacts: [
      {
        artifactId: "artifact-video-live",
        index: 0,
        path: "/tmp/puffer/.puffer/media/artifacts/artifact-video-live/generated.mp4",
        mimeType: "video/mp4",
        size: 9
      }
    ],
    status: "succeeded"
  });
  daemon.seedGeneratedVideoAccess(sessionId, "artifact-video-live", {
    state: "available",
    path: "/media/generated-video/fake-video-live",
    mimeType: "video/mp4",
    size: 9,
    expiresAtMs: baseTime + 60_000,
    bytes: Buffer.from("mp4-bytes")
  });
  await daemon.install(page);
  await daemon.open(page);
  await openSession(page, /Video preview success/);

  await page.locator(".pf-composer textarea").fill("/video animate this logo");
  await page.getByRole("button", { name: "Send" }).click();

  const card = page.getByRole("button", { name: "Open video attachment Generated video" });
  await expect(card).toBeVisible();
  await expect(page.locator(".pf-msg").filter({ has: card })).not.toContainText("media-job-video-success");
});
```

- [ ] **Step 2: Run the failing live test**

Run:

```bash
cd apps/puffer-desktop
npx playwright test tests/chat-session-ui.spec.ts -g "video slash success appends a generated video attachment"
```

Expected: FAIL because `/video` only sets a status message.

- [ ] **Step 3: Add generated video attachment helpers**

In `App.svelte`, add:

```ts
function generatedVideoExtension(mimeType: string): string {
  switch (mimeType.toLowerCase()) {
    case "video/mp4":
      return "MP4";
    case "video/webm":
      return "WEBM";
    default:
      return "VIDEO";
  }
}

function generatedVideoAttachmentId(artifactId: string): string {
  return `generated-video:${artifactId}`;
}

function generatedVideoAttachment(
  jobId: string,
  artifact: GeneratedMediaArtifactResult
): MessageAttachment {
  return {
    id: generatedVideoAttachmentId(artifact.artifactId),
    name: "Generated video",
    mimeType: artifact.mimeType || "video/*",
    size: artifact.size || 0,
    extension: generatedVideoExtension(artifact.mimeType || "video/*"),
    kind: "video",
    state: artifact.path ? "available" : "missing",
    source: {
      kind: "generated_media",
      jobId,
      artifactId: artifact.artifactId,
      index: artifact.index,
      ...(artifact.path ? { localPath: artifact.path } : {}),
      ...(artifact.remoteSourceUrl ? { remoteSourceUrl: artifact.remoteSourceUrl } : {})
    }
  };
}
```

Add:

```ts
async function appendGeneratedVideoPreview(
  sessionId: string,
  result: GenerateMediaResult
): Promise<void> {
  const artifacts = result.artifacts ?? [];
  if (artifacts.length === 0) return;
  const attachments = artifacts.map((artifact) =>
    generatedVideoAttachment(result.jobId, artifact)
  );
  if (selectedSession?.id !== sessionId) return;
  appendLive({
    id: `${GENERATED_IMAGE_PREVIEW_ID_PREFIX}video-${result.jobId}`,
    kind: "assistant",
    title: "Assistant",
    summary: artifacts.length === 1 ? "Generated video" : `Generated ${artifacts.length} videos`,
    body: "",
    meta: [],
    status: result.status,
    attachments
  });
}
```

In `submitMediaSlash`, replace the video branch:

```ts
} else {
  await appendGeneratedVideoPreview(sessionId, result);
  statusMessage = `Video generation ${result.status}.`;
}
```

- [ ] **Step 4: Run live `/video` test**

Run:

```bash
cd apps/puffer-desktop
npx playwright test tests/chat-session-ui.spec.ts -g "video slash success appends a generated video attachment"
```

Expected: PASS.

- [ ] **Step 5: Commit live `/video` result**

Run:

```bash
git add apps/puffer-desktop/src/App.svelte apps/puffer-desktop/tests/chat-session-ui.spec.ts
git commit -m "feat(desktop): show live generated video results"
```

---

## Task 8: Specs And Full Verification

**Files:**
- Create: `specs/puffer-cli/166.md`
- Create: `specs/puffer-desktop/682.md`

- [ ] **Step 1: Add component update specs**

Create `specs/puffer-cli/166.md`:

```markdown
# Generated Video Access Tickets

## Scope

The daemon now exposes generated video playback through short-lived ticket paths
and an HTTP range route.

## Behavior

- `create_generated_video_access` resolves `sessionId` and `artifactId` through
  session cwd and media artifact sidecars.
- Only `video/mp4` and `video/webm` generated video artifacts under
  `.puffer/media/artifacts/<artifact_id>/` are eligible.
- The RPC returns a ticket path, MIME type, size, and expiry timestamp; it does
  not return the daemon token or a guessed localhost URL.
- `GET /media/generated-video/<ticket>` serves full responses and single byte
  ranges.
- Expired tickets are pruned opportunistically.

## Safety

Non-video artifacts, unsupported MIME types, symlink escapes, path escapes, and
invalid ticket paths fail closed.
```

Create `specs/puffer-desktop/682.md`:

```markdown
# Generated Video Cards And Overlay Playback

## Scope

Generated video attachments now render in desktop chat and play through the
existing attachment overlay.

## Behavior

- `MessageAttachment.kind` accepts `video`.
- Generated video attachments request `create_generated_video_access` and build
  playback URLs from the active daemon handshake.
- Video cards render a native metadata-preloaded `<video>` thumbnail with a play
  affordance.
- Clicking a video card opens the attachment overlay with video controls.
- Live `/video` successes append `Generated video` attachments instead of only
  status text.
- Generated image preview behavior is unchanged.

## Safety

The UI does not expose local generated video paths in chat text. Blob URLs are
revoked by existing cleanup; daemon HTTP media URLs expire server-side.
```

- [ ] **Step 2: Run focused Rust tests**

Run:

```bash
cargo test -p puffer-core generated_video_access_metadata -- --nocapture
cargo test -p puffer-cli generated_video -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Run focused frontend tests**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/api/desktop.attachment-types.test.ts src/lib/screens/agent/imageOverlayAction.test.ts
npx playwright test tests/chat-session-ui.spec.ts -g "generated video|video slash success"
```

Expected: PASS.

- [ ] **Step 4: Run static frontend check**

Run:

```bash
cd apps/puffer-desktop
npm run check
```

Expected: PASS.

- [ ] **Step 5: Run broader workspace tests**

Run:

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 6: Commit specs and verification follow-through**

Run:

```bash
git add specs/puffer-cli/166.md specs/puffer-desktop/682.md
git commit -m "docs(media): record generated video playback updates"
```

---

## Final Acceptance Checklist

- [ ] Milhous generated video appears as a `Generated video` card after reload.
- [ ] The card does not display the local artifact path.
- [ ] The card uses a daemon HTTP media URL derived from the active handshake.
- [ ] Clicking the card opens an overlay with video controls.
- [ ] Generated images still load via existing image preview bytes.
- [ ] `/video` live success appends a generated video attachment.
- [ ] Rust tests cover path escape, MIME rejection, missing files, ticket expiry, and range parsing.
- [ ] Playwright covers persisted video card, overlay playback, missing access fallback, and live `/video`.
