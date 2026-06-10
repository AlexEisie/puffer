# Short Drama Generation Internal Tool Design

Date: 2026-06-10

## Summary

Puffer should add a product-grade `ShortDramaGeneration` internal tool for one
click short-drama project package generation.

The first version produces a durable project package, not a final composed MP4.
It plans the story, writes inspectable script and shot artifacts, generates one
video artifact per shot through the existing media runtime, and returns a
compact summary with retryable failures.

This is intentionally a thin orchestrator. It should not become a generic media
workflow engine, background queue, video compositor, or provider adapter layer.

## Product Contract

The tool accepts a short natural-language brief and creates:

- a stable short drama id
- a readable script and character/style notes
- a structured shot list
- one prompt file per generated shot
- generated media artifact references for successful shots
- a manifest that records project, shot, artifact, and failure state

The package lives under:

```text
.puffer/media/short-dramas/<short_drama_id>/
  manifest.json
  script.md
  shots.json
  prompts/
    shot-001-video.md
    shot-001-keyframe.md  # only when keyframes are enabled
  artifacts.json
```

The first version does not promise:

- final video concatenation
- subtitles or burned captions
- voiceover, music, or audio mixing
- automatic asset upload
- background job recovery
- arbitrary long-form productions

## Recommended User-Facing Flow

The agent uses a thin companion skill to call a helper alias:

```bash
shortdrama --brief "60 second vertical suspense short drama about ..."
```

The helper forwards to the internal CLI command, which sends a structured
execution request to the parent runtime. The runtime performs all validation,
planning, media generation, manifest persistence, and summary rendering.

## Input Schema

The internal tool schema should stay small:

```json
{
  "brief": "string",
  "targetDurationSeconds": 60,
  "aspect": "9:16",
  "style": "realistic vertical short drama",
  "shotDurationSeconds": 5,
  "generateKeyframes": false,
  "maxShots": 12,
  "purpose": "short-drama-package"
}
```

Rules:

- `brief` is required.
- All other fields have defaults.
- `targetDurationSeconds` guides planning but does not guarantee exact final
  runtime.
- `shotDurationSeconds` must map to selected video provider capability values.
- `maxShots` defaults to `12` and has an absolute cap of `20`.
- `targetDurationSeconds` has an absolute cap of `180`.
- `generateKeyframes` defaults to `false` because local generated images cannot
  currently be passed directly to `VideoGeneration`; only public `https://` or
  approved `asset://` references are supported by the existing video path.

## State Model

Use one project-level status and one shot-level status.

Project statuses:

- `planned`: script, shots, and prompt files are persisted; media generation has
  not started.
- `running`: at least one media call is in progress or has completed while other
  shots remain.
- `succeeded`: every required shot has a video artifact.
- `partial`: at least one shot succeeded and at least one shot failed.
- `failed`: planning failed or no usable video artifacts were produced.

Shot statuses:

- `planned`
- `running`
- `succeeded`
- `failed`

`partial` is a first-class successful product outcome: the user can inspect the
package and retry failed shots later.

## Runtime Architecture

Add these focused pieces:

- `resources/internal_tools/short_drama_generation.yaml`
  - Defines `ShortDramaGeneration`, aliases, schema, approval policy, network
    sandbox, and media display grouping.
- `resources/skills/short-drama-generation/SKILL.md`
  - Teaches the agent when to call `shortdrama`, how to map user language to the
    schema, and how to explain the returned project package.
- `crates/puffer-tools/src/internal_tools.rs`
  - Adds the stable CLI-only descriptor and helper alias `shortdrama`.
- `crates/puffer-cli/src/short_drama_internal_tools.rs`
  - Parses CLI args and builds the JSON payload. It contains no business logic.
- `crates/puffer-cli/src/cli_args.rs`
  - Adds the hidden internal command.
- `crates/puffer-core/runtime/internal_tool_permissions.rs`
  - Dispatches canonical `shortdramageneration` requests to the workflow
    executor after normal internal permission resolution.
- `crates/puffer-core/runtime/claude_tools/workflow/short_drama_generation.rs`
  - Owns validation, planning, project package persistence, media runtime calls,
    manifest updates, and summary output.

Do not add a new crate, database, generic project engine, or media job system in
the first version.

## Execution Flow

1. Validate input and current media settings.
   - Reject empty briefs.
   - Reject over-limit duration or shot counts.
   - Reject unsupported video duration, aspect, or resolution values before any
     media generation starts.
   - Require configured video media provider, model, operation, and adapter.

