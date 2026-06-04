# Chat Attachment Open Intents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make chat-visible attachments, message file paths, and tool file references open through one frontend intent route.

**Architecture:** Add a tiny shared `ChatOpenIntent` type, route it through `ConversationView` and `AgentDetail`, and render one on-demand attachment overlay for image previews and unavailable attachment details. Keep uploaded attachment storage transient; do not change daemon APIs or transcript persistence.

**Tech Stack:** Svelte 5 runes, existing Puffer desktop components, browser object URLs, Playwright, fake daemon.

---

## Scope And Guardrails

Implement only `docs/superpowers/specs/2026-06-04-chat-attachment-open-intents-design.md`.

Do not modify Rust daemon code, `run_agent_turn`, backend timeline item schemas, persisted transcript storage, file upload limits, Tauri drag/drop behavior, or attachment payload shape sent to the daemon.

Do not add a cache, dependency, right-click menu, download action, persistent object URL, or non-image file preview.

## File Structure

- Create: `apps/puffer-desktop/src/lib/chatOpenIntent.ts`
  - Owns the two frontend open intents and constructors.

- Create: `apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte`
  - Owns image preview and unavailable attachment detail UI.

- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte`
  - Emits attachment intents only in `message` mode.

- Modify: `apps/puffer-desktop/src/lib/components/MessageBody.svelte`
  - Emits file intents for local file links.

- Modify: `apps/puffer-desktop/src/lib/screens/agent/ToolCard.svelte`
  - Emits file intents for tool file targets.

- Modify: `apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte`
  - Receives child intents and forwards them to `AgentDetail`.

- Modify: `apps/puffer-desktop/src/lib/screens/agent/AgentDetailContent.svelte`
  - Passes `onOpenChatIntent` through to `ConversationView`, `MessageBody`, and `ToolCard` callers.

- Modify: `apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte`
  - Owns intent routing, Files tab selection, and attachment overlay state.

- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Adds focused UI coverage for image preview, unavailable attachment detail, and file-target routing.

- Create: `specs/puffer-desktop/665.md`
  - Documents behavior and compatibility constraints.

## Task 1: Add Failing Chat Open Target Tests

**Files:**
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`

- [ ] **Step 1: Add the message attachment overlay regression test**

Add this test after `composer add content menu attaches image and file drafts`:

```ts
test("message attachments open image preview and file details", async ({ page }) => {
  const imageBuffer = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAFgwJ/lzTnGQAAAABJRU5ErkJggg==",
    "base64"
  );
  const daemon = new FakeDaemon({
    sessions: [
      {
        sessionId: "session-attachment-open",
        displayName: "Attachment open targets",
        title: "Attachment open targets",
        cwd: "/tmp/puffer",
        folderPath: "/tmp/puffer",
        updatedAtMs: baseTime,
        createdAtMs: baseTime - 60_000,
        eventCount: 1,
        timeline: [
          {
            kind: "assistant_message",
            id: "attachment-open-seed",
            text: "Attach a screenshot and notes.",
            createdAtMs: baseTime - 30_000
          }
        ]
      }
    ]
  });

  await daemon.install(page);
  await daemon.open(page);

  await openSession(page, /Attachment open targets/);
  await page.getByRole("button", { name: "Add content" }).click();
  await page.locator('[data-testid="composer-file-input"]').setInputFiles([
    {
      name: "sample.png",
      mimeType: "image/png",
      buffer: imageBuffer
    },
    {
      name: "notes.md",
      mimeType: "text/markdown",
      buffer: Buffer.from("# Notes\n\nReview this.", "utf8")
    }
  ]);
  await page.getByRole("button", { name: "Send" }).click();
  await daemon.waitForRequest(
    "run_agent_turn",
    (request) =>
      request.params.sessionId === "session-attachment-open" &&
      request.params.message === "[Image: sample.png]\n[File: notes.md]"
  );

  await page.getByRole("button", { name: "Open image attachment sample.png" }).click();
  const previewDialog = page.getByRole("dialog", { name: "sample.png" });
  await expect(previewDialog).toBeVisible();
  await expect(previewDialog.getByAltText("sample.png")).toBeVisible();
  await expect(previewDialog).toContainText("PNG");
  await page.keyboard.press("Escape");
  await expect(page.locator('[data-testid="attachment-overlay"]')).toHaveCount(0);

  await page.getByRole("button", { name: "Open attachment details for notes.md" }).click();
  const detailsDialog = page.getByRole("dialog", { name: "notes.md" });
  await expect(detailsDialog).toBeVisible();
  await expect(detailsDialog).toContainText("Preview unavailable for this attachment.");
  await expect(detailsDialog).toContainText("text/markdown");
  await expect(page.getByRole("button", { name: "Files" })).toHaveAttribute("aria-pressed", "false");
});
```

