# Generated Image Placement And Local Links Execution Plan

**Goal:** In desktop chat, assistant messages that mention generated image
paths should render the generated thumbnails below that text, while local file
paths remain clickable and open in the Files tab.

**Spec:** `docs/superpowers/specs/2026-06-08-generated-image-placement-local-links-design.md`

**Scope:** Frontend rendering and Playwright coverage only unless the focused
test exposes an existing local-link parser bug.

## Scope Check

In scope:

- assistant messages with `generated_media` attachments;
- local path links already present in assistant text;
- existing attachment thumbnail and preview overlay;
- existing Files tab open intent path.

Out of scope:

- backend DTO or RPC changes;
- new media/artifact models;
- generated output path synthesis;
- Finder/open-external actions;
- Files tree root mounting for external paths;
- global thumbnail caches or gallery UI;
- CLI/TUI changes.

## Expected File Touches

- `apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte`
  - Add a tiny helper that detects generated-media attachments.
  - Render attachment previews after body only for assistant messages with
    generated-media attachments.
  - Preserve current ordering for user messages and non-generated attachments.

- `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Add one regression covering text-path links above generated thumbnails.

- `docs/superpowers/specs/2026-06-08-generated-image-placement-local-links-design.md`
  - Keep as the implementation reference.

- Optional, only if the new test exposes a parser gap:
  - `apps/puffer-desktop/src/lib/components/MessageBody.svelte`
  - Add the smallest local-path punctuation handling fix needed by the test.

## Task 1: Add The Failing UI Regression

- [ ] In `chat-session-ui.spec.ts`, create a fake session with one assistant
  message whose body contains two absolute generated image paths.
- [ ] Give that assistant message two `generated_media` image attachments using
  existing helper shapes from generated image tests.
- [ ] Seed generated preview bytes for both artifacts.
- [ ] Seed file contents for the same two local paths so clicking the text link
  can drive the Files tab through `read_file`.
- [ ] Assert both paths are rendered as links.
- [ ] Assert the first path link is before the first generated thumbnail in DOM
  order. Prefer `compareDocumentPosition` over pixel coordinates.
- [ ] Click the first path link and wait for a `read_file` request with that
  path.
- [ ] Switch back to Chat, click the first thumbnail, and assert the attachment
  overlay opens.

Expected before implementation: the DOM-order assertion fails because
generated thumbnails currently render before the assistant message body.

## Task 2: Add Minimal Generated-Attachment Ordering

- [ ] In `ConversationView.svelte`, add a helper equivalent to:

```ts
function hasGeneratedMediaAttachments(item: MessageTimelineItem): boolean {
  return Boolean(
    item.attachments?.some((attachment) => attachment.source.kind === "generated_media")
  );
}
```

- [ ] Use the helper only in the assistant-row message rendering path.
- [ ] For generated-media assistant messages, render:
  - visible body if non-empty;
  - `MessageAttachmentPreviewStrip` if attachments exist.
- [ ] For all other messages, keep the existing attachment/body order.
- [ ] Keep `visibleMessageBody` unchanged.
- [ ] Do not alter `MessageAttachmentPreviewStrip`, preview loading, or
  `AgentDetail` routing.

Implementation guard: avoid extracting a generic message layout component in
this change. The duplication is small and local; a new abstraction would be
more risk than value here.

## Task 3: Fix Only Proven Parser Gaps

Run the new test after Task 2.

- [ ] If the generated paths link correctly, skip this task.
- [ ] If a path fails because of trailing punctuation in the test fixture,
  adjust `MessageBody.svelte` punctuation splitting narrowly.
- [ ] Do not add broad URL parsing, filesystem probing, or backend validation to
  the message parser.

## Task 4: Run Focused Verification

- [ ] Run the focused Playwright test for the new regression.
- [ ] Run existing generated-media chat tests in `chat-session-ui.spec.ts`.
- [ ] Run the existing local-link/tool-row test in `files-workspace-ui.spec.ts`
  if practical.

Suggested commands:

```bash
pnpm --dir apps/puffer-desktop exec playwright test tests/chat-session-ui.spec.ts -g "generated"
pnpm --dir apps/puffer-desktop exec playwright test tests/files-workspace-ui.spec.ts -g "local file links"
```

If broader confidence is needed, run `pnpm --dir apps/puffer-desktop run
test:desktop -- tests/chat-session-ui.spec.ts`.

## Task 5: Final Review

- [ ] Check `git diff` for accidental backend, DTO, or media-runtime changes.
- [ ] Confirm no generated path is synthesized from structured tool output.
- [ ] Confirm user-upload attachment ordering is unchanged.
- [ ] Confirm generated thumbnail click still opens the overlay.
- [ ] Confirm local path click still opens Files.
- [ ] Update `specs/puffer-desktop/679.md` only if implementation behavior
  diverges from the docs/superpowers spec.

## Stop Conditions

Stop and revisit the spec if implementation appears to need:

- a backend allowed-root change;
- generated-media DTO changes;
- frontend parsing of `ImageGeneration` tool output;
- a new file reveal/open-external command;
- a second Files tree root for generated media paths;
- broad reordering of all message attachments.
