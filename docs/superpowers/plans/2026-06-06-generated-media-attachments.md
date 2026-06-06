# Generated Media Attachments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show persisted and live `ImageGeneration` results as assistant-side image attachments with the existing thumbnail and overlay UX.

**Architecture:** Add a typed generated-media attachment source to timeline DTOs, synthesize assistant attachments from structured `ImageGeneration` tool outputs and media sidecars, and read preview bytes by `sessionId + artifactId`. Keep tool cards as execution logs and keep image bytes out of timeline responses.

**Tech Stack:** Rust (`puffer-core`, `puffer-cli`, Tauri backend), Svelte/TypeScript (`apps/puffer-desktop`), Playwright desktop UI tests, Cargo unit tests.

---

## Recheck Outcome

The durable plan intentionally replaces the older transient thumbnail-only plan:

- Superseded plan: `docs/superpowers/plans/2026-06-06-imagegen-assistant-attachment-preview.md`
- Current spec: `docs/superpowers/specs/2026-06-06-generated-media-attachments-design.md`

Scope was reduced during review:

- no text-path scanning;
- no arbitrary local file preview;
- no gallery, artifact browser, reveal action, retry action, or provider-specific viewer;
- no global thumbnail cache;
- no turn reconstruction beyond a single linear transcript pass.

## File Structure

- Modify `crates/puffer-core/media_runtime.rs`
  - Public generated-media metadata and preview helpers by artifact id.
  - MIME sniffing for generated previews.

- Modify `crates/puffer-cli/src/desktop_api_types.rs`
  - Add typed attachment source.
  - Add `attachments` to `AssistantMessage`.

- Modify `crates/puffer-cli/src/desktop_api.rs`
  - Synthesize generated image attachments while loading timeline items.
  - Add focused unit tests for CLI timeline behavior.

- Modify `crates/puffer-cli/src/daemon.rs`
  - Change generated preview RPC params from `path` to `sessionId + artifactId`.
  - Resolve target cwd from the session store.

- Modify `apps/puffer-desktop/src-tauri/src/dtos.rs`
  - Mirror desktop DTO attachment source and assistant attachments.

- Modify `apps/puffer-desktop/src-tauri/src/session_data.rs`
  - Mirror CLI timeline synthesis for Tauri direct session loading.

- Modify `apps/puffer-desktop/src-tauri/src/backend.rs`
  - Mirror generated preview RPC behavior for Tauri backend tests.

- Modify `apps/puffer-desktop/src/lib/types.ts`
  - Add `AttachmentPreviewSource`.

- Modify `apps/puffer-desktop/src/lib/api/desktop.ts`
  - Normalize attachment source.
  - Normalize assistant attachments.
  - Add generated preview API by session id and artifact id.
  - Add one helper that reads preview bytes based on attachment source.

- Modify `apps/puffer-desktop/src/lib/screens/agent/MessageAttachmentPreviewStrip.svelte`
  - Use the source-aware preview helper instead of always reading chat attachments.

- Modify `apps/puffer-desktop/tests/support/fakeDaemon.ts`
  - Support source-aware assistant attachments and generated preview requests.

- Modify `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Add persisted generated-image attachment UI coverage.
  - Update old generated preview request expectations from `path` to `sessionId + artifactId`.

## Task 1: Core Generated Media Preview Helpers

**Files:**
- Modify: `crates/puffer-core/media_runtime.rs`

- [ ] **Step 1: Add failing media-runtime tests**

Add tests in the existing `#[cfg(test)]` module in `crates/puffer-core/media_runtime.rs`:

