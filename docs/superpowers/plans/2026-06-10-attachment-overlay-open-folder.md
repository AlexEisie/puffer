# Attachment Overlay Open-Folder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let any chat attachment that exists on disk — image, video, or plain file — open its containing folder from the attachment overlay.

**Architecture:** One overlay (`AttachmentOverlay.svelte`) drives one open-folder path (decision function → `desktop.ts` bridge → Tauri `open_containing_folder` → `image_actions.rs`). The decision function currently rejects non-image/video kinds; we remove that gate and branch only on `source.kind`. Image-gen/video-gen results flow through the same `generated_media` branch and are preserved. All image-specific names are renamed to attachment-neutral names (no backward-compat required).

**Tech Stack:** Svelte 5 (runes) + TypeScript, Vitest, Tauri v2 (Rust, crate `corbina`), `tauri-plugin-opener`.

**Spec:** `docs/superpowers/specs/2026-06-10-attachment-overlay-open-folder-design.md`

---

## File Structure

- `apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.ts` → **rename** to `attachmentOverlayAction.ts` — pure decision logic (kind/source → action). Sole behavioral change lives here.
- `apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.test.ts` → **rename** to `attachmentOverlayAction.test.ts` — unit tests for the decision logic.
- `apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte` — **modify** — imports, action call, generic label.
- `apps/puffer-desktop/src/lib/api/desktop.ts` — **modify** — rename bridge fn + invoke string.
- `apps/puffer-desktop/src-tauri/src/image_actions.rs` — **modify** — rename command + resolver, generic error strings, add file test.
- `apps/puffer-desktop/src-tauri/src/lib.rs` — **modify** — allowlist string + handler registration.

**Untouched (do not rename):** `open_image_dir` (lib.rs:431) and `download_image_from_url` — unrelated commands.

**Task order rationale:** Rust command rename (Task 2) lands *before* the `desktop.ts` invoke-string flip (Task 3), so the app never references an unregistered command between commits.

---

## Task 1: Frontend decision logic — rename and extend to files

**Files:**
- Rename: `apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.ts` → `attachmentOverlayAction.ts`
- Rename + Test: `apps/puffer-desktop/src/lib/screens/agent/imageOverlayAction.test.ts` → `attachmentOverlayAction.test.ts`

Run all commands from `apps/puffer-desktop/`.

- [ ] **Step 1: Rename both files with git**

```bash
cd apps/puffer-desktop
git mv src/lib/screens/agent/imageOverlayAction.ts src/lib/screens/agent/attachmentOverlayAction.ts
git mv src/lib/screens/agent/imageOverlayAction.test.ts src/lib/screens/agent/attachmentOverlayAction.test.ts
```

- [ ] **Step 2: Replace the test file with the new behavior**

Write `src/lib/screens/agent/attachmentOverlayAction.test.ts` with this exact content (renames the import, **changes** the old "non-image returns null" case to expect open-folder for a local file, and **adds** a remote-only file case):

