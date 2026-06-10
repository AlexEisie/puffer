# Generated Image Placement And Local Links Design

## Summary

Assistant messages that announce generated images should show the text and
local file links first, then show the generated image thumbnails below that
text. Local file paths in the text should stay clickable and should open in the
desktop Files tab.

This is a narrow `puffer-desktop` rendering change. It does not change media
storage, timeline DTOs, generated preview RPCs, or CLI/TUI rendering.

## Recheck Outcome

The earlier draft was too broad because it proposed rendering all user and
assistant attachment strips after message text. That risks changing uploaded
attachment UX for no benefit.

The tightened design only changes ordering for assistant messages whose
attachments include `source.kind === "generated_media"`. This matches the user
case: ImageGeneration text such as "created these files: /path/image.jpeg"
should remain visible above the generated thumbnails.

## Goals

- Put generated image thumbnails below the assistant text that references them.
- Keep local absolute paths and `file://` links clickable through the existing
  `ChatOpenIntent` route.
- Preserve generated thumbnail preview behavior and attachment overlay behavior.
- Preserve existing user-upload attachment ordering.
- Keep the implementation in frontend rendering and tests unless a focused test
  proves an existing local-link parser bug.

## Non-Goals

- Do not change transcript, session-store, Tauri, daemon, or generated-media
  DTO contracts.
- Do not add a gallery, artifact browser, retry UI, provider-specific image
  card, or Finder/open-external action.
- Do not scan tool output or arbitrary text to create generated attachments.
- Do not expose absolute paths that are only present in structured tool output.
- Do not mount external directories in the Files tree.
- Do not add thumbnail caches, image resizing, or preview persistence.
- Do not change CLI or TUI behavior.

## Architecture

Keep the current responsibility split:

- `MessageBody.svelte` parses Markdown links, `file://` links, and bare
  absolute local paths, then emits `file` open intents.
- `MessageAttachmentPreviewStrip.svelte` loads preview bytes for generated
  media and renders thumbnail buttons.
- `AttachmentPreviewStrip.svelte` renders the thumbnail/file-card UI.
- `AgentDetail.svelte` routes `file` intents to the Files tab and attachment
  intents to the preview overlay.
- `ConversationView.svelte` owns message layout order.

Only `ConversationView.svelte` needs a new ordering rule:

```text
if assistant message has any generated_media attachments:
  render visible message body
  render attachment preview strip
else:
  keep current attachment/body order
```

Empty-body generated image messages still render their thumbnails. The existing
`visibleMessageBody` suppression remains in place so synthetic
`[Image: Generated image]` attachment summary text does not duplicate the
thumbnail strip.

## Local File Behavior

Local links should keep using the existing `chatFileTarget` helper and
`ChatOpenIntent` file route. The target opens in the Files tab with the
requested path and optional line number.

For paths outside the current session folder but inside an allowed backend
root, Files should open the file as a tab even though the left tree remains
rooted at the session workspace. This avoids adding a second tree root or a
global media browser for a link-click use case.

If a local path is missing or outside allowed roots, the existing Files tab
error state is sufficient.

## Generated Image Behavior

Clicking a generated image thumbnail opens the existing attachment preview
overlay. Clicking text that contains the generated file path opens the Files
tab. The two actions remain distinct.

The desktop should not synthesize path text from `ImageGeneration` structured
tool output. Paths appear in chat only when the assistant message text already
contains them.

## Testing

Add one focused Playwright regression for an assistant message that contains
two generated image paths and two generated image attachments:

- verify both local paths render as links;
- verify the first path/link appears before the first image thumbnail in DOM
  order;
- click a path and verify `read_file` receives that path through the Files tab;
- return to Chat, click a thumbnail, and verify the attachment preview overlay
  opens.

Keep existing tests for:

- generated `/image` success hiding structured output paths;
- persisted generated attachments rendering as thumbnails;
- missing generated previews rendering unavailable thumbnails;
- normal message and tool-card local file links.

## Scope Guard

Stop and re-evaluate if implementation starts requiring any of these:

- backend allowed-root changes;
- new generated-media DTO fields;
- new attachment source variants;
- parsing `ImageGeneration` output in the frontend;
- a media gallery, file reveal command, or Finder integration;
- reordering all attachment strips across all message types.
