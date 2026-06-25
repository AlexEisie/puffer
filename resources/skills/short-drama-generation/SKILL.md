---
name: short-drama-generation
description: Use when the user asks to create a short drama from a prompt — e.g. "生成短剧", "制作微短剧", "make a short drama", "turn this script into a short drama", "ショートドラマを生成", "숏드라마를 만들어". Orchestrates script, storyboard, optional character images, per-shot video clips, and ffmpeg composition through the existing media tools.
allowed-tools:
  - Bash
  - Read
  - Write
  - Canvas
  - CanvasState
user-invocable: true
disable-model-invocation: false
requires-action: true
---

You orchestrate a short drama by driving the existing media tools yourself. There is
no single short-drama-generation tool. allowed-tools is guidance; media generation is enforced by
the internal tool permission path.

Trigger only on a request to CREATE/generate a short drama. Requests to analyze,
rewrite, summarize, or brainstorm a script do NOT trigger this skill unless the user
asks to produce the drama. Progress-only or promise-only replies are not completion:
after starting, either drive the pipeline or report the concrete blocker plainly.

## Gating mechanism (confirm each authored stage before spending credits)

Each stage that authors content (script, storyboard, character images, per-shot
results, final order) is gated through the existing `Canvas`/`CanvasState` tools so
the user reviews and edits a draft before you act on it. The mechanism uses no new
infrastructure — it is render → end-turn → re-entry → read-back:

- In a desktop inline environment, render the draft with `Canvas`, using
  `canvasId` = `canvas-drama-<id>-stage<N>` (N = the stage number). Then **end the
  current turn** — do NOT busy-wait or poll inside the skill.
- The user edits the Canvas and submits; submission automatically starts a new turn.
  That new turn **first** calls `CanvasState` with the same `canvasId` to read back the
  confirmed `values`, then writes artifacts and advances to the next stage.
- **Non-desktop environment** (no inline submit / no turn re-trigger): degrade to
  text-based per-stage confirmation, stating plainly that there is no visual gating in
  this environment.
- Never call `imagegen`/`videogen` and never advance on draft values before the
  confirmation for that stage has been read back.

## Pipeline (run in order; skip any stage whose inputs the prompt already supplies)

Pick a short kebab slug `<id>` from the drama title. Put project files under
`.puffer/media/drama/<id>/`. Generated image/video artifacts are written by the tools
to `.puffer/media/images|videos/` — you only reference them, never relocate them.

0. **Models (gate before any credit-consuming stage).** Confirm the image and video
   provider/model up front. Both are mandatory.

   Render `Canvas` with `canvasId = canvas-drama-<id>-stage0` whose body is a single
   `mediaModelSelect` node:

   ```json
   { "type": "Canvas", "canvasId": "canvas-drama-<id>-stage0",
     "spec": { "title": "Models", "body": [ { "type": "mediaModelSelect" } ] } }
   ```

   The node is self-populating: it fetches the connected image/video capabilities itself,
   seeds each dropdown from the currently-saved global media defaults, renders an in-place
   "connect a provider in Settings" prompt for any kind with no connected provider, and on
   confirmation persists the choice back to the global media settings. Do **not** fetch
   capabilities, build options, or branch on empty lists yourself. Then **end the turn**.

   In the next turn read back with `CanvasState` (same canvasId): `values` carries
   `{imgProvider,imgModel,vidProvider,vidModel}`. **Validate**: if `imgModel` or `vidModel` is empty,
   stop and report that both image and video models must be selected (direct the user to Settings if a
   kind has no provider). Record the four values in `manifest.json` for Stages 3/4.

1. **Script.** If the prompt already contains a script (or names a script file), use it
   directly (no gate needed). Otherwise draft one, then gate it: render
   `Canvas` with `canvasId = canvas-drama-<id>-stage1` and spec
   `{title:"Script draft",body:[{type:"textarea",id:"script",rows:14,value:"<draft>"}]}`
   The spec is exactly this — the canvas title is the only heading. Do **not** add a
   `summary`, do **not** wrap the textarea in a `card`, and do **not** set `regenerable`;
   the script draft textarea shows directly with only a Submit action. Then end the turn.
   In the next turn read it back with `CanvasState` (same canvasId)
   and save `values.script` to `.puffer/media/drama/<id>/script.md`.

