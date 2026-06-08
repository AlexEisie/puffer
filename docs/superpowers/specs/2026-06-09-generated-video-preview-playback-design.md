# Generated Video Preview Playback Design

Date: 2026-06-09

## Summary

Generated videos should render as normal assistant-side attachments in desktop
chat, show a first-frame thumbnail in the attachment strip, and play inside the
existing attachment overlay.

The implementation should use a short-lived local media URL served by the
daemon, not WebSocket/RPC video bytes. This keeps video playback stable for
larger files, avoids large Svelte state objects, and preserves the existing
metadata-first generated media model.

## Context

Milhous session `c6ad17e3-7444-48a5-ba32-0c92bc89788b` contains a successful
`VideoGeneration` result:

- artifact id: `9e1ce118-90bc-481b-a215-b2d904a590b1`
- MIME type: `video/mp4`
- size: `2989935`
- local file: `.puffer/media/artifacts/<artifact_id>/byteplus-video-...mp4`

The daemon timeline already synthesizes generated video attachments with
`kind = "video"`, `Generated video` names, and `GeneratedMedia` sources. The
remaining gap is preview and playback. Current frontend attachment types and
overlay rendering are image-biased, and the generated media preview RPC only
returns image bytes.

## Goals

- Show generated video attachments in chat history and live `/video` results.
- Display a first-frame thumbnail or native video metadata frame on the card.
- Play the generated video in the existing attachment overlay.
- Avoid sending video bytes through WebSocket/RPC JSON.
- Support HTTP range requests so browser video playback can seek and load
  efficiently.
- Keep generated media access limited to trusted artifact ids and session
  context.
- Preserve the metadata-first transcript and timeline contract.

## Recheck Outcome

The first pass was intentionally small, but a few details needed tightening:

- Use video-specific names and contracts. The target RPC is
  `create_generated_video_access`, not a generic generated media access API.
- Use `/media/generated-video/<ticket>` for the HTTP route. This avoids implying
  a general file or image serving surface.
- Keep image previews on the existing byte-preview path. Do not migrate images
  to ticket URLs in this change.
- Do not add revocation APIs, background cleanup tasks, `HEAD` handling, CORS
  policy changes, canvas thumbnail extraction, or visibility scheduling.
- Do add precise range semantics, opportunistic expired-ticket pruning, and
  blob URL cleanup rules so the narrow solution remains stable.
- Return a ticket path from the daemon RPC, not a full localhost URL. The
  frontend must build the media URL from the active daemon handshake so local,
  browser-preview, and SSH-forwarded daemon connections use the same origin the
  client is already connected to.

## Non-Goals

- No backward-compatible DTO preservation.
- No generic local file server.
- No media gallery, download manager, global media library, or cross-session
  media index.
- No ffmpeg dependency or persisted poster thumbnail sidecar in this change.
- No arbitrary path previewing from transcript text.
- No image-to-video, reference-frame, video-edit, or provider-specific playback
  UI.
- No video bytes in transcript events or long-lived frontend state.
- No ticket revocation endpoint or background garbage-collection task.
- No CORS headers or CSP changes unless a real browser test proves they are
  required. The first version does not read video pixels through canvas.

## Chosen Approach

Add a narrow generated video access layer to the daemon.

The frontend requests playback access by `sessionId` and `artifactId`. The
daemon validates the session, artifact sidecar, media kind, MIME type, and
canonical file path, then returns a short-lived ticket path. The frontend
converts the active daemon WebSocket handshake origin to HTTP and uses that URL
directly in `<video>` elements for thumbnail display and overlay playback.

This deliberately keeps images on the current preview-byte path and uses the
new URL path only where streaming matters: generated videos.

## Daemon Contract

Add an RPC method:

```json
{
  "method": "create_generated_video_access",
  "params": {
    "sessionId": "<session id>",
    "artifactId": "<artifact id>"
  }
}
```

Successful response:

```json
{
  "state": "available",
  "path": "/media/generated-video/<ticket>",
  "mimeType": "video/mp4",
  "size": 2989935,
  "expiresAtMs": 1780940000000
}
```

Failure response:

```json
{ "state": "missing" }
```

or:

```json
{ "state": "unsupported" }
```

The method is session-aware. It must resolve the session cwd before loading the
artifact, rather than assuming the daemon process cwd.

The daemon response does not include host, port, scheme, or the primary daemon
auth token. The frontend derives the final URL from the current `DaemonClient`
handshake:

- `ws://host:port/ws` becomes `http://host:port/media/generated-video/<ticket>`;
- `wss://host/ws` becomes `https://host/media/generated-video/<ticket>`.

This keeps SSH-forwarded and browser-preview connections working without the
remote daemon guessing the client's local forwarding address.

## Media Ticket Rules

The ticket is a random, unguessable token stored in daemon memory. It binds to:

- canonical file path;
- MIME type;
- byte size;
- artifact id;
- expiry timestamp.

Rules:

- Tickets expire after a short TTL, for example 5 to 10 minutes.
- Tickets are single-file capabilities, not reusable auth credentials.
- The daemon token must not be embedded in media URLs.
- Unknown or expired tickets fail closed.
- Expired tickets are pruned opportunistically when creating a new ticket or
  serving a media request.
- No persistent ticket storage is needed.
- No explicit revoke endpoint is needed for the first implementation.

## HTTP Media Route

Add a daemon HTTP route:

```text
GET /media/generated-video/<ticket>
```

The route must:

- serve only active ticket targets;
- set the artifact MIME type as `Content-Type`;
- support full `GET`;
- support single-range `Range` values in these forms:
  `bytes=start-end`, `bytes=start-`, and `bytes=-suffix_length`;
- return `206 Partial Content` with `Content-Range` for valid ranges;
- return `416 Range Not Satisfiable` for syntactically valid but unsatisfiable
  ranges;
- return a closed failure for invalid, expired, or unknown tickets;
- avoid directory listings, path parameters, and arbitrary local paths.

Multiple ranges and `HEAD` are not required.

## Path And MIME Validation

The daemon must load the artifact sidecar and validate that:

- artifact id syntax is safe;
- artifact kind is `video`;
- MIME starts with an allowed video MIME, initially `video/mp4` and
  `video/webm`;
- canonical file path exists and is a regular file;
- canonical file path is under
  `<session cwd>/.puffer/media/artifacts/<artifact_id>/`;
- symlink escapes are rejected after canonicalization.

`missing` is for a valid artifact whose sidecar or file cannot be found.
`unsupported` is for wrong kind, unsafe path, unsupported MIME, invalid
artifact identity, or provenance mismatch.

## Frontend Types And API

Update desktop frontend attachment types:

```ts
type AgentTurnAttachmentKind = "image" | "file" | "video";
```

Add a generated media access result type:

```ts
type GeneratedVideoAccessResult =
  | { state: "available"; url: string; mimeType: string; size: number; expiresAtMs: number }
  | { state: "missing" }
  | { state: "unsupported" };
```

Add `createGeneratedVideoAccess(sessionId, artifactId)` in the desktop API
layer. It should be used only for generated video attachments. Local file
attachments and generated images keep their existing preview behavior.

The desktop API function should call `create_generated_video_access`, receive
the daemon `path`, and return a frontend-facing `url` built from the active
daemon handshake. The UI should never concatenate hostnames itself.

`MessageAttachment.previewUrl` may continue to carry the transient display URL
for both image and video attachments. URL cleanup helpers must only call
`URL.revokeObjectURL` for `blob:` URLs; daemon media URLs expire server-side and
do not need object URL revocation.

The generated video access path is daemon-owned. The Tauri shell already
acquires a WebSocket daemon handshake through `ensure_local_daemon`, so the
frontend should use the same daemon RPC in browser preview and packaged desktop
contexts instead of duplicating this route in the Tauri backend.

## Attachment Strip Behavior

`MessageAttachmentPreviewStrip` should request a media access URL when all of
the following are true:

- attachment kind is `video`;
- source kind is `generated_media`;
- session id is present;
- attachment state is not `missing`.

The strip should render:

- a video thumbnail card when access is available;
- a play icon overlay;
- a file/video fallback card when access is missing or unsupported.

The thumbnail should use:

```html
<video preload="metadata" muted playsinline src="..."></video>
```

