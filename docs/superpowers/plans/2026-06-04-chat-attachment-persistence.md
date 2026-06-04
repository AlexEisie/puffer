# Chat Attachment Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refreshing desktop chat preserves uploaded attachment cards. Available image attachments can still open a preview, and missing files still render metadata and open the existing unavailable detail state.

**Reviewed Architecture:** Keep the existing HTML5 file picker/drop and chat open-intent UI. Add only the durable boundary that is missing: session-store sidecars plus structured `StoredAttachment` records on `TranscriptEvent::UserMessage`. Production file bytes cross only the local Tauri IPC staging command; daemon `run_agent_turn` receives staged attachment IDs, not bytes or client-supplied metadata.

**Updated Specs:**
- `docs/superpowers/specs/2026-06-04-chat-attachment-persistence-design.md`
- `specs/puffer-session-store/07.md`
- `specs/puffer-cli/158.md`
- `specs/puffer-desktop/666.md`

---

## Plan Review Findings

- Existing desktop attachment UI already covers draft cards, message cards, open intents, and unavailable detail. Do not rebuild that surface.
- Existing `run_agent_turn.attachments` is transient metadata. Replace it for production submit with `attachmentIds`; do not keep two durable payload contracts.
- Old transcript events without `attachments` must still deserialize. Add serde defaulting so historical sessions load.
- Do not parse `[Image: name]` or `[File: name]` text to synthesize cards. Those lines remain provider prompt fallback only.
- Validate staged IDs server-side. Attachment IDs are generated UUIDs; reject non-UUID/path-like IDs and verify loaded metadata matches the requested ID.
- Cancellation can persist the user prompt through `report_cancelled_turn` before the worker writes it. That fallback must also preserve staged attachments.
- Deleting a session must remove `<session-id>.attachments/`; the current file sweep does not remove directories.
- Preview state is computed, not persisted. The preview read path must re-check file existence on click.
- Avoid chunking, dedupe, download/open-with, rich document preview, cloud sync, and garbage collection in this change.

---

## Task 1: Session-Store Attachment Model

**Files**
- Create: `crates/puffer-session-store/src/attachments.rs`
- Modify: `crates/puffer-session-store/src/events.rs`
- Modify: `crates/puffer-session-store/src/lib.rs`
- Modify: `crates/puffer-session-store/src/store.rs`
- Spec reference: `specs/puffer-session-store/07.md`

- [x] Add failing tests in `events.rs`:
  - `UserMessage` serializes `attachments`.
  - A legacy `{"type":"user_message","text":"hi"}` event deserializes with `attachments: []`.
- [x] Add `StoredAttachmentKind` and `StoredAttachment` above `TranscriptEvent`.
- [x] Change `TranscriptEvent::UserMessage` to include:

```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
attachments: Vec<StoredAttachment>,
```

- [x] Update all workspace `TranscriptEvent::UserMessage` construction and matches.
- [x] Add `attachments.rs` with:
  - `StageAttachmentInput`
  - `AttachmentState`
  - `AttachmentPreviewBytes`
  - `SessionStore::stage_attachment`
  - `SessionStore::load_staged_attachments`
  - `SessionStore::attachment_state`
  - `SessionStore::read_attachment_preview`
- [x] Store files under:

```text
<config>/sessions/<session-id>.attachments/<attachment-id>/
  original
  metadata.json
```

- [x] Generate attachment IDs with `Uuid::new_v4()`.
- [x] Write `original.tmp`, rename to `original`, write `metadata.json.tmp`, then rename to `metadata.json`.
- [x] Sanitize display names by removing control characters and collapsing whitespace; fallback to `attachment`.
- [x] Reject `load_staged_attachments` IDs that are not UUID strings, and reject metadata whose `id` does not match the requested ID.
- [x] Compute paths from `session_id` + generated `id`, not from untrusted `storage_key`.
- [x] Update `SessionStore::delete_session` to remove the matching attachment directory with `remove_dir_all`.
- [x] Re-export only the types needed by CLI/Tauri.
- [x] Run:

```bash
cargo test -p puffer-session-store attachment -- --nocapture
cargo test -p puffer-session-store user_message -- --nocapture
```

Expected: all pass.

---

## Task 2: CLI Timeline And Turn Persistence

**Files**
- Modify: `crates/puffer-cli/src/desktop_api_types.rs`
- Modify: `crates/puffer-cli/src/desktop_api.rs`
- Modify: `crates/puffer-cli/src/daemon.rs`
- Spec reference: `specs/puffer-cli/158.md`

