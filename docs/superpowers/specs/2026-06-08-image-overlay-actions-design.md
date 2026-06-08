# Image Overlay Actions Design

## Goal

Improve the desktop image preview overlay with one contextual action next to
the close button:

- local image: open the containing folder
- URL image: download the image

The design optimizes for long-term stability, performance, and a small UI
surface. Backward compatibility with the old attachment source shape is out of
scope.

## Current Context

`AttachmentOverlay.svelte` currently renders attachment metadata, a preview
image when `attachment.kind === "image"` and `attachment.previewUrl` exists,
and a close button. The overlay receives `MessageAttachment`, whose source is
currently too coarse to distinguish durable local files from remote URL images.

Generated image previews and persisted chat attachment previews often render
through `blob:` URLs, so the overlay must not infer user actions from
`previewUrl`.

## Chosen Approach

Use an explicit attachment source model and keep the overlay action as a
single icon-only button immediately left of the close button.

```ts
type AttachmentPreviewSource =
  | { kind: "local_file"; path: string }
  | { kind: "remote_url"; url: string; suggestedName?: string }
  | { kind: "generated_media"; jobId: string; artifactId: string; index: number; localPath?: string };
```

Action resolution is a pure frontend helper:

- `local_file` image attachments return an `open_folder` action.
- `remote_url` image attachments return a `download` action.
- `generated_media` image attachments return `open_folder` only when
  `localPath` is present.
- non-image attachments return no overlay action.

This keeps behavior tied to durable metadata rather than transient preview
URLs.

## UI/UX

The overlay header stays compact:

- Left side: filename and metadata, with truncation for long names.
- Right side: action group.
- The contextual action button sits immediately left of the close button.
- Local image uses a folder icon with `aria-label="Open image folder"` and
  title `"Open image folder"`.
- URL image uses a download icon with `aria-label="Download image"` and title
  `"Download image"`.
- Close remains the rightmost X and keeps the current focus behavior.

While an action is running, its button is disabled. Failures render as one
short inline status under the header metadata. Successful folder opens do not
leave persistent status. Successful downloads may show a short saved-path
status if the native command returns a path.

There is no overflow menu, progress panel, toast system, or custom save picker.

## Native/API Surface

Add two narrow Tauri commands:

```text
open_image_containing_folder(path)
download_image_from_url(url, suggestedName?)
```

`open_image_containing_folder` validates that `path` is absolute, derives the
parent directory, and opens that directory via `tauri_plugin_opener`.

`download_image_from_url` accepts only `http` and `https`, downloads in Rust,
rejects non-image content types when available, writes to the user's Downloads
directory using a sanitized filename, and returns the saved path. It writes to
a temp file first and then atomically renames so partial downloads are not
presented as final images.

Frontend wrappers live in `desktop.ts`. `AttachmentOverlay.svelte` calls the
pure action resolver and then invokes the corresponding wrapper.

## Migration

Because backward compatibility is intentionally out of scope, message
attachment creation should emit the new explicit source kinds directly. The
old `user_upload` source should be removed from frontend examples and tests.

Generated media should include `localPath` when the backend knows the saved
artifact path. If `localPath` is absent, generated media keeps preview behavior
but shows no extra overlay action.

## Testing

Unit coverage:

- `imageOverlayAction(local_file image)` returns folder action.
- `imageOverlayAction(remote_url image)` returns download action.
- `imageOverlayAction(generated_media image with localPath)` returns folder
  action.
- `imageOverlayAction(generated_media image without localPath)` returns no
  action.
- non-image attachments return no action.

Type/API coverage:

- Update message attachment source examples to the explicit source model.
- Verify `readMessageAttachmentPreview` still routes generated media previews
  by artifact id.

UI coverage:

- Local image overlay shows the folder icon immediately left of close.
- URL image overlay shows the download icon immediately left of close.
- Escape and close still close the overlay and restore focus.

Rust coverage:

- Absolute path validation and parent directory derivation.
- URL scheme validation.
- Download filename sanitization.
- Response validation and target-path derivation without real network access.

## Out of Scope

- Download progress.
- Download queue or history.
- Custom save location picker.
- Opening the image file itself.
- Multi-action menu.
- Compatibility adapter for old attachment source shapes.
