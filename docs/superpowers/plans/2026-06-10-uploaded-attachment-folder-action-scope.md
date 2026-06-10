# Uploaded Attachment Folder Action Scope Implementation Plan

> **For agentic workers:** implement task-by-task. Keep this as a focused
> frontend action-policy change; do not migrate upload paths or attachment
> metadata.

**Goal:** Uploaded/staged chat attachments no longer show the overlay folder
action. Generated images and generated videos keep the folder action when their
`generated_media.localPath` exists. Remote images keep the download action.

**Spec:** `docs/superpowers/specs/2026-06-10-uploaded-attachment-folder-action-scope-design.md`

**Scope:** One pure action resolver plus focused frontend and Playwright tests.
No DTO changes, no session schema changes, no Tauri command changes, no upload
picker or drag/drop rewrite.

---

## File Touches

- `apps/puffer-desktop/src/lib/screens/agent/attachmentOverlayAction.test.ts`
  - change uploaded/local `local_file` expectations from `open_folder` to
    `null`;
  - keep generated-media and remote-image cases.
- `apps/puffer-desktop/src/lib/screens/agent/attachmentOverlayAction.ts`
  - return `null` for `source.kind === "local_file"`;
  - keep `generated_media.localPath -> open_folder`;
  - keep `remote_url` image -> download.
- `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - update the local-image overlay test to assert no folder action for uploaded
    local files;
  - add or adjust a generated-media overlay fixture to assert the folder action
    still appears for generated image/video artifacts.

Do not touch:

- `apps/puffer-desktop/src/lib/types.ts`
- `apps/puffer-desktop/src/lib/api/desktop.ts`
- `apps/puffer-desktop/src-tauri/src/image_actions.rs`
- `apps/puffer-desktop/src-tauri/src/lib.rs`
- `crates/puffer-session-store`
- `crates/puffer-cli/src/desktop_api*`

---

## Task 1: Update Resolver Unit Tests First

Run from repo root unless noted.

- [ ] In `attachmentOverlayAction.test.ts`, change
  `"returns open folder for local image files"` to expect `null`.
- [ ] Change `"returns open folder for local non-image files"` to expect
  `null`.
- [ ] Keep these positive cases unchanged:
  - remote URL image returns `download`;
  - generated image with `localPath` returns `open_folder`;
  - generated video with `localPath` returns `open_folder`.
- [ ] Keep negative cases for generated media without `localPath` and remote
  non-image files.
- [ ] Run the focused unit test and confirm it fails before implementation:

```bash
npm --prefix apps/puffer-desktop exec vitest -- run src/lib/screens/agent/attachmentOverlayAction.test.ts
```

Expected failure: the current resolver still returns `open_folder` for
`local_file`.

## Task 2: Change The Resolver

- [ ] In `attachmentOverlayAction.ts`, change the `local_file` branch to
  `return null;`.
- [ ] Do not change the generated-media or remote-url branches.
- [ ] Run the focused unit test again:

```bash
npm --prefix apps/puffer-desktop exec vitest -- run src/lib/screens/agent/attachmentOverlayAction.test.ts
```

Expected result: pass.

## Task 3: Update UI Coverage

- [ ] In `chat-session-ui.spec.ts`, rename/reword the existing local-image
  overlay test so it describes an uploaded/staged local source, not a generic
  local image.
- [ ] Assert the uploaded local image overlay renders metadata/preview and close
  but no folder action. A robust assertion is that the action group has only the
  close button, or that no button named `Open containing folder` is present.
- [ ] Add or reuse a generated-media fixture with
  `source.kind === "generated_media"` and `localPath`.
- [ ] Assert the generated-media overlay still shows the folder action left of
  close.
- [ ] Preserve the remote URL image download-action test.

Implementation guard: do not click the folder action in Playwright; this should
not open the OS file manager during tests.

## Task 4: Run Focused Verification

- [ ] Run the action resolver unit test:

```bash
npm --prefix apps/puffer-desktop exec vitest -- run src/lib/screens/agent/attachmentOverlayAction.test.ts
```

- [ ] Run the focused UI tests:

```bash
npm --prefix apps/puffer-desktop run test:desktop -- tests/chat-session-ui.spec.ts -g "attachment overlay"
```

- [ ] Run Svelte/type diagnostics:

```bash
npm --prefix apps/puffer-desktop run check
```

No Rust test is required for this change because the native folder command and
DTO serialization remain unchanged.

## Task 5: Final Review

- [ ] Confirm `git diff` does not include upload picker, drag/drop, DTO,
  session-store, or Tauri command changes.
- [ ] Confirm uploaded local attachments still open previews normally.
- [ ] Confirm generated image/video attachments still expose folder action when
  `localPath` exists.
- [ ] Confirm remote image attachments still expose download action.
