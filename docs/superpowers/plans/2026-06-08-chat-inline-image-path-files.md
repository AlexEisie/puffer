# Chat Inline Image Path Files Execution Plan

**Goal:** Inline-code local file paths in chat open the Files pane, and image
files opened in Files render as images. Attachment thumbnails continue to open
the attachment overlay.

**Spec:** `docs/superpowers/specs/2026-06-08-chat-inline-image-path-files-design.md`

**Scope:** Desktop frontend and Playwright coverage only. No backend API,
transcript, attachment DTO, generated-media runtime, or daemon allowlist changes.

## Scope Check

In scope:

- inline code spans that are exactly one `chatFileTarget()`;
- the existing file intent route from chat to Files;
- common image previews in Files for `.jpg`, `.jpeg`, `.png`, `.webp`, and `.gif`;
- existing generated image thumbnails and attachment overlay behavior;
- focused Playwright coverage.

Out of scope:

- opening text image paths directly in the attachment overlay;
- scanning arbitrary paths inside code snippets or fenced code blocks;
- streaming file reads, downloads, MIME sniffing, object URL lifecycle, or new
  backend endpoints;
- generated-media-specific path parsing;
- mounting extra Files roots or changing allowed roots.

## Expected File Touches

- `apps/puffer-desktop/src/lib/components/MessageBody.svelte`
  - Render inline-code local file targets as clickable local links with code
    styling.

- `apps/puffer-desktop/src/lib/screens/agent/filePreview.ts`
  - Add a minimal `image` preview type and extension-to-MIME mapping.
  - Make image preview construction fail clearly for truncated or non-base64
    results.

- `apps/puffer-desktop/src/lib/screens/agent/FilesPane.svelte`
  - Render the new `image` preview kind with a contained `<img>`.

- `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Add an inline-code local path regression that clicks through to Files and
    verifies attachment thumbnail behavior is unchanged.

- `apps/puffer-desktop/tests/files-workspace-ui.spec.ts`
  - Add image preview coverage and assert the rich-preview read cap is used.

## Task 1: Add Failing Chat Regression

- [ ] In `chat-session-ui.spec.ts`, create a fake session with one assistant
  message containing an inline-code absolute path, for example
  `` Image: `/tmp/puffer/.puffer/media/images/artifact-1/image.jpeg` ``.
- [ ] Attach one generated-media image attachment to the same message using the
  existing generated attachment helper shape.
- [ ] Seed generated-media preview bytes so the thumbnail renders.
- [ ] Seed the inline-code path as a binary image file in `FakeDaemon` with
  `seedBinaryFile`.
- [ ] Assert the inline-code path renders as a `link`, not just text.
- [ ] Click the inline-code path link.
- [ ] Wait for `read_file` with that path and `maxBytes` equal to the existing
  Files rich preview cap, 24 MiB.
- [ ] Assert the Files tab is active and the image preview is visible.
- [ ] Return to Chat, click the generated image thumbnail, and assert the
  existing attachment overlay opens.

Expected before implementation: the inline-code path is not a link.

## Task 2: Add Failing Files Image Preview Regression

- [ ] In `files-workspace-ui.spec.ts`, seed a `.jpeg` binary file with valid
  image bytes via `seedBinaryFile`.
- [ ] Open the Files panel and open that file through an existing path-link or
  persisted-tab setup.
- [ ] Assert the `read_file` request includes the 24 MiB rich preview byte cap.
- [ ] Assert Files renders an image preview with an accessible name derived from
  the file name.
- [ ] Assert the generic `Binary file (...)` placeholder is not visible.

Expected before implementation: Files shows the binary placeholder.

## Task 3: Link Inline-Code File Targets

- [ ] In `MessageBody.svelte`, keep the parser structure unchanged.
- [ ] In the render branch for `segment.kind === "code"`, check
  `chatFileTarget(segment.text)`.
- [ ] If it returns a target, render an `<a>` that calls `openFileLink` or emits
  `fileOpenIntent(target.path, target.line)`.
- [ ] Preserve the current code visual treatment by styling the link's child or
  link class as inline code.
- [ ] If the code text is not a file target, keep rendering plain `<code>`.

Implementation guard: do not inspect fenced code blocks, do not scan substrings,
and do not add filesystem existence checks in `MessageBody`.

## Task 4: Add Minimal Image Preview Type

- [ ] In `filePreview.ts`, add an `ImagePreview` type:

```ts
export type ImagePreview = {
  kind: "image";
  src: string;
  alt: string;
};
```

- [ ] Add `image` to the `FilePreview` union and `previewFormat` return type.
- [ ] Classify `.jpg`, `.jpeg`, `.png`, `.webp`, and `.gif` as `image`.
- [ ] Add a small extension-to-MIME helper.
- [ ] In `buildFilePreview`, return an image preview only when
  `file.encoding === "base64"` and `file.truncated === false`.
- [ ] Throw a readable error when an image file is truncated or not base64.
- [ ] Keep all existing document preview behavior unchanged.

Implementation guard: do not decode or validate image bytes in TypeScript. Let
the browser image renderer handle valid data URLs.

## Task 5: Render Image Preview In Files

- [ ] In `FilesPane.svelte`, add one branch for
  `activePreview.kind === "image"`.
- [ ] Render `<img src={activePreview.src} alt={activePreview.alt}>` inside the
  existing preview area.
- [ ] Add scoped CSS that constrains the image to the viewer body with
  `max-width`, `max-height`, and `object-fit: contain`.
- [ ] Keep the existing binary placeholder branch for unsupported binary files.

Implementation guard: do not add zoom, pan, download, context menu, or gallery
controls in this change.

## Task 6: Verification

- [ ] Run Svelte diagnostics:

```bash
npm --prefix apps/puffer-desktop run check
```

- [ ] Run focused chat regression:

```bash
npm --prefix apps/puffer-desktop run test:desktop -- tests/chat-session-ui.spec.ts -g "inline-code"
```

- [ ] Run focused Files regression:

```bash
npm --prefix apps/puffer-desktop run test:desktop -- tests/files-workspace-ui.spec.ts -g "image preview"
```

- [ ] Run existing generated-media coverage:

```bash
npm --prefix apps/puffer-desktop run test:desktop -- tests/chat-session-ui.spec.ts -g "Generated image|generated image"
```

## Final Review

- [ ] Check `git diff` for accidental backend, DTO, daemon, or generated-media
  runtime changes.
- [ ] Confirm plain text local paths still open Files.
- [ ] Confirm inline-code local paths open Files.
- [ ] Confirm attachment thumbnails still open the attachment overlay.
- [ ] Confirm unsupported binary files still show the existing binary
  placeholder.
- [ ] Confirm image previews do not allocate object URLs or add cleanup paths.

## Stop Conditions

Stop and revisit the spec if implementation appears to require:

- backend RPC or allowed-root changes;
- generated-media DTO changes;
- image byte sniffing;
- streaming reads or downloads;
- a new attachment/text-path routing rule;
- broad markdown parser replacement.
