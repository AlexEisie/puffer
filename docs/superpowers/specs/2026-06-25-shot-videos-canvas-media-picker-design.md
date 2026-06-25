# Shot videos canvas — video mediaPicker design

Date: 2026-06-25
Branch: `feat/concurrent-tool-capability-bridge`
Status: approved (design)

## Problem

Short-drama Stage 4 ("Per-shot video") gates the generated clips with a
read-only status `table` + a `multiSelect` (`retry`) that re-runs `videogen`
for selected shots. It shows no preview of the actual clips, and the retry
mechanism is the only review affordance.

We want Stage 4 to match the Stage 3 ("Character image") experience: a grid of
**video thumbnails** the user can check/uncheck, where clicking a thumbnail
opens a preview popup that **plays** the shot clip. The retry mechanism is
removed.

Constraints (from the user): no backward-compatibility burden; optimize for
long-term value, stability, and performance; avoid over-engineering.

## Goals

- Remove the Stage 4 retry mechanism (the status `table` + `retry` multiSelect).
- Stage 4 mirrors Stage 3: a `mediaPicker` grid (checkbox + thumbnail + label),
  clicking a thumbnail opens a preview popup.
- Thumbnails show the shot video's first frame; the popup plays the clip with
  controls.
- Works for **every** video provider, including local-only ones (e.g. BytePlus)
  that do not return a remote URL — the requirement (thumbnail + click-to-play)
  must hold regardless of provider.

## Non-goals

- No retry/regeneration UI inside Stage 4. Redo follows the Stage 3 pattern:
  re-run `videogen` and re-render the canvas.
- No new poster-extraction work: the browser's native `<video>` first-frame is
  the thumbnail. (The existing `video_poster` artifact pipeline is left
  untouched and unused here — see Alternatives.)
- No change to the canvas backend schema.

## Key decisions (resolved during brainstorming)

1. **Checkbox semantics** — checked = keep this shot for Stage 5 composition;
   unchecked = drop it. Mirrors Stage 3 exactly. This subsumes the old retry
   intent (deselect a bad clip rather than retry it). Stage 5 then only does
   order + mux over the kept clips.
2. **Media-type discrimination** — explicit per-item `kind: "image" | "video"`
   field (default `"image"`), not URL/extension sniffing. Robust for
   extensionless URLs; semantically explicit; long-term stable.
3. **Video source** — video items carry a workspace-relative `path`; the
   frontend mints a streaming URL via the existing `createFileMediaAccess(path)`
   primitive (HTTP Range support). This is provider-independent and is the same
   path the app already uses to play in-workspace videos. (Chosen over relying
   on `remoteSourceUrl`, which silently fails for local-only providers.) Every
   succeeded shot has a local `path` regardless of provider — `complete_video_job`
   always downloads the clip to disk — so `path` is the universally-reliable key
   and `remoteSourceUrl` is irrelevant to Stage 4.

`create_file_media_access` is documented (`daemon.rs:1019`) as serving "arbitrary
in-workspace video files … validates the path before inserting, so a ticket never
escapes the workspace," with Range support (`PARTIAL_CONTENT`, tested at
`daemon.rs:8363`). Clips live under cwd `.puffer/media/videos/`, so this is the
intended, already-tested mechanism — no backend change and no new feasibility risk.

## Architecture

Three layers change; the canvas backend does not.

### 1. Canvas data contract — `mediaPicker` item gains `kind` + `path`

`mediaPicker` item shape becomes:

```
{ id, label?, description?, kind?: "image" | "video",
  url?,    // image items: directly-loadable URL (e.g. remoteSourceUrl)
  path? }  // video items: workspace-relative path to the clip
```

- `kind` defaults to `"image"` → Stage 3 character-image items are unchanged
  (they keep using `url`).
- The backend `canvas.rs` is unchanged: `initial_canvas_values` only branches on
  `multi` for `mediaPicker` (`canvas.rs:141`); all item fields are passed through
  to the frontend verbatim. `kind`/`path` are a pure frontend concern.

`resources/canvas/components.md` — update the `mediaPicker` entry to document
`kind`, that video items use `path`, that the thumbnail is the first frame, and
that the preview popup plays the clip.

### 2. Frontend rendering — `InlineCanvasNode.svelte` + preview popup

`InlineCanvasNode.svelte` `mediaPicker` block (currently lines 509–534):

- **Access-URL resolution.** On mount, for each item with `kind === "video"`,
  call `createFileMediaAccess(item.path)` once and store the resulting streaming
  URL in an `id → url` map (Svelte state). Resolve once, not per render, to avoid
  duplicate RPCs. Handle the `state !== "available"` / error case by leaving the
  cell in a clearly-disabled "preview unavailable" state (do not crash the node).