- [ ] **Step 2: Add the restored pending attachment detail test**

Add this test after the message attachment overlay test:

```ts
test("restored pending attachment without preview opens unavailable detail", async ({ page }) => {
  const sessionId = "session-restored-attachment";
  await page.addInitScript(
    ({ key, expiresAtMs }) => {
      window.localStorage.setItem(
        key,
        JSON.stringify({
          expiresAtMs,
          item: {
            id: "pending-stale-image",
            kind: "user",
            createdAtMs: Date.now(),
            title: "User",
            summary: "stale.png",
            body: "[Image: stale.png]",
            meta: ["1 attachment"],
            attachments: [
              {
                id: "stale-image",
                name: "stale.png",
                mimeType: "image/png",
                size: 68,
                extension: "PNG",
                kind: "image"
              }
            ]
          }
        })
      );
    },
    {
      key: `puffer-desktop:pending-submitted:${sessionId}`,
      expiresAtMs: Date.now() + 10 * 60_000
    }
  );

  const daemon = new FakeDaemon({
    sessions: [
      {
        sessionId,
        displayName: "Restored attachment",
        title: "Restored attachment",
        cwd: "/tmp/puffer",
        folderPath: "/tmp/puffer",
        updatedAtMs: baseTime,
        createdAtMs: baseTime - 60_000,
        eventCount: 0,
        timeline: []
      }
    ]
  });

  await daemon.install(page);
  await daemon.open(page);

  await openSession(page, /Restored attachment/);
  await expect(page.getByText("stale.png")).toBeVisible();
  await expect(page.getByText("[Image: stale.png]")).toHaveCount(0);

  await page.getByRole("button", { name: "Open image attachment stale.png" }).click();
  const dialog = page.getByRole("dialog", { name: "stale.png" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("Preview unavailable for this attachment.");
});
```

- [ ] **Step 3: Add the unified file target routing test**

Add this test after the restored pending attachment test:

```ts
test("chat file targets route message paths and tool paths through Files", async ({ page }) => {
  const messagePath = "/tmp/puffer/src/main.rs";
  const toolPath = "/tmp/puffer/src/tool.rs";
  const daemon = new FakeDaemon({
    sessions: [
      {
        sessionId: "session-file-open-targets",
        displayName: "File open targets",
        title: "File open targets",
        cwd: "/tmp/puffer",
        folderPath: "/tmp/puffer",
        updatedAtMs: baseTime,
        createdAtMs: baseTime - 60_000,
        eventCount: 2,
        timeline: [
          {
            kind: "assistant_message",
            id: "file-open-message",
            text: `Review ${messagePath}:2 before changing the helper.`,
            createdAtMs: baseTime - 30_000
          },
          {
            kind: "tool_call",
            id: "file-open-tool",
            toolId: "read_file",
            status: "success",
            summary: `Read ${toolPath}`,
            inputJson: { path: toolPath },
            outputText: JSON.stringify({ content: "pub fn helper() {}\n" }),
            createdAtMs: baseTime - 20_000
          }
        ]
      }
    ]
  });
  daemon.seedFile(messagePath, "fn main() {\n    let target = 42;\n}\n");
  daemon.seedFile(toolPath, "pub fn helper() {}\n");

  await daemon.install(page);
  await daemon.open(page);

  await openSession(page, /File open targets/);
  await page.getByRole("link", { name: `${messagePath}:2` }).click();
  await expect(page.getByRole("button", { name: "Files" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".viewer")).toContainText(messagePath);
  await expect(page.locator(".viewer")).toContainText("let target = 42");

  await page.getByRole("button", { name: "Chat" }).click();
  await page.getByRole("button", { name: toolPath }).click();
  await expect(page.getByRole("button", { name: "Files" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".viewer")).toContainText(toolPath);
  await expect(page.locator(".viewer")).toContainText("pub fn helper() {}");
});
```