2. **Storyboard.** If the prompt already contains a shot breakdown, use it directly.
   Otherwise break the script into ordered shots (aim for a handful; one beat per shot).
   Give each shot a stable lowercase id (`shot-001`, `shot-002`, …) and record: subject,
   action, scene, lighting, camera, style, target duration (seconds), which characters
   appear, and any stability constraints. These fields become the video prompt — richer
   shots yield better clips.

   Gate the draft: render `Canvas` with `canvasId = canvas-drama-<id>-stage2` and spec
   `{title:"Storyboard",body:[{type:"editableTable",id:"storyboard",layout:"cards",columns:["shotId","subject","action","duration","characters"],rows:<draft shots>}]}`
   (`layout:"cards"` renders one card per shot with column 0 = shotId as the card
   title and the rest as labeled wrapping fields — the editableTable sits directly in
   `body`). Do **not** wrap it in a `card` and do **not** set `regenerable`. Then end
   the turn. In the next
   turn read it back with `CanvasState`: `values` for the editableTable id
   `"storyboard"` is the confirmed 2D array. In one shot, write
   `.puffer/media/drama/<id>/storyboard.md` (a markdown table of the confirmed rows) and
   seed `manifest.json`'s `shots[]` — column 0 is the `shotId`, the remaining columns
   become the shot's prompt fields.

3. **Character images (reference for video).** Scan the prompt for image references that
   are `https://` or `asset://` URLs.
   - If present, use those URLs directly as `--image-reference` in stage 4. Do NOT
     generate images.
   - If absent and the user wants character-consistent shots, **generate one image per character**:
     collect the distinct character names from the confirmed storyboard's `characters`
     column, and for each name run exactly one `imagegen` call —
     `imagegen --prompt "<single-character full-body front-view sheet>" --count 1 --provider <imgProvider> --model <imgModel>`.
     One call → one character → one image; N characters → N calls → N images.
     Never combine multiple characters into one image. Make each character stylized /
     non-photorealistic (cartoon, 3D render, illustration): image-to-video providers
     (e.g. BytePlus) reject photoreal real-person images on moderation. For each image read
     the tool result's `remoteSourceUrl` (same key the video tool uses):
       - If `remoteSourceUrl` is present, record it under that character in `manifest.json`
         `characterRefs` (`{ "<character>": "<url>" }`) and use it as that character's
         `--image-reference` in stage 4.
       - If `remoteSourceUrl` is absent, stop and report that the configured image
         provider does not produce a referenceable URL, so image-to-video is unavailable.
         Do NOT silently fall back to text-to-video.
   - If absent and consistency is not required, run text-to-video in stage 4.

   When you generated the per-character images, gate the choice: render `Canvas` with
   `canvasId = canvas-drama-<id>-stage3` and `title:"Character image"`, whose `body` is a
   single `mediaPicker` with no wrapping card: `{type:"mediaPicker", id:"pick", multi:true,
   value:[<every item id>], items:[{id,url,label,description}, …]}` — one item per character.
   Set `url` to that character's `remoteSourceUrl` (or its asset url on desktop), `label` to
   the character name only, and `description` to that character's sheet description. `value`
   lists every item id, so all characters are checked by default. Then end the turn. In the
   next turn read it back with `CanvasState`: `pick` is the array of checked item ids; map
   each back to its character via `characterRefs`. Checked characters' urls become stage 4
   `--image-reference`s; any unchecked character falls back to text-to-video for the shots it
   appears in. There is no Regenerate toggle — to redo a character, generate it again and
   re-render this canvas.

4. **Per-shot video.** For each shot in storyboard order, run one `videogen` command:
   - `videogen --prompt "<shot visual + action>" --provider <vidProvider> --model <vidModel>`
   - Add `--image-reference <url>` for each character in that shot's `characters` column that
     has a checked `characterRefs` url; keep order stable and refer to them as image 1,
     image 2, … in the prompt. A shot whose characters are all unchecked or unavailable runs
     text-to-video.
   - Each `videogen` call blocks until that clip is finished (the tool polls the provider
     to completion), so set an explicit long Bash timeout within the current Bash cap —
     budget per shot, not for the whole drama. One call → one finished clip.
   - Read `path` from the tool result and record it into the manifest (see below).
   - After running the shots, gate retries: render `Canvas` with
     `canvasId = canvas-drama-<id>-stage4`, a read-only `table` of per-shot status
     (shotId, status) and a `multiSelect` (`id:"retry"`, options = the shotIds), then end
     the turn. In the next turn read it back with `CanvasState` and re-run `videogen` only
     for the shotIds in `values.retry`; if none are selected, advance.