No canvas extraction is needed for the first version. Browser-native metadata
loading is enough for generated MP4 artifacts and avoids a thumbnail pipeline.

## Overlay Behavior

The existing `AttachmentOverlay` remains the single attachment detail surface.

Behavior:

- image attachments continue to render `<img>`.
- video attachments with a media URL render
  `<video controls autoplay playsinline src="...">`.
- video attachments without a media URL render the existing unavailable state.
- generated media folder actions can use the existing local path metadata, but
  the helper should become media-oriented rather than image-only.

The overlay should be able to request a fresh media URL if the strip did not
already have one or if a prior URL expired.

If the overlay receives an attachment without a video `previewUrl`, it should
request `createGeneratedVideoAccess` itself. If the video element fails to load
with an existing URL, it may retry once with a fresh URL, then fall back to the
unavailable state.

## Live `/video` Result Behavior

When `/video` generation succeeds and returns artifacts, the frontend should
append a live assistant item with generated video attachments instead of only a
status string.

The live attachment should use the same shape as persisted timeline video
attachments:

- id: `generated-video:<artifactId>`;
- name: `Generated video`;
- kind: `video`;
- MIME and size from the artifact result;
- source: `generated_media` with job id, artifact id, index, and local path.

This keeps live and reloaded sessions visually consistent.

## Error Handling

- Missing sidecar or file: show unavailable/fallback card.
- Unsupported MIME or non-video artifact: show unavailable/fallback card.
- Path validation failure: show unavailable/fallback card and do not expose the
  local path in primary chat text.
- Expired ticket: request a fresh ticket once when opening the overlay.
- Video element load error: keep the card visible and show the fallback
  treatment for that attachment.
- Invalid or unsatisfiable range: return HTTP `416`; the UI should rely on the
  native video element error path and not parse this status directly.

Errors should not leak absolute file paths into chat UI copy.

## Performance

- Video data must not travel through RPC JSON.
- The frontend should store only short strings and metadata for video media
  access, not byte arrays.
- Thumbnail videos use `preload="metadata"` to avoid eager full downloads.
- HTTP range support is required for responsive MP4 startup and seek behavior.
- No IntersectionObserver is required for the first implementation. Generated
  video counts are currently low, and adding visibility scheduling now would be
  premature.
- Ticket storage is an in-memory `HashMap`; prune expired entries
  opportunistically instead of adding a timer.

## Stability And Security

The media route is only a ticket-backed artifact route. It is not a filesystem
route.

Security properties:

- no arbitrary path parameter;
- no daemon auth token in DOM media URLs;
- no cross-session artifact lookup without session validation;
- no symlink escape;
- no directory traversal;
- no persistent public URLs.
- no frontend access to the daemon's primary auth token through media URLs.

If validation is ambiguous, fail closed as `unsupported`.

## Testing

Rust tests:

- valid video artifact creates an available media access result;
- non-video artifact returns `unsupported`;
- unsupported video MIME returns `unsupported`;
- missing sidecar or missing file returns `missing`;
- symlink escape returns `unsupported`;
- full `GET` returns video MIME and bytes;
- valid single range returns `206` and `Content-Range`;
- open-ended and suffix ranges return the expected byte slices;
- unsatisfiable range returns `416`;
- invalid, expired, and unknown tickets fail closed.

Frontend unit tests:

- `MessageAttachment` accepts `kind: "video"`;
- generated video access API calls the expected daemon method;
- video attachments choose media access instead of byte preview;
- URL cleanup revokes `blob:` URLs but does not revoke daemon HTTP URLs;
- image attachment preview behavior is unchanged.

Playwright tests:

- seeded generated video attachment renders a video card with play affordance;
- clicking the card opens an overlay with `<video controls>`;
- missing or unsupported access renders fallback without local path text;
- `/video` live success appends a `Generated video` attachment;
- session switch removes transient media URL state from the visible chat.

## Update Specs

Implementation should add concise component specs after code changes:

- next unused `specs/puffer-cli/NN.md` for daemon media ticket and HTTP range
  serving;
- next unused `specs/puffer-desktop/NN.md` for generated video cards and
  overlay playback.

These specs should describe the final behavior, not this design process.