- [ ] **Step 4: Run the focused tests and verify they fail**

From `apps/puffer-desktop`, run:

```bash
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "message attachments open image preview and file details|restored pending attachment without preview opens unavailable detail|chat file targets route message paths and tool paths through Files"
```

Expected: tests fail because attachment cards are not buttons, there is no attachment overlay, and tool-card file paths are not routed through the new shared intent.

## Task 2: Add The Shared Intent Type And Child Emitters

**Files:**
- Create: `apps/puffer-desktop/src/lib/chatOpenIntent.ts`
- Modify: `apps/puffer-desktop/src/lib/components/MessageBody.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/ToolCard.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte`

- [ ] **Step 1: Create the shared intent module**

Create `apps/puffer-desktop/src/lib/chatOpenIntent.ts`:

```ts
import type { MessageAttachment } from "./types";

export type ChatOpenIntent =
  | { kind: "file"; path: string; line: number | null }
  | { kind: "attachment"; attachment: MessageAttachment };

export function fileOpenIntent(path: string, line: number | null = null): ChatOpenIntent {
  return { kind: "file", path, line };
}

export function attachmentOpenIntent(attachment: MessageAttachment): ChatOpenIntent {
  return { kind: "attachment", attachment };
}
```

- [ ] **Step 2: Update MessageBody to emit file intents**

In `apps/puffer-desktop/src/lib/components/MessageBody.svelte`, add the import:

```svelte
  import { fileOpenIntent, type ChatOpenIntent } from "../chatOpenIntent";
```

Replace the exported callback:

```svelte
  export let onOpenFile: ((path: string, line?: number | null) => void) | undefined = undefined;
```

with:

```svelte
  export let onOpenChatIntent: ((intent: ChatOpenIntent) => void) | undefined = undefined;
```

Replace `openFileLink` with:

```svelte
  function openFileLink(event: MouseEvent, href: string) {
    const target = fileTarget(href);
    if (!target) return;
    event.preventDefault();
    onOpenChatIntent?.(fileOpenIntent(target.path, target.line));
  }
```

- [ ] **Step 3: Update ToolCard to emit file intents**

In `apps/puffer-desktop/src/lib/screens/agent/ToolCard.svelte`, add the import:

```svelte
  import { fileOpenIntent, type ChatOpenIntent } from "../../chatOpenIntent";
```

Replace the prop type:

```svelte
    onOpenFile?: (path: string, line?: number | null) => void;
```

with:

```svelte
    onOpenChatIntent?: (intent: ChatOpenIntent) => void;
```

Replace the destructured prop:

```svelte
    onOpenFile
```

with:

```svelte
    onOpenChatIntent
```

Replace `openFilePath` with:

```svelte
  function openFilePath(path: string) {
    const target = fileTarget(path);
    if (!target) return;
    onOpenChatIntent?.(fileOpenIntent(target.path, target.line));
  }
```

Change the read/diff file path rendering from:

```svelte
            <div class="pf-file-path" title={toolRender.path}>{toolRender.path}</div>
```