2. Plan the short drama package.
   - Produce a strict structured plan with title, logline, characters, style
     bible, continuity notes, and shots.
   - Each shot receives a stable `shotId`, intent, scene description, camera
     direction, continuity notes, and video prompt seed.
   - Parse the plan strictly. Allow at most one repair attempt. Do not loop
     indefinitely.

3. Persist planning artifacts.
   - Create the project directory.
   - Write initial `manifest.json`.
   - Write `script.md`, `shots.json`, and `prompts/*.md`.
   - Persist before media generation so failures leave inspectable state.

4. Generate media.
   - Run serially in the first version.
   - Optionally generate keyframes and record their artifacts, but do not require
     video generation to consume those keyframes.
   - Generate one video per shot using the existing exact media runtime, not by
     recursively invoking shell helpers.
   - Update `manifest.json` after every shot.

5. Return a compact JSON summary.
   - Include `shortDramaId`, `status`, `manifestPath`, `scriptPath`,
     `shotsTotal`, `shotsSucceeded`, `shotsFailed`, `artifacts`, and
     `retryableFailures`.

## Error Handling

Planning failures stop before media generation.

Media failures are recorded per shot:

```json
{
  "shotId": "shot-003",
  "status": "failed",
  "error": "provider rate limit",
  "retryable": true,
  "promptPath": "prompts/shot-003-video.md"
}
```

Retry classification:

- Provider rate limits, timeouts, and transient network failures are retryable.
- Invalid configuration, unsupported parameters, empty prompts, and schema
  violations are not retryable.

The first version should not implement automatic retries, exponential backoff,
or background recovery. It should expose retryable failures in the manifest and
summary.

## Performance Strategy

Use predictable serial generation in the first version.

Reasons:

- Video providers are the bottleneck.
- Parallel generation increases rate-limit and partial-failure complexity.
- Serial execution simplifies manifest updates and user-visible progress.

Keep the internal implementation structured so later work can add a bounded
`maxConcurrency = 2` without changing the manifest format. Do not expose
concurrency in the first version.

Prompt limits:

- `brief`: 4,000 characters
- one shot video prompt: 8,000 characters
- style bible: 4,000 characters
- continuity notes: 4,000 characters

## Companion Skill

Create a thin `short-drama-generation` skill with skill-creator. The skill is
agent-facing usage guidance, not product logic.

It should teach the agent:

- when to use `shortdrama` instead of manual `imagegen` or `videogen` calls
- how to map user requests into the small input schema
- how to explain package outputs and partial failures
- that the first version does not create final MP4 compositions
- not to hand-author placeholder videos or imply success without artifacts

The skill should not include scripts or assets. Avoid a `references/` directory
unless real short-drama prompting guidance proves too large for `SKILL.md`.

## Non-Goals

- No final MP4 composition.
- No subtitles, voiceover, music, or audio mixing.
- No generic workflow engine.
- No new provider adapter.
- No background queue.
- No database-backed project model.
- No model-facing promotion of internal media tools.
- No automatic natural-language media classification.
- No hidden local image upload path for video references.

## Testing

Resource tests:

- `ShortDramaGeneration` loads as an internal tool.
- It is absent from normal model-facing tool definitions.
- The `shortdrama` alias resolves through internal descriptor helpers.
- The companion skill documents the helper alias and does not instruct direct
  `puffer internal-tool` calls.

CLI tests:

- CLI args serialize into the expected JSON input.
- Invalid JSON flags fail at the CLI boundary.
- Missing parent internal execution endpoint fails clearly.

Core workflow tests:

- valid input creates the project directory and manifest
- empty brief fails
- missing video media config fails
- unsupported parameter values fail before media generation
- planning artifacts are persisted before first media call
- successful shots write prompt files and artifact references
- one failed shot produces project status `partial`
- all shot failures produce project status `failed`
- manifest updates happen after each shot
- over-limit `maxShots` and `targetDurationSeconds` are rejected

Integration tests:

- internal permission dispatch routes canonical `shortdramageneration` to the
  workflow executor
- permission denial does not start media generation
- existing `ImageGeneration` and `VideoGeneration` behavior remains isolated

Do not run real provider media generation in default CI. Keep live provider
tests manual or ignored.

## Open Product Decisions

These are deliberately out of scope for the first implementation:

- final composition command and file contract
- retry command shape
- UI project viewer
- asset upload or staging for local keyframes
- subtitle and audio generation

The first version should leave these as future additions around the manifest,
not hooks inside the initial control flow.