```ts
import { expect, test } from "vitest";
import type { MessageAttachment } from "../../types";
import { attachmentOverlayAction } from "./attachmentOverlayAction";

function attachment(overrides: Partial<MessageAttachment>): MessageAttachment {
  return {
    id: "attachment-1",
    name: "pixel.png",
    mimeType: "image/png",
    size: 12,
    extension: "PNG",
    kind: "image",
    state: "available",
    source: { kind: "local_file", path: "/tmp/puffer/pixel.png" },
    ...overrides
  };
}

test("returns open folder for local image files", () => {
  expect(attachmentOverlayAction(attachment({}))).toEqual({
    kind: "open_folder",
    path: "/tmp/puffer/pixel.png"
  });
});

test("returns download for remote URL image files", () => {
  expect(
    attachmentOverlayAction(
      attachment({
        source: { kind: "remote_url", url: "https://example.test/pixel.png" },
        previewUrl: "https://example.test/preview.png"
      })
    )
  ).toEqual({
    kind: "download",
    url: "https://example.test/pixel.png",
    suggestedName: "pixel.png"
  });
});

test("returns open folder for generated media with a local path", () => {
  expect(
    attachmentOverlayAction(
      attachment({
        source: {
          kind: "generated_media",
          jobId: "job-1",
          artifactId: "artifact-1",
          index: 0,
          localPath: "/tmp/puffer/.puffer/media/images/artifact-1/pixel.png"
        }
      })
    )
  ).toEqual({
    kind: "open_folder",
    path: "/tmp/puffer/.puffer/media/images/artifact-1/pixel.png"
  });
});

test("returns open folder for generated video with a local path", () => {
  expect(
    attachmentOverlayAction({
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

test("returns null for generated media without a local path even when it has a preview URL", () => {
  expect(
    attachmentOverlayAction(
      attachment({
        source: {
          kind: "generated_media",
          jobId: "job-1",
          artifactId: "artifact-1",
          index: 0
        },
        previewUrl: "blob:generated-preview"
      })
    )
  ).toBeNull();
});

test("returns open folder for local non-image files", () => {
  expect(
    attachmentOverlayAction(
      attachment({
        name: "report.pdf",
        kind: "file",
        mimeType: "application/pdf",
        extension: "PDF",
        source: { kind: "local_file", path: "/tmp/puffer/report.pdf" }
      })
    )
  ).toEqual({ kind: "open_folder", path: "/tmp/puffer/report.pdf" });
});

test("returns null for remote-only files with no local path", () => {
  expect(
    attachmentOverlayAction(
      attachment({
        name: "report.pdf",
        kind: "file",
        mimeType: "application/pdf",
        extension: "PDF",
        source: { kind: "remote_url", url: "https://example.test/report.pdf" }
      })
    )
  ).toBeNull();
});
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
node_modules/.bin/vitest run src/lib/screens/agent/attachmentOverlayAction.test.ts
```

Expected: FAIL — the source module still exports `imageOverlayAction`, so the import `{ attachmentOverlayAction }` is `undefined` and every test errors (e.g. `attachmentOverlayAction is not a function`).

- [ ] **Step 4: Replace the source file with the renamed, file-aware logic**

Write `src/lib/screens/agent/attachmentOverlayAction.ts` with this exact content (renames the type + function, drops the `kind` early-return, branches only on `source.kind`):

```ts
import type { MessageAttachment } from "../../types";

export type AttachmentOverlayAction =
  | { kind: "open_folder"; path: string }
  | { kind: "download"; url: string; suggestedName: string };

export function attachmentOverlayAction(
  attachment: MessageAttachment | null
): AttachmentOverlayAction | null {
  if (!attachment) return null;

  switch (attachment.source.kind) {
    case "local_file":
      return attachment.source.path
        ? { kind: "open_folder", path: attachment.source.path }
        : null;
    case "generated_media":
      return attachment.source.localPath
        ? { kind: "open_folder", path: attachment.source.localPath }
        : null;
    case "remote_url":
      return attachment.kind === "image" && attachment.source.url
        ? { kind: "download", url: attachment.source.url, suggestedName: attachment.name }
        : null;
  }
}
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
node_modules/.bin/vitest run src/lib/screens/agent/attachmentOverlayAction.test.ts
```

Expected: PASS — 7 passed.

- [ ] **Step 6: Commit**

```bash
git add src/lib/screens/agent/attachmentOverlayAction.ts src/lib/screens/agent/attachmentOverlayAction.test.ts
git commit -m "feat(desktop): open-folder action for local file attachments"
```

---

## Task 2: Rust command — rename and prove file-agnostic

**Files:**
- Modify: `apps/puffer-desktop/src-tauri/src/image_actions.rs` (cmd `open_image_containing_folder`:21, resolver `resolve_image_containing_folder`:43, tests:264-277)
- Modify: `apps/puffer-desktop/src-tauri/src/lib.rs` (allowlist string:56, handler:525)