to:

```svelte
            <button
              type="button"
              class="pf-file-path pf-file-path-button"
              title={toolRender.path}
              onclick={() => openFilePath(toolRender.path)}
            >
              {toolRender.path}
            </button>
```

Add CSS below the existing `.pf-file-path` rule:

```css
  .pf-file-path-button {
    width: 100%;
    border: 0;
    text-align: left;
    cursor: pointer;
  }
  .pf-file-path-button:hover {
    color: var(--foreground);
    text-decoration: underline;
  }
```

- [ ] **Step 4: Update AttachmentPreviewStrip to emit attachment intents in message mode**

In `apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte`, add the import:

```svelte
  import { attachmentOpenIntent, type ChatOpenIntent } from "../../chatOpenIntent";
```

Add the callback prop:

```svelte
    onOpenChatIntent?: (intent: ChatOpenIntent) => void;
```

Include it in destructuring:

```svelte
    onOpenChatIntent
```

Add this snippet inside the instance script:

```svelte
  function attachmentOpenLabel(attachment: AttachmentPreviewItem): string {
    return attachment.kind === "image"
      ? `Open image attachment ${attachment.name}`
      : `Open attachment details for ${attachment.name}`;
  }
```

Replace the inner each body with:

```svelte
      {#if variant === "message"}
        <button
          type="button"
          class="pf-attachment-preview pf-attachment-preview-action"
          aria-label={attachmentOpenLabel(attachment)}
          title={attachment.name}
          onclick={() => onOpenChatIntent?.(attachmentOpenIntent(attachment))}
        >
          {#if attachment.previewUrl && attachment.kind === "image"}
            <div class="pf-attachment-thumb">
              <img src={attachment.previewUrl} alt={attachment.name} draggable="false" />
            </div>
          {:else}
            <div class="pf-attachment-file-card" data-kind={attachment.kind}>
              <span class="pf-attachment-file-icon">
                <Icon name="file" size={18} />
              </span>
              <span class="pf-attachment-file-copy">
                <span class="pf-attachment-file-name">{attachment.name}</span>
                <span class="pf-attachment-file-ext">{attachment.extension}</span>
              </span>
            </div>
          {/if}
        </button>
      {:else}
        <div class="pf-attachment-preview">
          {#if attachment.previewUrl && attachment.kind === "image"}
            <div class="pf-attachment-thumb">
              <img src={attachment.previewUrl} alt={attachment.name} draggable="false" />
            </div>
          {:else}
            <div class="pf-attachment-file-card" data-kind={attachment.kind}>
              <span class="pf-attachment-file-icon">
                <Icon name="file" size={18} />
              </span>
              <span class="pf-attachment-file-copy">
                <span class="pf-attachment-file-name">{attachment.name}</span>
                <span class="pf-attachment-file-ext">{attachment.extension}</span>
              </span>
            </div>
          {/if}
          {#if removable}
            <button
              type="button"
              class="pf-attachment-remove"
              aria-label={`Remove attachment ${attachment.name}`}
              title="Remove attachment"
              onclick={() => onRemove?.(attachment.id)}
            >
              <Icon name="x" size={13} />
            </button>
          {/if}
        </div>
      {/if}
```

Add CSS after `.pf-attachment-preview`:

```css
  .pf-attachment-preview-action {
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .pf-attachment-preview-action:hover .pf-attachment-thumb,
  .pf-attachment-preview-action:hover .pf-attachment-file-card {
    border-color: color-mix(in oklab, var(--primary) 58%, var(--border));
  }
  .pf-attachment-preview-action:focus-visible {
    outline: 2px solid color-mix(in oklab, var(--primary) 70%, white);
    outline-offset: 2px;
    border-radius: 8px;
  }
```

- [ ] **Step 5: Run typecheck or focused test compile**

From `apps/puffer-desktop`, run:

```bash
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "message attachments open image preview and file details"
```

Expected: the test still fails because no parent is routing intents and no overlay exists, but Svelte compilation should pass.

## Task 3: Route Intents Through Conversation And Agent Detail

**Files:**
- Create: `apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AgentDetailContent.svelte`
- Modify: `apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte`

- [ ] **Step 1: Add the attachment overlay component**

Create `apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte`:

```svelte
<script lang="ts">
  import { tick } from "svelte";
  import Icon from "../../design/Icon.svelte";
  import type { MessageAttachment } from "../../types";

  type Props = {
    attachment: MessageAttachment | null;
    onClose: () => void;
  };

  let { attachment, onClose }: Props = $props();
  let closeButtonEl: HTMLButtonElement | undefined;
  let titleId = $derived(attachment ? `attachment-overlay-title-${attachment.id}` : "attachment-overlay-title");
  let canPreviewImage = $derived(Boolean(attachment?.kind === "image" && attachment.previewUrl));

  function formatBytes(size: number): string {
    if (!Number.isFinite(size) || size < 0) return "Unknown size";
    if (size < 1024) return `${size} B`;
    const kib = size / 1024;
    if (kib < 1024) return `${kib.toFixed(kib >= 10 ? 0 : 1)} KiB`;
    const mib = kib / 1024;
    return `${mib.toFixed(mib >= 10 ? 0 : 1)} MiB`;
  }

  function close() {
    onClose();
  }

  $effect(() => {
    if (!attachment || typeof window === "undefined") return;
    void tick().then(() => closeButtonEl?.focus());
    const handleKeydown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      close();
    };
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
</script>

{#if attachment}
  <div
    class="pf-attachment-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby={titleId}
    data-testid="attachment-overlay"
  >
    <button
      type="button"
      class="pf-attachment-overlay-backdrop"
      aria-label="Close attachment preview"
      onclick={close}
    ></button>
    <section class="pf-attachment-dialog">
      <header class="pf-attachment-dialog-head">
        <div>
          <h2 id={titleId}>{attachment.name}</h2>
          <p>{attachment.extension} · {attachment.mimeType} · {formatBytes(attachment.size)}</p>
        </div>
        <button
          bind:this={closeButtonEl}
          type="button"
          class="pf-attachment-dialog-close"
          aria-label="Close attachment preview"
          onclick={close}
        >
          <Icon name="x" size={15} />
        </button>
      </header>

      {#if canPreviewImage && attachment.previewUrl}
        <div class="pf-attachment-image-frame">
          <img src={attachment.previewUrl} alt={attachment.name} draggable="false" />
        </div>
      {:else}
        <div class="pf-attachment-unavailable">
          <span class="pf-attachment-unavailable-icon">
            <Icon name="file" size={24} />
          </span>
          <strong>Preview unavailable for this attachment.</strong>
          <span>This chat item has attachment metadata, but no durable preview content.</span>
        </div>
      {/if}
    </section>
  </div>
{/if}

<style>
  .pf-attachment-overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding: 32px;
  }
  .pf-attachment-overlay-backdrop {
    position: absolute;
    inset: 0;
    border: 0;
    background: color-mix(in oklab, black 48%, transparent);
  }
  .pf-attachment-dialog {
    position: relative;
    width: min(860px, 100%);
    max-height: min(760px, 90vh);
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--background);
    box-shadow: var(--shadow-lg);
  }
  .pf-attachment-dialog-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  .pf-attachment-dialog-head h2 {
    margin: 0;
    font-size: 14px;
    line-height: 20px;
    font-weight: 700;
  }
  .pf-attachment-dialog-head p {
    margin: 2px 0 0;
    color: var(--muted-foreground);
    font-size: 12px;
    line-height: 16px;
  }
  .pf-attachment-dialog-close {
    width: 30px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--background);
    color: var(--muted-foreground);
    cursor: pointer;
  }
  .pf-attachment-dialog-close:hover {
    color: var(--foreground);
    background: var(--accent);
  }
  .pf-attachment-image-frame {
    min-height: 240px;
    display: grid;
    place-items: center;
    overflow: auto;
    background: color-mix(in oklab, var(--muted) 45%, black);
  }
  .pf-attachment-image-frame img {
    max-width: 100%;
    max-height: 72vh;
    display: block;
    object-fit: contain;
  }
  .pf-attachment-unavailable {
    min-height: 240px;
    display: grid;
    place-items: center;
    align-content: center;
    gap: 8px;
    padding: 32px;
    color: var(--muted-foreground);
    text-align: center;
  }
  .pf-attachment-unavailable strong {
    color: var(--foreground);
    font-size: 14px;
  }
  .pf-attachment-unavailable-icon {
    width: 48px;
    height: 48px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--muted);
  }
</style>
```

