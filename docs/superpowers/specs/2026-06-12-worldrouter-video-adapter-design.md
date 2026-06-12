# WorldRouter Video Adapter Design

Date: 2026-06-12

Status: Approved design, pending implementation plan

Constraints: do not preserve backward compatibility for the old WorldRouter
video path; optimize for long-term clarity, stability, and runtime performance;
avoid a broad async-video framework.

## Problem

The `Milhous`-reported video generation failure surfaced as:

```text
provider=worldrouter adapter=relaydance_video task=unknown
```

The failing session ran:

```bash
videogen --prompt '创建一个机器人战斗的视频'
```

The active user media config selected:

```toml
[media.video]
provider_id = "worldrouter"
logical_model_id = "seedance-2.0-fast"

[media.video.selections]
duration = "5"
resolution = "480p"
```

The bundled WorldRouter provider descriptor currently routes video generation
through the generic `relaydance_video` adapter:

```yaml
video:
  execution:
    adapter: relaydance_video
    base_url: https://inference-api.worldrouter.ai
    path: /api/v3/contents/generations/tasks
    prompt_format: content_array
```

WorldRouter's official Seedance API is not the old Relaydance/NewAPI task
shape. It is a native async task API:

- Submit: `POST /api/v3/contents/generations/tasks`
- Submit response: `{ "id": "task-123", "requestId": "req-123" }`
- Poll: `GET /api/v3/contents/generations/tasks/{task_id}`
- Poll success response includes `status: "succeeded"` and
  `content.video_url`.

The current `relaydance_video` submit path parses the submit response with
`RelaydanceVideoTask::from_value`, which requires both a task id and a
`status`. Because WorldRouter submit responses do not include `status`, submit
parsing fails before a `MediaJob` is persisted. That is why the error says
`task=unknown` and no WorldRouter job file appears in `.puffer/media/jobs`.

## Goals

- Introduce a dedicated `worldrouter_video` adapter for WorldRouter Seedance.
- Make submit parsing match the documented WorldRouter contract: submit only
  needs a task id.
- Persist the queued job immediately after submit succeeds.
- Poll the documented task status endpoint until terminal state.
- Persist generated MP4 artifacts from `content.video_url`.
- Make errors stage-specific and useful for diagnosis.
- Keep shared video job mechanics in `video_jobs`; do not create a generic
  provider framework.

## Non-Goals

- Do not support the old WorldRouter-through-`relaydance_video` path.
- Do not preserve compatibility with a WorldRouter submit response that contains
  `status`.
- Do not redesign desktop media settings.
- Do not add background polling or UI event streaming.
- Do not alter BytePlus, Replicate, or Relaydance behavior except where a shared
  adapter enum or adapter availability list requires adding `worldrouter_video`.
- Do not add provider-specific UI.

## Selected Approach

Add a focused `worldrouter_video` adapter and switch WorldRouter's video
execution descriptor to it.

Rejected alternatives:

- Keep WorldRouter inside `relaydance_video` with conditional parsing.
  This is smaller, but it preserves a misleading adapter boundary and makes the
  next protocol drift harder to diagnose.
- Build a generic async-video schema framework.
  This is too broad for the current provider set. The only needed abstraction is
  already present in `video_jobs`: polling, terminal status mapping, and artifact
  persistence.

## Architecture

Add:

```text
crates/puffer-media/src/media/worldrouter_video.rs
```

The module owns the WorldRouter Seedance protocol:

- request body construction
- task submission
- submit response parsing
- task polling
- poll response parsing
- successful artifact completion through shared video job helpers

The module should expose a production adapter plus a small test transport, using
the existing pattern from `relaydance_video.rs` and `byteplus_video.rs`.

Shared helpers remain in:

```text
crates/puffer-media/src/media/video_jobs.rs
```

`worldrouter_video` should reuse:

- `VideoPollingConfig`
- `video_poll_url`
- `poll_video_until_terminal`
- `record_transient_poll_error`
- `persist_failed_video_job`
- `complete_video_job`
- `map_video_task_status`

Provider descriptor update:

```yaml
video:
  execution:
    adapter: worldrouter_video
    base_url: https://inference-api.worldrouter.ai
    path: /api/v3/contents/generations/tasks
    prompt_format: content_array
```

The provider's public model ids stay stable:

- `seedance-2.0`
- `seedance-2.0-fast`

## Data Flow