5. **Compose.** Before composing, gate the final order and mux mode: render `Canvas` with
   `canvasId = canvas-drama-<id>-stage5`, a `card` containing an `editableTable`
   (`id:"order"`, `columns: ["shotId"]`, `rows` = the succeeded clips in current order —
   the user confirms/reorders) and a `singleSelect` (`id:"mux"`, options `copy` /
   `re-encode`), then end the turn. In the next turn read it back with `CanvasState`:
   compose in the confirmed `values.order`, preferring stream-copy unless `values.mux` is
   `re-encode`.

   Stitch the successful shot clips in the confirmed order with ffmpeg. First
   probe ffmpeg: `command -v ffmpeg`. If missing, stop and report — do not fake a file.
   Include only shots whose video succeeded; if none succeeded, skip composition and
   report. Build the concat list with single-quote escaping (each clip line is
   `file '<path>'`, with any `'` in the path written as `'\''`). Prefer stream-copy
   (clips from the same provider share codec/params); only if concat-copy fails with a
   codec/params mismatch, retry with a re-encode:

   ```bash
   : > .puffer/media/drama/<id>/concat.txt
   # append one line per SUCCEEDED clip, in order (escape single quotes):
   printf "file '%s'\n" "<clip path, ' -> '\\''>" >> .puffer/media/drama/<id>/concat.txt
   # primary: fast, no re-encode
   ffmpeg -f concat -safe 0 -i .puffer/media/drama/<id>/concat.txt \
     -c copy .puffer/media/drama/<id>/final.mp4
   # fallback only if the copy fails on mismatched streams:
   ffmpeg -f concat -safe 0 -i .puffer/media/drama/<id>/concat.txt \
     -c:v libx264 -pix_fmt yuv420p .puffer/media/drama/<id>/final.mp4
   ```

   If some shots failed but others composed, report it as a partial drama and list the
   missing shot ids.

## Manifest (your working ledger — keep it simple)

Maintain `.puffer/media/drama/<id>/manifest.json` as you go. It is a plain ordered list,
not a schema'd artifact:

```json
{
  "id": "<id>",
  "shots": [
    { "shotId": "shot-001", "status": "succeeded", "prompt": "...", "imageReferences": ["https://..."], "videoArtifactId": "...", "videoPath": ".puffer/media/videos/<aid>/..." }
  ],
  "final": ".puffer/media/drama/<id>/final.mp4"
}
```

## Failure contracts (never paper over)

- If a kind has no connected provider, the Stage 0 node shows an in-place "connect a provider in
  Settings" prompt and that model stays empty; on read-back, stop and tell the user to connect a
  provider for that kind in Settings — never fall back to text-to-video or to config defaults.
- If `imgModel` or `vidModel` is empty on read-back, stop and report that both image and video models
  must be selected before continuing.
- Do not advance any gated stage on draft values — wait for the stage's confirmation to
  be read back first.
- If `CanvasState` returns no value for a gated stage (the user did not submit), report
  "no confirmation received" and stop; do not fall back to the draft.
- In a non-desktop environment (no Canvas / no inline submit), degrade to text-based
  per-stage confirmation — including provider/model — and say so plainly; do not skip the
  confirmation.
- If a chosen video provider is Relaydance (prompt-only) and the user wants image
  references, report that the configured provider does not support image references.
- If ffmpeg is unavailable or composition fails, report it plainly and keep the
  per-shot clips; do not claim a composed drama was produced.
- Report final-video success only when `final.mp4` actually exists; a missing final
  video can still leave useful per-shot clips — say so rather than implying success.
- Do not hand-author placeholder media (SVG, stills, stub mp4) and present it as
  generated output.