- [ ] **Step 2: Update ConversationView props and child calls**

In `ConversationView.svelte`, import the type:

```svelte
  import type { ChatOpenIntent } from "../../chatOpenIntent";
```

Add this prop to `Props`:

```ts
    onOpenChatIntent?: (intent: ChatOpenIntent) => void;
```

Add it to destructuring:

```ts
    onOpenChatIntent,
```

Update message attachment strip calls:

```svelte
                  <AttachmentPreviewStrip
                    attachments={row.item.attachments}
                    variant="message"
                    {onOpenChatIntent}
                  />
```

Run this search to list every message body call site:

```bash
rg -n "MessageBody body=.*onOpenFile" apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte
```

For each match, keep the existing `body` attribute expression exactly as it is
and replace only `onOpenFile={onOpenFileLink}` with `{onOpenChatIntent}`.

Run this search to list every tool card call site:

```bash
rg -n "ToolCard item=.*onOpenFile" apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte
```

For each match, keep the existing `item` attribute expression and any other
props exactly as they are, and replace only `onOpenFile={onOpenFileLink}` with
`{onOpenChatIntent}`.

Remove the `onOpenFileLink` prop from `ConversationView.svelte` after all callers use `onOpenChatIntent`.

- [ ] **Step 3: Update AgentDetailContent to pass intents**

In `AgentDetailContent.svelte`, import the type:

```svelte
  import type { ChatOpenIntent } from "../../chatOpenIntent";
```

Replace this prop:

```ts
    onOpenFileLink?: (path: string, line?: number | null) => void;
```

with:

```ts
    onOpenChatIntent?: (intent: ChatOpenIntent) => void;
```

Update destructuring from:

```ts
    onOpenFileLink,
```

to:

```ts
    onOpenChatIntent,
```

Update the `ConversationView` call to pass:

```svelte
      {onOpenChatIntent}
```

- [ ] **Step 4: Update AgentDetail to route intents and render overlay**

In `AgentDetail.svelte`, add imports:

```svelte
  import AttachmentOverlay from "./AttachmentOverlay.svelte";
  import type { ChatOpenIntent } from "../../chatOpenIntent";
  import type { MessageAttachment } from "../../types";
```

Add state near `fileToOpen`:

```ts
  let openAttachment = $state<MessageAttachment | null>(null);
```

Replace `openLinkedFile` with:

```ts
  function openChatIntent(intent: ChatOpenIntent) {
    if (intent.kind === "file") {
      fileToOpen = { path: intent.path, line: intent.line, requestId: ++fileOpenRequestId };
      tab = "files";
      return;
    }
    openAttachment = intent.attachment;
  }
```

When the session changes, also clear the open attachment:

```ts
    openAttachment = null;
```

Update both `AgentDetailContent` calls from:

```svelte
        onOpenFileLink={openLinkedFile}
```

to:

```svelte
        onOpenChatIntent={openChatIntent}
```

Render the overlay near the end of the root markup, before the search panel:

```svelte
  <AttachmentOverlay attachment={openAttachment} onClose={() => (openAttachment = null)} />
```

- [ ] **Step 5: Run the focused tests**

From `apps/puffer-desktop`, run:

```bash
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "message attachments open image preview and file details|restored pending attachment without preview opens unavailable detail|chat file targets route message paths and tool paths through Files"
```

Expected: the three focused tests pass.

## Task 4: Documentation And Regression Sweep

**Files:**
- Create: `specs/puffer-desktop/665.md`
- Modify: none else unless verification exposes a bug in Task 2 or Task 3.

- [ ] **Step 1: Add the puffer-desktop update spec**

Create `specs/puffer-desktop/665.md`:

```md
# puffer-desktop Update 665: Chat Open Targets

## Summary

Chat-visible attachments, message file paths, and tool file references now route
through one frontend open-intent path.

## Behavior

- Image attachments in submitted chat messages open an on-demand preview overlay
  while their object URL is still available.
- File attachments and image attachments without preview data open the same
  overlay in an unavailable detail state.
- Message body file paths and tool-card file targets open the Files tab through
  the same file intent.
- Composer draft attachment remove behavior is unchanged.

## Constraints

This change does not alter daemon APIs, persisted transcript schemas,
attachment upload limits, object URL persistence, or file download behavior.
Attachment previews remain transient frontend presentation state.

## Verification

Playwright coverage in `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
checks image preview, unavailable attachment detail, restored pending attachment
detail, message path routing, tool path routing, and unchanged composer removal.
```

- [ ] **Step 2: Run adjacent attachment and composer tests**

From `apps/puffer-desktop`, run:

```bash
npm run test:desktop -- tests/chat-session-ui.spec.ts -g "composer add content menu attaches image and file drafts|chat surface drop attaches image and file drafts|message attachments open image preview and file details|restored pending attachment without preview opens unavailable detail|chat file targets route message paths and tool paths through Files"
```

Expected: all selected tests pass.

- [ ] **Step 3: Search for stale callback names**

From repo root, run:

```bash
rg -n "onOpenFileLink|onOpenFile=|onOpenFile\\?|openLinkedFile" apps/puffer-desktop/src
```

Expected: no matches.

- [ ] **Step 4: Review the implementation diff**

From repo root, run:

```bash
git diff -- apps/puffer-desktop/src/lib/chatOpenIntent.ts apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte apps/puffer-desktop/src/lib/components/MessageBody.svelte apps/puffer-desktop/src/lib/screens/agent/ToolCard.svelte apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte apps/puffer-desktop/src/lib/screens/agent/AgentDetailContent.svelte apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte apps/puffer-desktop/tests/chat-session-ui.spec.ts specs/puffer-desktop/665.md
```

Expected:

- no daemon or Rust files changed;
- no persistent attachment storage added;
- no new package dependencies;
- `ChatOpenIntent` has only `file` and `attachment` variants;
- file target routing is owned by `AgentDetail.svelte`;
- attachment object URLs are not copied, cached, or revoked by the overlay.

- [ ] **Step 5: Commit**

From repo root, run:

```bash
git add apps/puffer-desktop/src/lib/chatOpenIntent.ts \
  apps/puffer-desktop/src/lib/screens/agent/AttachmentOverlay.svelte \
  apps/puffer-desktop/src/lib/screens/agent/AttachmentPreviewStrip.svelte \
  apps/puffer-desktop/src/lib/components/MessageBody.svelte \
  apps/puffer-desktop/src/lib/screens/agent/ToolCard.svelte \
  apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte \
  apps/puffer-desktop/src/lib/screens/agent/AgentDetailContent.svelte \
  apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte \
  apps/puffer-desktop/tests/chat-session-ui.spec.ts \
  specs/puffer-desktop/665.md
git commit -m "feat(desktop): unify chat open targets"
```

Expected: one focused implementation commit.