```rust
#[test]
fn generated_media_preview_by_artifact_uses_sidecar_path() {
    let workspace = tempfile::tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let image_path = service
        .write_image_artifact_bytes("artifact-1", "image.jpeg", &[0xff, 0xd8, 0xff, 0xd9])
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: image_path,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    let result = read_generated_media_preview_by_artifact(workspace.path(), "artifact-1");

    assert_eq!(
        result,
        GeneratedMediaPreviewResult::Available {
            mime_type: "image/jpeg".to_string(),
            bytes: vec![0xff, 0xd8, 0xff, 0xd9],
        }
    );
}

#[test]
fn generated_media_preview_by_artifact_rejects_symlink_escape() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_image = outside.path().join("image.jpeg");
    std::fs::write(&outside_image, [0xff, 0xd8, 0xff, 0xd9]).unwrap();
    let link_dir = workspace.path().join(".puffer/media/images/artifact-1");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link = link_dir.join("image.jpeg");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_image, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside_image, &link).unwrap();

    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: link,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        read_generated_media_preview_by_artifact(workspace.path(), "artifact-1"),
        GeneratedMediaPreviewResult::Unsupported
    );
}

#[test]
fn generated_media_preview_by_artifact_sniffs_mime_when_extension_lies() {
    let workspace = tempfile::tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let image_path = service
        .write_image_artifact_bytes("artifact-1", "image.png", &[0xff, 0xd8, 0xff, 0xd9])
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: image_path,
            mime_type: "image/png".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    let result = read_generated_media_preview_by_artifact(workspace.path(), "artifact-1");

    assert!(matches!(
        result,
        GeneratedMediaPreviewResult::Available { mime_type, .. } if mime_type == "image/jpeg"
    ));
}
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
cargo test -p puffer-core generated_media_preview_by_artifact -- --nocapture
```

Expected: fail because `read_generated_media_preview_by_artifact` does not exist.

- [ ] **Step 3: Implement artifact-id preview helpers**

Add public helpers in `crates/puffer-core/media_runtime.rs`:

```rust
/// Carries generated image attachment metadata without image bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedMediaAttachmentMetadata {
    pub artifact_id: String,
    pub mime_type: String,
    pub byte_count: u64,
    pub state: String,
}

/// Loads generated image metadata by artifact id.
pub fn generated_media_attachment_metadata(
    workspace_root: impl AsRef<Path>,
    artifact_id: &str,
) -> Option<GeneratedMediaAttachmentMetadata> {
    let service = MediaGenerationService::new(workspace_root.as_ref());
    let artifact = service.load_artifact(artifact_id).ok()?;
    if artifact.kind != MediaKind::Image {
        return None;
    }
    let state = if artifact.path.is_file() { "available" } else { "missing" };
    Some(GeneratedMediaAttachmentMetadata {
        artifact_id: artifact.id,
        mime_type: canonical_generated_image_mime_type(&artifact.path, Some(&artifact.mime_type))
            .unwrap_or_else(|| artifact.mime_type.clone()),
        byte_count: artifact.byte_count,
        state: state.to_string(),
    })
}

/// Reads generated image preview bytes by artifact id.
pub fn read_generated_media_preview_by_artifact(
    workspace_root: impl AsRef<Path>,
    artifact_id: &str,
) -> GeneratedMediaPreviewResult {
    let service = MediaGenerationService::new(workspace_root.as_ref());
    let artifact = match service.load_artifact(artifact_id) {
        Ok(artifact) => artifact,
        Err(_) => return GeneratedMediaPreviewResult::Missing,
    };
    if artifact.kind != MediaKind::Image {
        return GeneratedMediaPreviewResult::Unsupported;
    }
    let image_root = generated_media_image_root(workspace_root.as_ref());
    read_generated_media_preview_from_root_with_mime(
        &image_root,
        &artifact.path,
        Some(&artifact.mime_type),
    )
}
```

Refactor the existing path helper to share validation with a new private helper:

```rust
fn read_generated_media_preview_from_root_with_mime(
    image_root: &Path,
    path: &Path,
    sidecar_mime_type: Option<&str>,
) -> GeneratedMediaPreviewResult {
    let canonical_path = match std::fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return if missing_generated_media_path_is_under_root(image_root, path) {
                GeneratedMediaPreviewResult::Missing
            } else {
                GeneratedMediaPreviewResult::Unsupported
            };
        }
        Err(_) => return GeneratedMediaPreviewResult::Missing,
    };
    let canonical_root = match std::fs::canonicalize(image_root) {
        Ok(path) => path,
        Err(_) => return GeneratedMediaPreviewResult::Unsupported,
    };
    if !canonical_path.starts_with(canonical_root) {
        return GeneratedMediaPreviewResult::Unsupported;
    }
    let metadata = match std::fs::metadata(&canonical_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return GeneratedMediaPreviewResult::Missing;
        }
        Err(_) => return GeneratedMediaPreviewResult::Missing,
    };
    if !metadata.is_file() {
        return GeneratedMediaPreviewResult::Unsupported;
    }
    let bytes = match std::fs::read(&canonical_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return GeneratedMediaPreviewResult::Missing;
        }
        Err(_) => return GeneratedMediaPreviewResult::Missing,
    };
    let Some(mime_type) = sniff_generated_image_mime_type(&bytes)
        .or_else(|| canonical_sidecar_image_mime_type(sidecar_mime_type))
        .or_else(|| generated_image_mime_type(&canonical_path))
    else {
        return GeneratedMediaPreviewResult::Unsupported;
    };
    GeneratedMediaPreviewResult::Available {
        mime_type: mime_type.to_string(),
        bytes,
    }
}
```