- **Thumbnail cell.** For `kind === "video"`: render
  `<video src={accessUrl} muted preload="metadata" playsinline>` (Range +
  `metadata` preload fetches only enough for the first frame — the performance
  lever). Otherwise render the existing `<img src={item.url}>`. The checkbox,
  selected state, and label markup are unchanged. The `<video>` elements need the
  same `<!-- svelte-ignore a11y_media_has_caption -->` the app already uses for
  caption-less generated videos (`AttachmentOverlay.svelte:163`).
- **Preview popup.** Generalize `CanvasImagePreview.svelte` → rename to
  `CanvasMediaPreview.svelte`, taking a `kind` prop. `kind === "video"` renders
  `<video src={accessUrl} controls autoplay>` (reusing the playback pattern
  already proven in `AttachmentOverlay.svelte`); otherwise the existing `<img>`.
  The modal chrome (backdrop, close button, Esc handler, name, description) is
  unchanged. Update the single import site in `InlineCanvasNode.svelte`.

### 3. Skill — short-drama Stage 4 rewrite

`resources/skills/short-drama-generation/SKILL.md` Stage 4:

- **Remove** the read-only status `table` + `multiSelect(retry)` gate entirely.
- After running each shot's `videogen`, render a `mediaPicker` (`multi:true`)
  mirroring Stage 3, one item per **succeeded** shot:
  `{ id: <shotId>, kind: "video", path: <clip workspace path>, label: <shotId>,
     description: <shot prompt summary> }`, with `value` = every succeeded item id
  (all checked by default).
- End the turn. On read-back, the checked ids are the shots kept for composition;
  unchecked shots are dropped. There is no retry — to redo a shot, re-run
  `videogen` and re-render this canvas (same wording as Stage 3's redo note).
- Stage 5's `editableTable` rows are seeded from the **kept** clips (Stage 4
  selection), not "all succeeded clips".

## Data flow (desktop, happy path)

```
videogen → clip saved at .puffer/media/videos/<aid>/...  (manifest records path)
skill → Canvas mediaPicker item { kind:"video", path }
InlineCanvasNode mount → createFileMediaAccess(path) → http://…/<ticket> (Range)
  thumbnail  <video preload=metadata>  → first frame
  click → CanvasMediaPreview <video controls autoplay> → plays clip
user checks/unchecks → submit → CanvasState read-back → kept shotIds → Stage 5
```

## Error handling

- `createFileMediaAccess` returns non-available / throws → cell shows a disabled
  "preview unavailable" state; the item is still selectable so composition is not
  blocked by a preview failure. Never crash the canvas node.
- Non-desktop environment (no daemon / no Tauri) → `createFileMediaAccess` is
  unavailable; Stage 4 video preview degrades. The short-drama skill already
  specifies a text-based degrade path for non-desktop; the canvas gate is a
  desktop-inline feature.
- A shot whose `videogen` failed produces no clip → it is simply not added as an
  item; the skill reports failed shots plainly in turn text (existing failure
  contract), it does not fabricate a tile.
- Access-ticket TTL: `createFileMediaAccess` returns an `expiresAtMs` ticket. For
  a short review gate this is a non-issue; if the canvas sits open past the TTL a
  later play may 404. We deliberately do **not** build re-minting (YAGNI) — note
  it as a known limit. If it bites in practice, lazily re-resolve on preview-open.

## Testing

- **Frontend (Vitest):** mediaPicker renders `<video>` for `kind:"video"` and
  `<img>` for image/absent kind; video access URL is resolved once per item;
  unavailable access URL yields the disabled state without throwing; clicking a
  video thumbnail mounts `CanvasMediaPreview` in video mode. Mock
  `createFileMediaAccess`.
- **Backend (Rust):** add a `canvas.rs` initial-values assertion that a
  `mediaPicker` carrying `kind`/`path` items still produces the correct default
  `value` (empty array for `multi:true`) — i.e. the new fields are inert to the
  backend.
- **Skill:** no Rust test asserts Stage 4 prompt text (verified via grep); the
  SKILL.md change is doc-only and validated by reading.

## Verification (done during design)

- `create_file_media_access` allowed-roots cover workspace media — confirmed via
  the daemon handler comment and Range tests (`daemon.rs:1019`, `:8363`). No
  open feasibility risk remains; nothing left to verify before wiring.

## Alternatives considered

- **`remoteSourceUrl` as the video source (simplest).** Reuses the generic
  `url`-only mediaPicker with no new RPC. Rejected: silently fails for
  local-only providers (BytePlus), breaking the core requirement.
- **Artifact poster via `read_generated_media_preview` (artifactId).** Reuses
  the existing poster-extraction pipeline for cheap JPEG thumbnails. Rejected as
  the primary mechanism: couples the generic mediaPicker to the
  session/artifact media subsystem and is heavier to wire than the path-based
  `createFileMediaAccess`, for no thumbnail-quality benefit at this scale (a
  handful of shots, `preload=metadata` is light). The poster pipeline is left
  intact and may be adopted later purely as a perf optimization if shot counts
  grow.
- **Separate `videoPicker` node type.** Rejected: doubles component surface for
  what is one rendering branch; `kind` keeps a single, data-driven picker.
```
