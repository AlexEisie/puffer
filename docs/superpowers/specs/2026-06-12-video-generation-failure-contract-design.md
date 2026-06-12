# Video Generation Failure Contract Design

Date: 2026-06-12

Status: Approved design, reviewed for overdesign, implementation plan written

Constraints: do not optimize for backward compatibility; optimize for
long-term clarity, stability, and performance; prevent overdesign.

## Problem

The `Milhous` session asked Puffer to create a robot battle video. The request
correctly activated the `video-generation` skill and ran:

```bash
videogen --prompt '创建一个短视频，画面为两台未来机器人在废墟城市中战斗，电影感，动态镜头，金属火花和爆炸，夜晚雨中，高清写实风格。'
```

The command returned a normal JSON result, but that result only exposed:

```json
{
  "jobId": "7c9bdcea-bc2a-4866-b856-e6a374bb97e2",
  "kind": "video",
  "requestedCount": 1,
  "artifacts": [],
  "provider": "worldrouter",
  "model": "seedance-2.0-fast",
  "status": "failed",
  "parameters": {
    "duration": "5",
    "resolution": "480p"
  },
  "purpose": null
}
```

The persisted media job had the actionable diagnostic:

```json
{
  "providerJobId": "agt_56f6548fd09642da9081a760ab00",
  "remoteStatus": "failed",
  "error": "The service encountered an unexpected internal error."
}
```

The failure was therefore not a missing skill, a failed shell command, or a
local validation rejection. It was a provider-side task failure whose details
were persisted but not returned through the `videogen` contract. The model and
user had to inspect `.puffer/media/jobs/<job-id>.json` to see the real reason.

## Goals

- Make `videogen` stdout the trusted, self-contained result for one video
  generation request.
- Surface persisted job diagnostics in the workflow JSON when a video job
  fails.
- Keep the diagnostic contract deterministic: the workflow JSON always includes
  the three diagnostic keys, using `null` when a value is absent.
- Preserve the current media job state machine and artifact persistence model.
- Keep provider adapters responsible for writing `provider_job_id`,
  `remote_status`, and `error` onto `MediaJob`.
- Keep the workflow layer responsible only for exposing those fields to the
  model/user boundary.
- Use field names that can later support a unified image/video result contract
  without requiring a redesign.

## Non-Goals

- Do not add automatic retry.
- Do not add provider health checks or a provider health cache.
- Do not change the default video provider or model.
- Do not add a new media error enum or failure taxonomy.
- Do not redesign the media runtime, polling loop, or job state machine.
- Do not change desktop UI behavior unless existing DTOs or tests require a
  minimal update for the new fields.
- Do not change image generation behavior except for the smallest shared-struct
  adjustment required by compilation.
- Do not expose additional job internals such as prompt text, adapter id,
  `remoteGetUrl`, raw provider payloads, or HTTP response bodies.

## Selected Approach

Extend the exact media result contract with job diagnostics:

- `provider_job_id`
- `remote_status`
- `error`

Then expose those fields from `VideoGeneration` as camelCase JSON:

- `providerJobId`
- `remoteStatus`
- `error`

Rejected alternatives:

- Return a Rust `Err` for remote failed jobs. This makes the command look like
  the local tool failed, even though a provider job exists and the stable result
  is `status: "failed"`.
- Add automatic retry. This may reduce some transient failures, but it adds
  latency and cost and does not fix the missing diagnostic contract.
- Expand to full media generation stability work. That is valuable later, but
  it would touch image generation, provider health, UI presentation, and retry
  policy. The current issue needs a smaller execution-contract fix.

## Data Flow

The current flow remains intact:

```text
videogen
  -> internal tool permission broker
  -> VideoGeneration workflow
  -> generate_exact_media_with_cache
  -> provider adapter
  -> MediaJob
  -> workflow JSON output
```

Only two boundaries change:

1. `puffer-media` returns the job diagnostics in
   `ExactMediaGenerationResult`.
2. `puffer-core` includes those diagnostics in
   `video_generation_output`.

`MediaJob` stays the single persisted source of truth for:

- provider id
- model id
- local status
- provider job id
- remote status
- error
- artifact ids

## Output Contract

For a failed remote job, `videogen` should return:

```json
{
  "jobId": "7c9bdcea-bc2a-4866-b856-e6a374bb97e2",
  "kind": "video",
  "requestedCount": 1,
  "artifacts": [],
  "provider": "worldrouter",
  "model": "seedance-2.0-fast",
  "status": "failed",
  "providerJobId": "agt_56f6548fd09642da9081a760ab00",
  "remoteStatus": "failed",
  "error": "The service encountered an unexpected internal error.",
  "parameters": {
    "duration": "5",
    "resolution": "480p"
  },
  "purpose": null
}
```

For successful jobs or failed jobs without a provider diagnostic, the workflow
JSON must still include the same three diagnostic keys with `null` values:

```json
{
  "providerJobId": null,
  "remoteStatus": null,
  "error": null
}
```

This removes caller ambiguity and keeps the output shape stable without adding a
new result variant.

## Diagnostic Safety

Only these job diagnostics should cross the `videogen` stdout boundary:

- `providerJobId`
- `remoteStatus`
- `error`

Do not add `remoteGetUrl`, adapter id, prompt text, raw provider response JSON,
HTTP request metadata, headers, or credentials to the result. Artifact entries
already expose persisted local artifact paths on success; that behavior stays
unchanged.

Provider errors that originate from submit, poll, or download failures must keep
using the existing secret-redaction paths before they reach `MediaJob.error`.
This design does not add new redaction logic; it preserves the current boundary
and only exposes the already persisted, redacted `error` field.

## Error Semantics

Remote terminal failure:

- Adapter maps the provider terminal failure to `MediaJobStatus::Failed`.
- Adapter writes `remote_status` and `error` when available.
- `generate_exact_media_with_cache` returns a normal exact media result.
- `videogen` exits successfully and prints JSON with `status: "failed"` plus
  diagnostics.

Local pre-job failure:

- Config missing, auth missing, invalid prompt, invalid parameters, or submit
  failure before a stable provider job exists remains a tool error.
- The user sees a command/tool failure because there is no stable failed media
  job to report.

Remote success with unusable output:

- Existing behavior remains: if the provider reports success but the output URL
  or artifact cannot be persisted, the job is marked failed and the persisted
  error is exposed through the same result fields.

## Implementation Boundaries

`puffer-media`:

- Extend `ExactMediaGenerationResult` with optional diagnostics:
  `provider_job_id`, `remote_status`, and `error`. The struct already uses
  `#[serde(rename_all = "camelCase")]`, so serialized field names become
  `providerJobId`, `remoteStatus`, and `error`.
- Populate the fields in `exact_media_generation_result(job, artifacts)` from
  `MediaJob`.
- Avoid adding provider-specific logic to the result builder.

`puffer-core`:

- Extend `video_generation_output` to include `providerJobId`,
  `remoteStatus`, and `error` on every response, using JSON `null` when the
  result has no value.
- Keep parameter and artifact output unchanged.
- Do not inspect provider-specific job files from workflow code.

Provider adapters:

- No new provider behavior is required for this fix.
- Existing WorldRouter behavior already persists the needed fields for the
  observed failure.
- If a provider does not populate diagnostics, the result simply omits or nulls
  those fields.

## Testing

Add focused tests only:

1. `puffer-media` exact media result includes `error`, `remote_status`, and
   `provider_job_id` for a failed video job.
2. WorldRouter adapter remote failure persists the provider error into
   `job.error` and `remote_status`.
3. `puffer-core` `VideoGeneration` output serializes failed-job diagnostics as
   `error`, `remoteStatus`, and `providerJobId`.
4. Successful video output includes the same diagnostic keys as `null`, while
   artifact output stays unchanged.

Avoid broad test matrices for retries, health checks, UI rendering, or image
generation. Those belong to later work if the scope expands.

## Acceptance Criteria

- Replaying a Milhous-style WorldRouter remote failed job produces `videogen`
  JSON with the provider error text.
- The assistant can report the real cause directly from tool output without
  reading `.puffer/media/jobs`.
- Successful video generation still returns the same artifact entries.
- Successful video generation returns `providerJobId`, `remoteStatus`, and
  `error` as `null` when no values are present.
- No new background services, retry loops, or provider health state are added.
- The change remains localized to media result shaping and workflow output.
