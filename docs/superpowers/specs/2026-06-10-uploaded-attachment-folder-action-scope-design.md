# Uploaded Attachment Folder Action Scope Design

Date: 2026-06-10

## Problem

Uploaded chat attachments currently surface the same overlay folder action as
generated media when their source is `local_file`. For uploaded attachments this
path is Puffer's staged copy under the session store, not the user's original
folder. Opening it is technically consistent with the stored path but misleading
in the product: users expect either their source folder or no folder action, not
an internal `.puffer/sessions/...attachments/<id>/` directory.

Earlier design work considered storing and reopening the user's original source
folder. A code review showed both upload entry points currently use browser
`File` objects:

- the Add images and files picker is a hidden `<input type="file">`;
- drag/drop reads `DragEvent.dataTransfer.files`;
- both paths create composer drafts from `File`, which does not reliably expose
  an absolute source path.

Tauri and Electron-style desktop apps can solve this by using native file
dialogs and native drag/drop path events, but that would replace the upload
entry points and staging flow. The benefit is not worth the added surface for
this overlay action.

## Decision

Do not show a folder action for user-uploaded chat attachments.

Keep the folder action only for Puffer-owned generated media when a stable
`generated_media.localPath` exists:

- generated images: open the generated artifact's containing folder;
- generated videos: open the generated artifact's containing folder.

Remote image attachments keep the download action. Remote files/videos and
uploaded files/images/videos have no overlay action beyond close.

This supersedes `2026-06-10-uploaded-attachment-original-folder-design.md` for
the uploaded-attachment overlay action. The original-folder path-aware upload
model remains a possible future feature, but it is not part of this fix.

This is intentionally a frontend action-policy change. It should not change
attachment staging, transcript persistence, DTO source variants, preview reads,
or native file-opening commands.

## Behavior

| Attachment source | Action |
| --- | --- |
| `generated_media` with `localPath` | Open containing folder |
| `generated_media` without `localPath` | none |
| `remote_url` image | Download image |
| `remote_url` file/video | none |
| uploaded/staged `local_file` image/file/video | none |

The action resolver should be source-driven, not preview-driven. `previewUrl`
may be a blob URL, a remote URL, or absent; it should not decide whether a
folder action appears.

## Architecture

Keep the current attachment data model. Do not add `originalPath`, native path
drafts, upload bookmarks, or path-scoped permissions.

The only functional rule change is in the frontend overlay action resolver:

```ts
case "local_file":
  return null;
case "generated_media":
  return attachment.source.localPath
    ? { kind: "open_folder", path: attachment.source.localPath }
    : null;
case "remote_url":
  return attachment.kind === "image"
    ? { kind: "download", url: attachment.source.url, suggestedName: attachment.name }
    : null;
```

The existing Tauri `open_containing_folder` command remains useful for generated
media and should keep its validation: absolute path, existing file,
canonicalize, open parent directory.

Do not remove or rename `openContainingFolder` or the Tauri
`open_containing_folder` command. The command is still the right bridge for
generated image/video artifacts. The problem is only that uploaded
`local_file.path` points at an internal staged copy, so uploaded attachments
should not request this bridge.

Preview loading is unchanged. Uploaded `local_file` images still use
`readChatAttachmentPreview`/`readMessageAttachmentPreview` for thumbnails and
overlay previews. Hiding the folder icon must not make uploaded image previews
unavailable.

## UX

Uploaded attachment overlays still show metadata, preview when available, and
the close button. They do not show a disabled or hidden-explanation folder
button. Removing the affordance is clearer than opening an internal copy or
showing a persistent warning for normal uploads.

Generated media overlays keep the folder icon. For those artifacts the app owns
the output location, so the action is stable and expected.

The visible label, icon, busy state, and error handling for generated-media
folder opens remain unchanged. This fix should not introduce a new menu,
secondary fallback button, tooltip explanation, or special uploaded-attachment
warning.

## Testing

Update frontend unit tests for the action resolver:

- uploaded/local `local_file` image returns `null`;
- uploaded/local `local_file` non-image file returns `null`;
- generated image with `localPath` returns `open_folder`;
- generated video with `localPath` returns `open_folder`;
- generated media without `localPath` returns `null`;
- remote image returns `download`;
- remote file/video returns `null`.

Update UI coverage so an uploaded attachment overlay contains no folder action,
while generated media overlays still show it.

The existing Playwright fixture named around "local image source" should be
reframed as an uploaded/staged local source and assert that no folder action is
shown. A separate positive generated-media fixture should assert the folder
action remains visible for `generated_media.localPath`.

Rust command tests do not need changes because the native folder opener still
exists and its validation behavior is unchanged.

## Non-Goals

- No native file picker rewrite.
- No Tauri drag/drop path migration.
- No original source path persistence.
- No session metadata migration.
- No staged-copy fallback button.
- No generic download action for remote non-image files.
- No `open_containing_folder` command rename/removal.
- No label/icon redesign.