- [x] Add `ChatAttachmentDto` with `id`, `name`, `mimeType`, `size`, `extension`, `kind`, and `state`.
- [x] Add `attachments: Vec<ChatAttachmentDto>` to `TimelineItemDto::UserMessage`.
- [x] Change `load_session_detail`/`timeline_items` so user-message attachment DTOs are mapped with `SessionStore::attachment_state`.
- [x] Add a focused desktop API test that stages one image, appends a user message with the stored attachment, loads session detail, and asserts `state == "available"`.
- [x] Parse `attachmentIds`/`attachment_ids` in `run_agent_turn`; require an array of strings.
- [x] Do not persist client-supplied `attachments` metadata as durable attachments. Production submit must use staged IDs.
- [x] Load staged attachments before provider execution and before appending `UserMessage`.
- [x] If loading any staged ID fails, publish the existing turn-error envelope, remove the turn handle, and do not persist the user message.
- [x] Store the loaded `Vec<StoredAttachment>` on `TurnHandle` so `report_cancelled_turn` can write the same attachments if cancellation wins the race.
- [x] Slash-command/sessionless turn paths use empty attachments.
- [x] Add daemon smoke coverage for `run_agent_turn` with one staged attachment and timeline reload.
- [x] Run:

```bash
cargo test -p puffer-cli timeline_user_message_includes_attachment_state -- --nocapture
cargo test -p puffer-cli --test daemon_turn_smoke attachment -- --nocapture
```

Expected: all pass.

---

## Task 3: Tauri Staging And Preview Commands

**Files**
- Create: `apps/puffer-desktop/src-tauri/src/chat_attachments.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/lib.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/dtos.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/dto.rs`

- [x] Add command registration expectations for:
  - `stage_chat_attachment`
  - `read_chat_attachment_preview`
- [x] Implement `stage_chat_attachment(sessionId, name, mimeType, extension, kind, bytes)`.
- [x] Enforce the existing 20 MiB per-file limit in Rust, even though the frontend checks first.
- [x] Convert `kind` to `StoredAttachmentKind`; reject unsupported values.
- [x] Return a timeline-compatible attachment DTO with computed `state`.
- [x] Implement `read_chat_attachment_preview(sessionId, attachmentId)`:
  - return `missing` when metadata or backing file is absent;
  - return `unsupported` for non-image attachments;
  - return `{ state: "available", mimeType, bytes }` only for available images.
- [x] Register both Tauri commands in `generate_handler!` and `REGISTERED_TAURI_COMMANDS`.
- [x] Change Tauri `run_agent_turn` from `attachments: Option<Vec<ChatAttachmentInput>>` to `attachment_ids: Option<Vec<String>>`, and forward `attachmentIds` to the backend payload.
- [x] Remove the old `ChatAttachmentInput`/`ChatAttachmentKind` types from `src-tauri/src/lib.rs` if they are no longer used.
- [x] Run:

```bash
cargo test -p corbina registered_tauri_commands -- --nocapture
```

Expected: pass.

---

## Task 4: Frontend API And Types

**Files**
- Modify: `apps/puffer-desktop/src/lib/types.ts`
- Modify: `apps/puffer-desktop/src/lib/api/desktop.ts`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/attachments.ts`
- Modify: `apps/puffer-desktop/src/lib/agentTurnAttachments.ts`

- [x] Add:

```ts
type AttachmentState = "available" | "missing";
type AttachmentPreviewResult =
  | { state: "available"; mimeType: string; bytes: number[] }
  | { state: "missing" }
  | { state: "unsupported" };
