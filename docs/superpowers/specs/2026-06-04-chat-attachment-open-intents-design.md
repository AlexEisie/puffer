# Chat Attachment Open Intents

## Goal

Unify click behavior for chat-visible uploaded attachments, local file paths in
message bodies, and file references shown by tool output.

The desktop chat should treat these as one family of "openable chat objects":

- uploaded image and file attachments rendered in user messages;
- local file paths and `file://` links rendered in message text;
- file references surfaced by tool cards.

The implementation should improve long-term clarity and stability without
introducing a large attachment storage rewrite.

## Scope

This design covers the desktop frontend interaction layer in
`apps/puffer-desktop`.

Included:

- message attachment click handling;
- existing message-body local path clicks;
- existing tool-card file reference clicks;
- a lightweight image preview overlay for message attachments with a live
  `previewUrl`;
- the same overlay showing an attachment detail state for attachments that
  cannot be previewed;
- one shared open-intent route owned by the agent detail screen.

Excluded:

- Rust daemon changes;
- `run_agent_turn` protocol changes;
- persistent transcript schema changes;
- writing uploaded attachment files to disk;
- download actions;
- context menus;
- persisted object URLs or frontend attachment caches;
- composer draft attachment previews beyond the existing remove behavior;
- backend timeline attachment normalization.

## Current State

`ConversationView.svelte` currently renders submitted user-message attachments
through `AttachmentPreviewStrip.svelte`. Image attachments can include a
browser object URL while the optimistic message is alive. Persisted/reloaded
messages keep only attachment metadata.

`MessageBody.svelte` already detects local paths and calls `onOpenFile`.
`ToolCard.svelte` also accepts an `onOpenFile` callback for file references.
These are useful behaviors, but the open routing is split by component surface.

`App.svelte` deliberately strips `previewUrl` before storing pending submitted
messages in `localStorage`. This is correct and should remain unchanged:
object URLs are process-local browser resources, not durable transcript data.

## Design Review Update

The first design separated `file` and `reference` intents. That is more
structure than this change needs. Message body paths and tool-card file targets
both route to the Files tab with the same `path` and optional `line`, so they
should share one `file` intent.

The implementation should use only two intent variants:

- `file`: a local path that opens in the Files tab;
- `attachment`: a rendered message attachment that opens in the attachment
  overlay.

This keeps the boundary useful without adding a taxonomy for behavior that does
not currently differ.

## Architecture

Introduce one small frontend type for chat-object open requests:

```ts
type ChatOpenIntent =
  | { kind: "file"; path: string; line: number | null }
  | { kind: "attachment"; attachment: MessageAttachment };
```

The type should live in a small shared frontend helper module,
`apps/puffer-desktop/src/lib/chatOpenIntent.ts`, because both
`components/MessageBody.svelte` and agent-screen components need it.

Component responsibilities:

- `AttachmentPreviewStrip.svelte` renders attachment cards and thumbnails. In
  message mode, clicking an item emits an attachment intent. In composer mode,
  it keeps the current remove-focused behavior.
- `MessageBody.svelte` keeps local path detection but emits a unified file
  intent instead of owning destination semantics.
- `ToolCard.svelte` emits the same file intent for file targets.
- `ConversationView.svelte` passes intents upward and does not decide which
  panel opens.
- `AgentDetail.svelte` is the single routing owner. It maps file intents to the
  Files tab and maps attachment intents to the attachment overlay.

This keeps chat rendering components dumb and makes future additions, such as
PDF preview or a context menu, route through one place.

## Interaction Rules

Message attachments:

- Image attachment with `previewUrl`: click opens a lightweight overlay with
  the full image, file name, and size.
- Image attachment without `previewUrl`: click opens the same overlay in
  detail mode with a clear unavailable state.
- Non-image attachment: click opens the same overlay in detail mode. It must
  not invent a path or switch to the Files tab unless a future model provides a
  real path.

Local file paths:

- Absolute paths and valid `file://` links in messages open the Files tab.
- Line numbers, when present, are preserved.