Add MIME helpers:

```rust
fn canonical_sidecar_image_mime_type(value: Option<&str>) -> Option<&'static str> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/webp" => Some("image/webp"),
        _ => None,
    }
}

fn canonical_generated_image_mime_type(path: &Path, sidecar_mime_type: Option<&str>) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    sniff_generated_image_mime_type(&bytes)
        .or_else(|| canonical_sidecar_image_mime_type(sidecar_mime_type))
        .or_else(|| generated_image_mime_type(path))
        .map(str::to_string)
}

fn sniff_generated_image_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}
```

- [ ] **Step 4: Verify core tests pass**

Run:

```bash
cargo test -p puffer-core generated_media_preview -- --nocapture
```

Expected: all generated media preview tests pass.

- [ ] **Step 5: Commit core helper work**

Run:

```bash
git add crates/puffer-core/media_runtime.rs
git commit -m "feat(core): read generated media previews by artifact"
```

## Task 2: CLI Timeline Generated Attachments

**Files:**
- Modify: `crates/puffer-cli/src/desktop_api_types.rs`
- Modify: `crates/puffer-cli/src/desktop_api.rs`

- [ ] **Step 1: Add failing CLI timeline tests**

Add tests in `crates/puffer-cli/src/desktop_api.rs` under the existing test module:

```rust
#[test]
fn timeline_attaches_generated_image_to_following_assistant_message() {
    let (temp, store) = test_store();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    write_generated_image_artifact(&workspace, "artifact-1", "image.jpeg", b"\xff\xd8\xff\xd9");
    let session = record_with_cwd(
        workspace,
        vec![
            TranscriptEvent::ToolInvocation {
                call_id: "call-img".to_string(),
                tool_id: "ImageGeneration".to_string(),
                input: "{}".to_string(),
                output: serde_json::json!({
                    "artifactId": "artifact-1",
                    "path": "/unused/by/preview.jpeg",
                    "status": "succeeded"
                })
                .to_string(),
                success: true,
                actor: None,
                subject: None,
                metadata: None,
            },
            TranscriptEvent::AssistantMessage {
                text: "Done".to_string(),
                actor: None,
            },
        ],
    );

    let items = timeline_items(&store, &session);

    let Some(TimelineItemDto::AssistantMessage { attachments, .. }) =
        items.iter().find(|item| matches!(item, TimelineItemDto::AssistantMessage { .. }))
    else {
        panic!("assistant message exists");
    };
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].id, "generated-image:artifact-1");
    assert!(matches!(
        attachments[0].source,
        ChatAttachmentSourceDto::GeneratedMedia { ref artifact_id } if artifact_id == "artifact-1"
    ));
}

#[test]
fn timeline_flushes_generated_image_without_following_assistant() {
    let (temp, store) = test_store();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    write_generated_image_artifact(&workspace, "artifact-1", "image.jpeg", b"\xff\xd8\xff\xd9");
    let session = record_with_cwd(
        workspace,
        vec![TranscriptEvent::ToolInvocation {
            call_id: "call-img".to_string(),
            tool_id: "ImageGeneration".to_string(),
            input: "{}".to_string(),
            output: serde_json::json!({ "artifactId": "artifact-1", "status": "succeeded" })
                .to_string(),
            success: true,
            actor: None,
            subject: None,
            metadata: None,
        }],
    );

    let items = timeline_items(&store, &session);

    assert!(items.iter().any(|item| matches!(
        item,
        TimelineItemDto::AssistantMessage { text, attachments, .. }
            if text.is_empty() && attachments.len() == 1
    )));
}
```

Add test helpers in the same module:

```rust
fn record_with_cwd(cwd: PathBuf, events: Vec<TranscriptEvent>) -> SessionRecord {
    let mut record = record(events);
    record.metadata.cwd = cwd;
    record
}

fn write_generated_image_artifact(workspace: &Path, artifact_id: &str, filename: &str, bytes: &[u8]) {
    let image_dir = workspace.join(".puffer/media/images").join(artifact_id);
    std::fs::create_dir_all(&image_dir).unwrap();
    let image_path = image_dir.join(filename);
    std::fs::write(&image_path, bytes).unwrap();
    let sidecar_dir = workspace.join(".puffer/media/artifact-sidecars");
    std::fs::create_dir_all(&sidecar_dir).unwrap();
    std::fs::write(
        sidecar_dir.join(format!("{artifact_id}.json")),
        serde_json::to_string_pretty(&serde_json::json!({
            "id": artifact_id,
            "jobId": "job-1",
            "kind": "image",
            "path": image_path,
            "mimeType": "image/jpeg",
            "byteCount": bytes.len(),
            "metadata": {},
            "createdAtMs": 1
        }))
        .unwrap(),
    )
    .unwrap();
}
```

- [ ] **Step 2: Run the failing CLI tests**

Run:

```bash
cargo test -p puffer-cli timeline_attaches_generated_image -- --nocapture
cargo test -p puffer-cli timeline_flushes_generated_image -- --nocapture
```

Expected: fail because assistant messages have no attachments and no source exists.

- [ ] **Step 3: Add DTO source and assistant attachments**

In `crates/puffer-cli/src/desktop_api_types.rs`, add:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ChatAttachmentSourceDto {
    UserUpload,
    GeneratedMedia {
        #[serde(rename = "artifactId")]
        artifact_id: String,
    },
}
```

Add a `source` field to `ChatAttachmentDto`:

```rust
pub(crate) source: ChatAttachmentSourceDto,
```

Set uploaded attachments to `UserUpload` in `ChatAttachmentDto::from_stored`:

```rust
source: ChatAttachmentSourceDto::UserUpload,
```

Add attachments to assistant messages:

```rust
AssistantMessage {
    id: String,
    text: String,
    attachments: Vec<ChatAttachmentDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<MessageActor>,
},
```

- [ ] **Step 4: Implement linear generated-attachment synthesis**

In `crates/puffer-cli/src/desktop_api.rs`, import the core metadata helper:

```rust
use puffer_core::generated_media_attachment_metadata;
```

Add helper functions near `attachment_dtos`:

```rust
fn generated_image_attachment(cwd: &Path, output: &str) -> Option<ChatAttachmentDto> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    let artifact_id = value.get("artifactId")?.as_str()?.trim();
    if artifact_id.is_empty() {
        return None;
    }
    let metadata = generated_media_attachment_metadata(cwd, artifact_id)?;
    Some(ChatAttachmentDto {
        id: format!("generated-image:{artifact_id}"),
        name: "Generated image".to_string(),
        mime_type: metadata.mime_type,
        size: metadata.byte_count,
        extension: generated_image_extension(&metadata.mime_type).to_string(),
        kind: "image".to_string(),
        state: metadata.state,
        source: ChatAttachmentSourceDto::GeneratedMedia {
            artifact_id: artifact_id.to_string(),
        },
    })
}