```

- [x] Extend `MessageAttachment` with optional `state`.
- [x] Keep `ComposerAttachmentDraft.file: File`; this is required for staging.
- [x] Make `messageAttachmentFromDraft` return the file plus existing object URL for optimistic rendering.
- [x] Add `stageChatAttachment(sessionId, attachment)`:
  - in dev/test, allow `window.__PUFFER_TEST_STAGE_CHAT_ATTACHMENT__`;
  - in production, require Tauri and call `stage_chat_attachment`;
  - send bytes via Tauri IPC only.
- [x] Add `readChatAttachmentPreview(sessionId, attachmentId)`:
  - prefer Tauri command when available;
  - allow fake-daemon fallback only for dev/test browser Playwright coverage;
  - do not add a production daemon file-byte preview endpoint.
- [x] Change `AgentTurnOptions.attachments` to `attachmentIds?: string[]`.
- [x] Keep `displayAttachments?: MessageAttachment[]` on submit options for optimistic UI only.
- [x] Update `formatAgentTurnMessage` call sites to accept staged refs and still produce prompt fallback lines.
- [x] Run:

```bash
npm --prefix apps/puffer-desktop run check
```

Expected: it may fail until Task 5 updates submit flow; do not commit this task alone.

---

## Task 5: Submit Flow And Preview Opening

**Files**
- Modify: `apps/puffer-desktop/src/App.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte`

- [x] In `ConversationView.submit`, pass draft-derived `displayAttachments` with file objects to `onSubmitMessage`; do not build daemon-facing metadata there.
- [x] In `App.svelte`, reject attachment submit for remote sessions with a clear status message.
- [x] Stage attachments before inserting the optimistic user row or calling `runAgentTurn`.
- [x] Build `stagedDisplayAttachments` by merging each durable staged ref with the draft `previewUrl` for immediate optimistic image preview.
- [x] Build the provider fallback `turnMessage` from staged refs.
- [x] Pass `attachmentIds: stagedRefs.map((attachment) => attachment.id)` to `runAgentTurn`.
- [x] Persist optimistic pending messages with existing preview URL stripping.
- [x] Ensure `visibleMessageBody` hides fallback attachment lines only when structured `attachments` are present.
- [x] Keep `AttachmentPreviewStrip` synchronous; it should emit an attachment open intent and not read storage.
- [x] In `AgentDetail.openChatIntent`, when opening an image attachment without `previewUrl`, call `readChatAttachmentPreview` using the selected session ID.
- [x] If preview read returns `available`, create a temporary object URL and pass it to the overlay.
- [x] If preview read returns `missing` or `unsupported`, open the existing unavailable detail state.
- [x] Revoke any object URL created by preview-read when the overlay closes or the selected session changes.
- [x] Run:

```bash
npm --prefix apps/puffer-desktop run check
```

Expected: pass.

---

## Task 6: Desktop Backend Timeline Parity

**Files**
- Modify: `apps/puffer-desktop/src-tauri/src/backend.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/session_data.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/session_api.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/dtos.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/dto.rs`

- [x] Add the same attachment DTO shape to both Tauri DTO modules, or factor a tiny shared helper only if the local module structure makes that cheaper than duplication.
- [x] Add `attachments` to both Tauri timeline `UserMessage` variants.
- [x] For session-store-backed timeline conversion, pass `SessionStore` into `timeline_items` and compute attachment state.
- [x] For in-memory `StoredEvent::User` fallback paths, return `attachments: Vec::new()`.
- [x] Run:

```bash
cargo test -p corbina
```

Expected: pass.

---

## Task 7: Playwright Coverage

**Files**
- Modify: `apps/puffer-desktop/tests/support/fakeDaemon.ts`
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`

- [x] Add a dev-only staging hook helper in tests before composer attachment submits.
- [x] Update existing attachment-submit assertions to expect `attachmentIds`, not full `attachments` metadata.
- [x] Extend `FakeDaemon` with `seedAttachmentPreview(sessionId, attachmentId, preview)` and a `read_chat_attachment_preview` dispatch case.
- [x] Add regression: persisted attachment cards survive browser refresh.
- [x] Add regression: fallback text such as `[Image: 1.jpg]` is hidden when structured attachments exist.
- [x] Add regression: available persisted image opens preview after refresh.
- [x] Add regression: missing persisted image opens unavailable detail and does not render an image.
- [x] Run:

```bash
npm --prefix apps/puffer-desktop run test:desktop-ui -- tests/chat-session-ui.spec.ts
```

Expected: pass.

---

## Task 8: Final Verification

- [x] Run Rust checks:

```bash
cargo test -p puffer-session-store
cargo test -p puffer-cli
cargo test -p corbina
```

- [x] Run frontend checks:

```bash
npm --prefix apps/puffer-desktop run check
npm --prefix apps/puffer-desktop run test:desktop-ui -- tests/chat-session-ui.spec.ts
```

- [x] Review changed files:

```bash
git status --short
git diff --stat
git diff --check
```

Expected: no whitespace errors and no unrelated file edits.

## Verification Notes

- `cargo test -p puffer-session-store`: passed.
- `cargo test -p puffer-cli --test daemon_turn_smoke`: passed.
- `cargo test -p puffer-cli`: run; all non-snapshot suites reached by the run passed, but 6 existing `tmux_snapshots` fixture comparisons still fail with normalized snapshot mismatches.
- `cargo test --manifest-path apps/puffer-desktop/src-tauri/Cargo.toml`: passed.
- `npm --prefix apps/puffer-desktop run check`: passed with existing `Workflows.svelte` unused-selector warnings.
- `npx playwright test tests/chat-session-ui.spec.ts`: passed 144/144. The npm `test:desktop-ui -- tests/chat-session-ui.spec.ts` wrapper was also attempted, but it expanded to the full desktop UI suite and hit unrelated non-attachment failures.
- `git diff --check`: passed.