Tool file references:

- Tool-card file targets open the Files tab through the same route as message
  file paths.

Composer attachments:

- Draft attachments remain remove-oriented.
- Clicking a composer draft attachment must not open preview in this change.
  This avoids conflict with the remove button and keeps this design scoped to
  chat-visible content.

Unavailable targets:

- Failed or unsupported attachment opens show explicit text such as "Preview
  unavailable for this attachment."
- The UI must not silently ignore a valid click.
- File targets defer missing-file and permission failures to the existing
  FilesPane error UI.

## Data Flow

Upload and send:

1. `attachments.ts` creates `ComposerAttachmentDraft` values from
   `FileList | File[]`.
2. Image drafts get a `previewUrl` from `URL.createObjectURL(file)`.
3. Submit builds:
   - `attachments`: daemon-facing metadata;
   - `displayAttachments`: optimistic frontend display data with `previewUrl`
     when available.
4. The daemon receives only metadata and the formatted message text.

Render and open:

1. `ConversationView.svelte` renders message attachments with
   `AttachmentPreviewStrip`.
2. Clicking a message attachment emits an attachment intent.
3. Clicking a local path or tool target emits a file intent.
4. `AgentDetail.svelte` routes:
   - file -> update `fileToOpen` and switch to `files`;
   - attachment -> open the attachment overlay in preview or detail mode.

Lifecycle:

- The attachment overlay uses the existing object URL for image preview; it
  does not create a new one.
- Closing the overlay does not revoke the URL.
- Existing message cleanup remains responsible for
  `revokeTimelineAttachmentPreviews`.
- Pending submitted messages stored in `localStorage` continue stripping
  `previewUrl`.
- Reloaded transcripts do not gain attachment objects in this change. Existing
  pending optimistic messages may show attachment details after `previewUrl`
  has been stripped, but this design does not add backend attachment
  persistence.

## Error Handling

Attachment preview is allowed only when the attachment has a usable
`previewUrl`. Missing preview data is not an exception; it is a normal detail
state.

Files-pane open failures stay in the existing Files tab error path. The open
intent should not pre-read files, check daemon access, or duplicate FilesPane
error handling.

If an attachment intent is malformed, the route should fail closed with an
explanatory detail state instead of throwing during render. File intents should
be created only from already-validated local path parsing.

## Performance

The design avoids work proportional to transcript size beyond normal rendering.

Do not:

- read attachment file contents during render;
- add global attachment caches;
- persist or clone blob URLs;
- create preview object URLs on click;
- add observers for this behavior;
- generate previews for non-image attachments.

The only heavier UI is the attachment overlay, and it is rendered on demand.

## Accessibility

Clickable attachment cards should be real buttons or equivalent keyboard
targets with clear labels:

- image: `Open image attachment <name>`;
- file: `Open attachment details for <name>`.

The attachment overlay should support:

- close button;
- `Esc` close;
- focus containment or at least focus restoration to the clicked attachment;
- useful image alt text from the attachment name.

Attachment detail state should be readable by screen readers and should not
depend only on color.

## Testing

Add focused desktop UI coverage in `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
or a similarly scoped agent chat spec.

Cases:

1. Message image attachment with `previewUrl` opens an overlay with the image
   and file name; `Esc` closes it.
2. Message non-image attachment opens attachment detail and does not switch to
   the Files tab.
3. A restored pending message attachment without `previewUrl` opens the
   unavailable detail state.
4. Message body local path still opens the Files tab and preserves line number.
5. Tool-card file reference opens through the same Files tab path.
6. Composer draft attachment remove behavior remains unchanged.

The tests should not depend on native Tauri path-drop behavior.

## Long-Term Follow-Up

A later design can introduce durable attachment resources if Puffer needs
reloaded transcripts to preview uploaded files. That should be separate because
it affects daemon storage, session persistence, security boundaries, and
possibly upload limits.

This design intentionally stops at a stable frontend intent boundary.