fn generated_image_extension(mime_type: &str) -> &'static str {
    match mime_type {
        "image/png" => "PNG",
        "image/jpeg" => "JPEG",
        "image/webp" => "WEBP",
        _ => "IMAGE",
    }
}
```

Update `timeline_items` to keep a buffer:

```rust
let mut pending_generated_attachments: Vec<ChatAttachmentDto> = Vec::new();
```

For `ImageGeneration` tool calls, push generated metadata after pushing the tool card:

```rust
if *success && tool_id == "ImageGeneration" {
    if let Some(attachment) = generated_image_attachment(&record.metadata.cwd, output) {
        pending_generated_attachments.push(attachment);
    }
}
```

When handling `AssistantMessage`, attach and clear the buffer:

```rust
let attachments = std::mem::take(&mut pending_generated_attachments);
pending_assistant = Some(TimelineItemDto::AssistantMessage {
    id: format!("timeline-{index}"),
    text: text.clone(),
    attachments,
    actor: actor.clone(),
});
```

Before user/system/command/diff/session-renamed/rewrite events that break the assistant response, flush the buffer:

```rust
flush_pending_generated_attachments(
    &mut items,
    &mut pending_generated_attachments,
    index,
);
```

Define the flush helper:

```rust
fn flush_pending_generated_attachments(
    items: &mut Vec<TimelineItemDto>,
    pending: &mut Vec<ChatAttachmentDto>,
    index: usize,
) {
    if pending.is_empty() {
        return;
    }
    items.push(TimelineItemDto::AssistantMessage {
        id: format!("timeline-{index}-generated-media"),
        text: String::new(),
        attachments: std::mem::take(pending),
        actor: None,
    });
}
```

Also call the flush helper after the loop before the final `flush_pending_assistant`.

- [ ] **Step 5: Run CLI timeline tests**

Run:

```bash
cargo test -p puffer-cli timeline_attaches_generated_image -- --nocapture
cargo test -p puffer-cli timeline_flushes_generated_image -- --nocapture
cargo test -p puffer-cli timeline_user_message_includes_attachment_state -- --nocapture
```

Expected: all pass.

- [ ] **Step 6: Commit CLI timeline work**

Run:

```bash
git add crates/puffer-cli/src/desktop_api_types.rs crates/puffer-cli/src/desktop_api.rs
git commit -m "feat(cli): surface generated media as assistant attachments"
```

## Task 3: CLI Session-Aware Generated Preview RPC

**Files:**
- Modify: `crates/puffer-cli/src/daemon.rs`

- [ ] **Step 1: Add failing daemon preview test**

Add a daemon unit test near existing generated preview tests:

```rust
#[test]
fn read_generated_media_preview_resolves_session_cwd() {
    let temp = tempfile::tempdir().unwrap();
    let paths = ConfigPaths::discover(temp.path());
    let session_store = SessionStore::from_paths(&paths).unwrap();
    let workspace = temp.path().join("other-workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let session = session_store.create_session(workspace.clone()).unwrap();
    write_generated_image_artifact(&workspace, "artifact-1", "image.jpeg", b"\xff\xd8\xff\xd9");
    let state = test_state_with_paths(paths);

    let response = handle_read_generated_media_preview(
        &state,
        &serde_json::json!({
            "sessionId": session.id.to_string(),
            "artifactId": "artifact-1"
        }),
    )
    .unwrap();

    assert_eq!(response["state"], "available");
    assert_eq!(response["mimeType"], "image/jpeg");
}
```

Use the same `write_generated_image_artifact` pattern from Task 2, or add a local helper if the daemon tests already have isolated helpers.

- [ ] **Step 2: Run the failing daemon test**

Run:

```bash
cargo test -p puffer-cli read_generated_media_preview_resolves_session_cwd -- --nocapture
```

Expected: fail because the handler still expects `path`.

- [ ] **Step 3: Change params and handler**

Update params:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedMediaPreviewParams {
    session_id: String,
    artifact_id: String,
}
```

Update handler:

```rust
fn handle_read_generated_media_preview(state: &DaemonState, params: &Value) -> Result<Value> {
    let input: GeneratedMediaPreviewParams =
        serde_json::from_value(params.clone()).context("invalid generated media preview params")?;
    let session_store = SessionStore::from_paths(&state.paths)?;
    let cwd = desktop_api::load_session_cwd(&session_store, &input.session_id)?;
    let result = puffer_core::read_generated_media_preview_by_artifact(&cwd, &input.artifact_id);
    Ok(serde_json::to_value(result)?)
}
```

- [ ] **Step 4: Update old path-based daemon tests**

For existing tests that call `read_generated_media_preview` with `path`, either:

- convert them to use a created session and `artifactId`, or
- move path-only validation coverage to `puffer-core` tests from Task 1.

Do not keep public daemon tests that assert path-based preview request behavior.

- [ ] **Step 5: Run daemon tests**

Run:

```bash
cargo test -p puffer-cli read_generated_media_preview -- --nocapture
```

Expected: all daemon generated preview tests pass.

- [ ] **Step 6: Commit daemon RPC work**

Run:

```bash
git add crates/puffer-cli/src/daemon.rs
git commit -m "feat(cli): read generated previews by session artifact"
```

## Task 4: Tauri Backend Mirror

**Files:**
- Modify: `apps/puffer-desktop/src-tauri/src/dtos.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/session_data.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/backend.rs`

- [ ] **Step 1: Add failing Tauri DTO/session tests**

Add tests mirroring the CLI tests in the closest existing Tauri test module:

```rust
#[test]
fn tauri_timeline_attaches_generated_image_to_assistant_message() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    write_generated_image_artifact(&workspace, "artifact-1", "image.jpeg", b"\xff\xd8\xff\xd9");
    let record = session_record_with_events(workspace, vec![
        TranscriptEvent::ToolInvocation {
            call_id: "call-img".to_string(),
            tool_id: "ImageGeneration".to_string(),
            input: "{}".to_string(),
            output: serde_json::json!({ "artifactId": "artifact-1", "status": "succeeded" })
                .to_string(),
            success: true,
            actor: None,
            subject: None,
            metadata: None,
        },
        TranscriptEvent::AssistantMessage {
            text: "Done".to_string(),
            actor: None,
        },
    ]);

    let items = timeline_items(&SessionStore::from_paths(&ConfigPaths::discover(temp.path())).unwrap(), &record);

    assert!(items.iter().any(|item| matches!(
        item,
        TimelineItemDto::AssistantMessage { attachments, .. } if attachments.len() == 1
    )));
}
```

Use local helper names that fit the Tauri module. The assertion is the important behavior.

- [ ] **Step 2: Run the failing Tauri test**

Run:

```bash
cargo test -p corbina tauri_timeline_attaches_generated_image -- --nocapture
```

Expected: fail until DTO and timeline synthesis are mirrored.

- [ ] **Step 3: Mirror DTO source and assistant attachments**

Apply the same `ChatAttachmentSourceDto`, `ChatAttachmentDto.source`, and `AssistantMessage.attachments` changes from Task 2 to:

```text
apps/puffer-desktop/src-tauri/src/dtos.rs
```

- [ ] **Step 4: Mirror timeline synthesis**

Apply the same linear `pending_generated_attachments` buffer and helper behavior from Task 2 to:

```text
apps/puffer-desktop/src-tauri/src/session_data.rs
```

Keep the implementation local to session timeline loading. Do not introduce shared cross-crate abstractions unless the duplicate grows beyond this narrow synthesis logic.

- [ ] **Step 5: Mirror generated preview RPC**

In `apps/puffer-desktop/src-tauri/src/backend.rs`, update `GeneratedMediaPreviewParams`:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedMediaPreviewParams {
    session_id: String,
    artifact_id: String,
}
```

Update the handler:

```rust
fn read_generated_media_preview(&self, params: Value) -> Result<GeneratedMediaPreviewResult> {
    let input: GeneratedMediaPreviewParams =
        serde_json::from_value(params).context("invalid generated media preview params")?;
    let cwd = crate::session_data::load_session_cwd(&input.session_id)?;
    Ok(read_generated_media_preview_by_artifact(cwd, &input.artifact_id))
}
```

- [ ] **Step 6: Run Tauri tests**

Run:

```bash
cargo test -p corbina read_generated_media_preview -- --nocapture
cargo test -p corbina generated_image -- --nocapture
```

Expected: relevant Tauri backend and timeline tests pass.

- [ ] **Step 7: Commit Tauri mirror work**

Run:

```bash
git add apps/puffer-desktop/src-tauri/src/dtos.rs apps/puffer-desktop/src-tauri/src/session_data.rs apps/puffer-desktop/src-tauri/src/backend.rs
git commit -m "feat(desktop): mirror generated media attachments"
```

## Task 5: Frontend Types and Preview Loading

**Files:**
- Modify: `apps/puffer-desktop/src/lib/types.ts`
- Modify: `apps/puffer-desktop/src/lib/api/desktop.ts`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/MessageAttachmentPreviewStrip.svelte`

- [ ] **Step 1: Add or update type tests**

Update `apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts` with:

```ts
import type { MessageAttachment } from "../types";

const generatedAttachment: MessageAttachment = {
  id: "generated-image:artifact-1",
  name: "Generated image",
  mimeType: "image/jpeg",
  size: 4,
  extension: "JPEG",
  kind: "image",
  state: "available",
  source: { kind: "generated_media", artifactId: "artifact-1" }
};

expect(generatedAttachment.source.kind).toBe("generated_media");
```

- [ ] **Step 2: Run the failing type test**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/api/desktop.attachment-types.test.ts
```

Expected: fail because `MessageAttachment.source` does not exist.

- [ ] **Step 3: Add source types**

In `apps/puffer-desktop/src/lib/types.ts`, add:

```ts
export type AttachmentPreviewSource =
  | { kind: "user_upload" }
  | { kind: "generated_media"; artifactId: string };
```

Update `MessageAttachment`:

```ts
export type MessageAttachment = AgentTurnAttachment & {
  state?: AttachmentState;
  source: AttachmentPreviewSource;
  file?: File;
  previewUrl?: string | null;
};
```

- [ ] **Step 4: Normalize backend source and assistant attachments**

In `apps/puffer-desktop/src/lib/api/desktop.ts`, update `BackendChatAttachment`:

```ts
type BackendChatAttachmentSource =
  | { kind: "user_upload" }
  | { kind: "generated_media"; artifactId: string };

type BackendChatAttachment = {
  id: string;
  name: string;
  mimeType: string;
  size: number;
  extension: string;
  kind: "image" | "file";
  state?: AttachmentState;
  source: BackendChatAttachmentSource;
};
```

Update `BackendTimelineItem` assistant message branch:

```ts
| ({
    kind: "assistant_message";
    id: string;
    text: string;
    createdAtMs?: number | null;
    attachments?: BackendChatAttachment[];
  } & BackendActorFields)
```

Update normalization:

```ts
function normalizeMessageAttachment(value: BackendChatAttachment): MessageAttachment {
  return {
    id: value.id,
    name: value.name,
    mimeType: value.mimeType,
    size: value.size,
    extension: value.extension,
    kind: value.kind,
    state: value.state,
    source: value.source
  };
}
```

Update the assistant message case:

```ts
case "assistant_message": {
  const attachments = (value.attachments ?? []).map(normalizeMessageAttachment);
  return {
    id: value.id,
    kind: "assistant",
    createdAtMs: value.createdAtMs ?? null,
    title: "Assistant response",
    summary: preview(value.text),
    body: value.text,
    meta: [],
    ...(attachments.length > 0 ? { attachments } : {}),
    actor: value.actor ?? null
  };
}
```

- [ ] **Step 5: Add generated preview API helper**

In `apps/puffer-desktop/src/lib/api/desktop.ts`, replace the path-based helper with:

```ts
export async function readGeneratedMediaPreview(
  sessionId: string,
  artifactId: string
): Promise<AttachmentPreviewResult> {
  const client = await ensureLocalDaemonClient();
  return client.request<AttachmentPreviewResult>("read_generated_media_preview", {
    sessionId,
    artifactId
  });
}
```

Add a source-aware helper:

```ts
export async function readMessageAttachmentPreview(
  sessionId: string,
  attachment: MessageAttachment
): Promise<AttachmentPreviewResult> {
  if (attachment.source.kind === "generated_media") {
    return readGeneratedMediaPreview(sessionId, attachment.source.artifactId);
  }
  return readChatAttachmentPreview(sessionId, attachment.id);
}
```

- [ ] **Step 6: Use the source-aware helper in preview strip**

In `MessageAttachmentPreviewStrip.svelte`, change the import:

```ts
import { readMessageAttachmentPreview } from "../../api/desktop";
```

Update `loadPreview`:

```ts
const preview = await readMessageAttachmentPreview(targetSessionId, attachment);
```

- [ ] **Step 7: Run frontend unit/type tests**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/api/desktop.attachment-types.test.ts
```

Expected: pass.

- [ ] **Step 8: Commit frontend type work**

Run:

```bash
git add apps/puffer-desktop/src/lib/types.ts apps/puffer-desktop/src/lib/api/desktop.ts apps/puffer-desktop/src/lib/screens/agent/MessageAttachmentPreviewStrip.svelte apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts
git commit -m "feat(desktop): read previews from attachment sources"
```

## Task 6: Desktop UI Tests and Fake Daemon

**Files:**
- Modify: `apps/puffer-desktop/tests/support/fakeDaemon.ts`
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`

- [ ] **Step 1: Add failing persisted generated preview UI test**

Add a Playwright test:

```ts
test("persisted ImageGeneration results render as assistant image attachments", async ({ page }) => {
  const daemon = new FakeDaemon({
    sessions: [
      {
        sessionId: "session-generated-media",
        displayName: "Generated media",
        title: "Generated media",
        cwd: "/tmp/puffer",
        folderPath: "/tmp/puffer",
        updatedAtMs: baseTime,
        createdAtMs: baseTime - 60_000,
        eventCount: 2,
        timeline: [
          {
            kind: "tool_call",
            id: "tool-image",
            toolId: "ImageGeneration",
            status: "ok",
            inputText: JSON.stringify({ prompt: "draw an icon" }),
            outputText: JSON.stringify({
              artifactId: "artifact-generated-1",
              status: "succeeded"
            }),
            createdAtMs: baseTime - 20_000
          },
          {
            kind: "assistant_message",
            id: "assistant-image",
            text: "Done",
            createdAtMs: baseTime - 10_000,
            attachments: [
              {
                id: "generated-image:artifact-generated-1",
                name: "Generated image",
                mimeType: "image/png",
                size: onePixelPngBytes.length,
                extension: "PNG",
                kind: "image",
                state: "available",
                source: { kind: "generated_media", artifactId: "artifact-generated-1" }
              }
            ]
          }
        ]
      }
    ]
  });
  daemon.seedGeneratedMediaPreview("session-generated-media", "artifact-generated-1", {
    state: "available",
    mimeType: "image/png",
    bytes: onePixelPngBytes
  });
  await daemon.install(page);
  await daemon.open(page);

  await openSession(page, /Generated media/);

  const thumbnail = page.getByRole("button", { name: "Open image attachment Generated image" });
  await expect(thumbnail).toBeVisible();
  await expect(thumbnail.getByAltText("Generated image")).toBeVisible();
  await expect(page.getByText("/tmp/puffer/.puffer/media/images")).toHaveCount(0);
  await thumbnail.click();
  await expect(page.getByTestId("attachment-overlay")).toBeVisible();
});
```

- [ ] **Step 2: Run the failing UI test**

Run:

```bash
cd apps/puffer-desktop
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "persisted ImageGeneration results render"
```

Expected: fail until fake daemon and frontend preview helper support generated preview source.

- [ ] **Step 3: Update fake daemon generated preview seeding**

In `apps/puffer-desktop/tests/support/fakeDaemon.ts`, change generated preview storage from path key to `sessionId + artifactId` key:

```ts
private generatedMediaPreviewKey(sessionId: string, artifactId: string): string {
  return `${sessionId}\u0000${artifactId}`;
}

seedGeneratedMediaPreview(
  sessionId: string,
  artifactId: string,
  preview: AttachmentPreviewFixture
): void {
  this.generatedMediaPreviews.set(this.generatedMediaPreviewKey(sessionId, artifactId), preview);
}
```

Update request handling:

```ts
private readGeneratedMediaPreview(params: JsonRecord): AttachmentPreviewFixture {
  const sessionId = String(params.sessionId ?? "");
  const artifactId = String(params.artifactId ?? "");
  return this.generatedMediaPreviews.get(this.generatedMediaPreviewKey(sessionId, artifactId)) ?? {
    state: "missing"
  };
}
```

- [ ] **Step 4: Update old `/image` slash tests**

For existing tests that assert:

```ts
request.params.path === generatedPath
```

replace with:

```ts
request.params.sessionId === "session-image-preview" &&
request.params.artifactId === "artifact-preview-success"
```

Seed generated previews with:

```ts
daemon.seedGeneratedMediaPreview("session-image-preview", "artifact-preview-success", {
  state: "available",
  mimeType: "image/png",
  bytes: onePixelPngBytes
});
```

- [ ] **Step 5: Run generated media UI tests**

Run:

```bash
cd apps/puffer-desktop
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "image|generated"
```

Expected: generated media and existing image attachment tests pass.

- [ ] **Step 6: Commit UI tests**

Run:

```bash
git add apps/puffer-desktop/tests/support/fakeDaemon.ts apps/puffer-desktop/tests/chat-session-ui.spec.ts
git commit -m "test(desktop): cover generated media attachments"
```

## Task 7: End-to-End Verification

**Files:**
- No source edits unless a verification failure identifies a focused fix.

- [ ] **Step 1: Run Rust tests for touched crates**

Run:

```bash
cargo test -p puffer-core generated_media_preview -- --nocapture
cargo test -p puffer-cli generated_media -- --nocapture
cargo test -p corbina generated_media -- --nocapture
```

Expected: all pass.

- [ ] **Step 2: Run focused desktop tests**

Run:

```bash
cd apps/puffer-desktop
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "image|generated"
```

Expected: all focused generated image and attachment preview tests pass.

- [ ] **Step 3: Run broad workspace check if time permits**

Run:

```bash
cargo test --workspace
```

Expected: workspace tests pass. If unrelated pre-existing failures appear, capture the failure names and keep the generated-media commits intact.

- [ ] **Step 4: Manual Milhous verification**

Open the Milhous session in desktop UI and verify:

- generated images display as assistant thumbnails;
- clicking a thumbnail opens the attachment overlay;
- local absolute image paths are not primary chat UI;
- the `ImageGeneration` tool card still appears in agent activity;
- no generated preview request uses only a path.

- [ ] **Step 5: Final review**

Run:

```bash
git log --oneline -6
git status --short
```

Expected:

- generated-media work is split into small commits;
- no unrelated dirty files were staged or committed;
- remaining dirty files are either pre-existing or explicitly part of a follow-up.
