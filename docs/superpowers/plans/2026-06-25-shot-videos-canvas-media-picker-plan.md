# Execution plan — Shot videos canvas video mediaPicker

Spec: `2026-06-25-shot-videos-canvas-media-picker-design.md`
Branch: `feat/concurrent-tool-capability-bridge`

TDD throughout: write the failing test first, then the minimal code to pass, then
refactor. Each phase ends green and is committed separately.

## Phase 1 — Preview popup: generalize to media (frontend)

Rename `CanvasImagePreview.svelte` → `CanvasMediaPreview.svelte`, add a `kind`
prop, branch image vs video. Smallest isolated unit, no dependency on the picker.

1. Add `apps/puffer-desktop/src/lib/screens/agent/CanvasMediaPreview.test.ts`
   (Vitest + @testing-library/svelte):
   - `kind:"image"` (or omitted) renders an `<img>` with the given `url`/`name`.
   - `kind:"video"` renders a `<video>` with `controls` and the given `url`.
   - Esc / backdrop click / close button each call `onClose`.
2. `git mv CanvasImagePreview.svelte CanvasMediaPreview.svelte`; add
   `kind?: "image" | "video"` to `Props`; in the template branch the media
   element (`<video controls autoplay>` vs `<img>`). Keep all modal chrome,
   `aria-label`, and styles; add the `<!-- svelte-ignore a11y_media_has_caption -->`
   on the `<video>`. Update the import + usage in `InlineCanvasNode.svelte`.
3. Run the desktop unit suite; green. Commit:
   `refactor(canvas): generalize CanvasImagePreview to CanvasMediaPreview (image|video)`.

## Phase 2 — mediaPicker renders video items (frontend)

Make the `mediaPicker` block in `InlineCanvasNode.svelte` (lines ~509–534)
media-type aware and resolve video access URLs.

1. Extend the existing InlineCanvasNode test (or add
   `InlineCanvasNode.mediaPicker.test.ts`), mocking `createFileMediaAccess`:
   - An item with `kind:"video"` + `path` renders a `<video>` whose `src` is the
     resolved access URL; `createFileMediaAccess` is called once per video item.
   - An item with no `kind` (or `kind:"image"`) still renders `<img src=url>` and
     does NOT call `createFileMediaAccess` (Stage 3 unchanged).
   - When `createFileMediaAccess` resolves non-available/throws, the cell renders
     a disabled "preview unavailable" state and the node does not throw.
   - Clicking a video thumbnail mounts `CanvasMediaPreview` with `kind:"video"`.
2. Implement:
   - Add `videoUrls = $state(new Map<string,string>())`. In an `$effect` keyed on
     the picker items, for each `kind==="video"` item call
     `createFileMediaAccess(item.path)` once; on `state==="available"` store
     `id → url`; on failure store a sentinel/leave unset. Guard against
     re-resolving an id already present.
   - Thumbnail: `{#if item.kind === "video"}` → `<video src={videoUrls.get(id)}
     muted preload="metadata" playsinline>` (+ a11y-ignore) and a disabled state
     when the url is missing; `{:else}` keep `<img>`.
   - Preview: pass `kind={previewItem.kind}` and the resolved video url (or
     `item.url` for images) into `CanvasMediaPreview`.
   - Import `createFileMediaAccess` from `../../api/desktop`.
3. Desktop suite green. Commit:
   `feat(canvas): mediaPicker renders video thumbnails with click-to-play preview`.

## Phase 3 — Backend inertness assertion (Rust)

Confirm the new item fields don't perturb canvas state defaults — no production
code change expected.

1. Add a `canvas.rs` test: a `mediaPicker` with `multi:true` whose items carry
   `kind`/`path` still yields initial `value == []`; with `multi:false`/absent,
   `value == null`. (Asserts `kind`/`path` are inert to `initial_canvas_values`.)
2. `cargo test -p puffer-core canvas` green. Commit only if a test was added:
   `test(canvas): mediaPicker kind/path items are inert to initial values`.

## Phase 4 — components.md doc

Update the `mediaPicker` entry in `resources/canvas/components.md`:
- items schema gains `kind?: "image" | "video"` (default image), video items use
  `path` (workspace-relative) instead of `url`.
- preview: video items show the first frame as the thumbnail and play with
  controls in the popup.

Commit: `docs(canvas): document mediaPicker video items (kind/path)`.

## Phase 5 — short-drama SKILL Stage 4 rewrite

`resources/skills/short-drama-generation/SKILL.md`:
1. Replace the Stage 4 retry gate (read-only `table` + `multiSelect(retry)`) with
   a `mediaPicker(multi:true)` mirroring Stage 3: one item per succeeded shot —
   `{ id:<shotId>, kind:"video", path:<clip workspace path>, label:<shotId>,
     description:<shot summary> }`, `value` = all succeeded ids. End the turn;
   on read-back the checked ids are the shots kept for composition; unchecked are
   dropped; redo = re-run `videogen` and re-render (Stage 3 redo wording).
2. Update Stage 5 so its `editableTable` rows come from the Stage-4-kept clips,
   not "all succeeded clips".
3. Grep confirms no Rust test asserts Stage 4 prompt text; doc-only.

Commit: `feat(short-drama): Stage 4 video mediaPicker review gate, remove retry`.

## Final verification

- `cd apps/puffer-desktop && <pkg> test` (Vitest) green.
- `cargo test -p puffer-core canvas` green.
- Manual smoke (desktop, optional): run the short-drama skill to Stage 4, confirm
  thumbnails show first frames and clicking plays the clip; uncheck one and verify
  it is excluded from Stage 5.

## Out of scope / deferred

- Re-minting expired access tickets (YAGNI — see spec).
- Adopting the `video_poster` JPEG pipeline for thumbnails (perf option if shot
  counts grow; native first-frame is sufficient now).