Run cargo commands from repo root (`/Users/zhangxiao/Documents/work/github/puffer`).

- [ ] **Step 1: Add the failing file-agnostic test**

In `image_actions.rs`, inside `mod tests`, add this test (references the not-yet-renamed `resolve_containing_folder`):

```rust
    #[test]
    fn containing_folder_resolves_for_non_image_files() {
        let temp = tempfile::tempdir().unwrap();
        let doc = temp.path().join("report.pdf");
        std::fs::write(&doc, b"%PDF-1.4").unwrap();

        assert_eq!(
            super::resolve_containing_folder(&doc).unwrap(),
            temp.path().canonicalize().unwrap()
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p corbina containing_folder
```

Expected: FAIL — compile error `cannot find function 'resolve_containing_folder' in module 'super'` (the function is still named `resolve_image_containing_folder`).

- [ ] **Step 3: Rename the command and resolver, generalize error strings**

In `image_actions.rs`, replace the command (lines 19-28) with:

```rust
/// Opens the folder containing an absolute, existing file path.
#[tauri::command]
pub(crate) fn open_containing_folder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let dir = resolve_containing_folder(Path::new(&path))?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| error.to_string())
}
```

And replace the resolver (lines 43-55) with:

```rust
fn resolve_containing_folder(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("path must be absolute".to_string());
    }
    let canonical = path.canonicalize().map_err(|error| error.to_string())?;
    if !canonical.is_file() {
        return Err("path must be an existing file".to_string());
    }
    canonical
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "path has no containing folder".to_string())
}
```

Then rename the references inside the existing first test. Replace the test function (lines 264-277) with:

```rust
    #[test]
    fn containing_folder_requires_absolute_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let image = temp.path().join("pixel.png");
        std::fs::write(&image, b"png").unwrap();

        assert_eq!(
            super::resolve_containing_folder(&image).unwrap(),
            temp.path().canonicalize().unwrap()
        );
        assert!(super::resolve_containing_folder(std::path::Path::new("pixel.png")).is_err());
        assert!(super::resolve_containing_folder(temp.path()).is_err());
        assert!(super::resolve_containing_folder(&temp.path().join("missing.png")).is_err());
    }
```

- [ ] **Step 4: Update the command registration in lib.rs**

In `lib.rs`, change the allowlist string (line 56) from `"open_image_containing_folder",` to:

```rust
    "open_containing_folder",
```

And change the handler registration (line 525) from `image_actions::open_image_containing_folder,` to:

```rust
            image_actions::open_containing_folder,
```

Leave `open_image_dir` (allowlist + handler) and `download_image_from_url` unchanged.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cargo test -p corbina containing_folder
```

Expected: PASS — `containing_folder_requires_absolute_existing_file` and `containing_folder_resolves_for_non_image_files` both pass.

- [ ] **Step 6: Commit**

```bash
git add apps/puffer-desktop/src-tauri/src/image_actions.rs apps/puffer-desktop/src-tauri/src/lib.rs
git commit -m "refactor(desktop): rename open_image_containing_folder to open_containing_folder"
```

---

## Task 3: Frontend bridge and overlay — wire up the renamed command and generic label

**Files:**
- Modify: `apps/puffer-desktop/src/lib/api/desktop.ts:2544-2549`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte` (imports:3,6; action:21; key signature:53; call:69; label:22-28)

Run commands from `apps/puffer-desktop/`.

- [ ] **Step 1: Rename the desktop bridge function**

In `desktop.ts`, replace the function (lines 2544-2549) with:

```ts
export async function openContainingFolder(path: string): Promise<void> {
  if (!canInvokeTauri()) {
    throw new Error("Opening a folder requires the Tauri desktop shell.");
  }
  await invoke("open_containing_folder", { path });
}
```

- [ ] **Step 2: Update the overlay imports**