1. `videogen` sends a `video-generation` internal tool request.
2. Runtime reads `[media.video]` and resolves `worldrouter/seedance-2.0-fast`.
3. Media resolver returns adapter `worldrouter_video`, concrete model id, and
   request-field parameters.
4. `generate_exact_video_from_media_request` dispatches to
   `generate_worldrouter_video`.
5. Adapter submits the task with a body shaped like:

   ```json
   {
     "model": "seedance-2.0-fast",
     "content": [
       { "type": "text", "text": "创建一个机器人战斗的视频" }
     ],
     "resolution": "480p",
     "duration": 5
   }
   ```

6. Submit parser reads `id` and optional `requestId`.
7. Runtime creates a queued `MediaJob` with:

   - `providerId = "worldrouter"`
   - `adapter = "worldrouter_video"`
   - `providerJobId = submit.id`
   - `remoteStatus = null` or `"submitted"`

8. Polling calls:

   ```text
   GET /api/v3/contents/generations/tasks/{task_id}
   ```

9. Poll parser reads:

   - `id`
   - `status`
   - `content.video_url` when present
   - provider error message fields when present

10. On `succeeded`, `complete_video_job` downloads and persists the MP4.

## Parsing Rules

### Submit Response

Required:

- `id` as a non-empty string

Optional:

- `requestId`

The submit parser must not require `status`.

### Poll Response

Required:

- `id` as a non-empty string
- `status` as a non-empty string

Success:

- `status == "succeeded"` maps to `MediaJobStatus::Succeeded`
- `content.video_url` is required before artifact completion

Failure:

- `failed` and `expired` map to failed
- `cancelled` maps to canceled
- readable error text should be taken from the most specific available field,
  such as `error.message`, `message`, or provider-specific reason fields.

Unknown status should use the existing shared status mapping behavior: keep the
job non-terminal so bounded polling can continue.

## Error Handling

All user-visible adapter errors should include provider, adapter, and phase:

```text
provider=worldrouter adapter=worldrouter_video phase=submit
provider=worldrouter adapter=worldrouter_video phase=poll task=task-123
provider=worldrouter adapter=worldrouter_video phase=download task=task-123
```

Submit failures are terminal for that tool invocation because no local job can
be safely resumed without a task id.

Poll transport and parse failures are transient. They should record the latest
job error and keep polling within the bounded attempt budget.

Terminal provider failures must persist the job as failed with the provider's
best available error message.

If a poll response is `succeeded` but lacks `content.video_url`, mark the job
failed with a precise diagnostic:

```text
succeeded WorldRouter video task is missing content.video_url
```

Secrets must continue to be redacted through the existing provider error
redaction path.

## Performance

This design keeps the current synchronous generation behavior. It does not add
background workers, new event streams, or a scheduler.

Runtime cost is bounded by the existing polling configuration. The adapter adds
no extra requests beyond the documented submit plus poll loop.

Request parsing should use direct JSON field access, not schema reflection or a
generic mapping engine.

## Testing

Add fixtures for:

- WorldRouter submit success:

  ```json
  { "id": "task-123", "requestId": "req-123" }
  ```

- WorldRouter poll queued/running.
- WorldRouter poll succeeded:

  ```json
  {
    "id": "task-123",
    "model": "seedance-2.0-fast",
    "status": "succeeded",
    "content": {
      "video_url": "https://media.example.com/output.mp4"
    },
    "resolution": "480p",
    "duration": 5
  }
  ```

- WorldRouter poll failed with an error message.
- Submit response missing `id`.
- Poll success missing `content.video_url`.

Required test coverage:

- request body matches WorldRouter docs for text-to-video.
- submit without `status` creates a queued job.
- poll success downloads and stores one MP4 artifact.
- submit parse failure includes `phase=submit` and response shape context.
- poll parse/transport failure is transient when a task id exists.
- `resources/providers/worldrouter.yaml` declares `adapter: worldrouter_video`.
- capability listing exposes WorldRouter Seedance via `worldrouter_video`.
- the old `relaydance_video` tests still pass unchanged for Relaydance fixtures.

## Acceptance Criteria

- `videogen` with `[media.video] provider_id = "worldrouter"` no longer fails
  at submit because `status` is absent.
- A successful WorldRouter Seedance job persists a local MP4 artifact.
- WorldRouter failures produce phase-specific diagnostics instead of
  `task=unknown` unless the submit response truly lacks an id.
- No generic async-video framework is introduced.
- `cargo test -p puffer-media` passes.
- Provider resource tests verify the WorldRouter adapter declaration.