In `AttachmentOverlay.svelte`, change line 3 from `import { downloadImageFromUrl, openImageContainingFolder } from "../../api/desktop";` to:

```ts
  import { downloadImageFromUrl, openContainingFolder } from "../../api/desktop";
```

And change line 6 from `import { imageOverlayAction, type ImageOverlayAction } from "./imageOverlayAction";` to:

```ts
  import { attachmentOverlayAction, type AttachmentOverlayAction } from "./attachmentOverlayAction";
```

- [ ] **Step 3: Update the action derivation and generic label**

In `AttachmentOverlay.svelte`, change line 21 from `let overlayAction = $derived(imageOverlayAction(attachment));` to:

```ts
  let overlayAction = $derived(attachmentOverlayAction(attachment));
```

Then replace the `overlayActionLabel` block (lines 22-28) with a single generic label:

```ts
  let overlayActionLabel = $derived(
    overlayAction?.kind === "download" ? "Download image" : "Open containing folder"
  );
```

- [ ] **Step 4: Update the remaining references**

In `AttachmentOverlay.svelte`, change the `overlayActionKey` signature (line 53) from `function overlayActionKey(action: ImageOverlayAction | null): string {` to:

```ts
  function overlayActionKey(action: AttachmentOverlayAction | null): string {
```

And change the open-folder call (line 69) from `await openImageContainingFolder(action.path);` to:

```ts
        await openContainingFolder(action.path);
```

- [ ] **Step 5: Type-check the frontend**

```bash
node_modules/.bin/svelte-check --tsconfig ./tsconfig.json
```

Expected: no errors referencing `imageOverlayAction`, `ImageOverlayAction`, or `openImageContainingFolder`. (Pre-existing unrelated warnings, if any, are acceptable — there must be zero errors in `AttachmentOverlay.svelte` and `desktop.ts`.)

- [ ] **Step 6: Confirm no stale references remain**

```bash
grep -rn --include='*.ts' --include='*.svelte' --include='*.rs' \
  -e 'imageOverlayAction' -e 'ImageOverlayAction' -e 'openImageContainingFolder' \
  -e 'open_image_containing_folder' -e 'resolve_image_containing_folder' \
  /Users/zhangxiao/Documents/work/github/puffer
```

Expected: no output (all references renamed).

- [ ] **Step 7: Commit**

```bash
git add src/lib/api/desktop.ts src/lib/screens/agent/AttachmentOverlay.svelte
git commit -m "feat(desktop): generic open-containing-folder action in attachment overlay"
```

---

## Task 4: Full verification

- [ ] **Step 1: Frontend unit tests**

```bash
cd apps/puffer-desktop
node_modules/.bin/vitest run src/lib/screens/agent/attachmentOverlayAction.test.ts
```

Expected: PASS — 7 passed.

- [ ] **Step 2: Rust tests + compile**

```bash
cd /Users/zhangxiao/Documents/work/github/puffer
cargo test -p corbina containing_folder
```

Expected: PASS — 2 tests pass, crate compiles (proves lib.rs handler/allowlist match the renamed command).

- [ ] **Step 3: Frontend type-check**

```bash
cd apps/puffer-desktop
node_modules/.bin/svelte-check --tsconfig ./tsconfig.json
```

Expected: zero errors in the touched files.

- [ ] **Step 4: Manual smoke test (desktop app)**

Run the app (`npm run tauri dev` or the project's usual launch). In a chat that has:
1. an **image** attachment → overlay shows "Open containing folder" → click opens the folder. ✅
2. a generated **image/video** result → overlay still opens its folder (regression check). ✅
3. a non-image **file** attachment that exists on disk (e.g. a PDF) → overlay now shows "Open containing folder" → click opens the folder. ✅ (This is the new capability.)
4. Verify the media **settings** "open image dir" button (MediaSettingsModal) still works — confirms `open_image_dir` was left intact. ✅
