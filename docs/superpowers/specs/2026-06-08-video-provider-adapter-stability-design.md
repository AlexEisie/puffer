# Video Provider Adapter Stability Design

Date: 2026-06-08

## Problem

The Milhous session `c6ad17e3-7444-48a5-ba32-0c92bc89788b` failed a
`VideoGeneration` tool call with:

```text
Tool execution failed: video task response missing `id`
```

The local media job proves the remote task was created:

- `providerId = relaydance`
- `modelId = doubao-seedance-2-0-720p`
- `adapter = openai_video`
- `providerJobId = task_TA20QV69xGqQXapyyM3ynyB1elNki8pg`
- `status = queued`

That means `POST /v1/video/generations` returned a parseable task id at least
once. The failure is therefore most likely in the polling path:
`GET /v1/video/generations/{task_id}` returned a successful JSON body whose
shape does not match the current parser's hardcoded top-level `id` and
`status` assumptions.

The deeper issue is not one missing fallback path. The current adapter name
`openai_video` implies a generic protocol, but the implementation has only been
tested against one idealized envelope. Existing providers also expose or
discover video-capable model names, especially BytePlus Seedance/Dreamina
models, and Puffer needs a stable way to add them without turning YAML into an
untyped protocol engine.

## Goals

- Fix the current Relaydance video polling failure with a verified parser.
- Make future video model onboarding model-driven when the protocol is already
  implemented.
- Add existing-provider video support where Puffer can verify and execute the
  provider protocol, starting with BytePlus text-to-video models.
- Keep protocol behavior typed, tested, and adapter-owned.
- Reuse one stable video task lifecycle for submit, poll, download, and job
  persistence.
- Improve error observability so provider shape mismatches identify the phase,
  provider, task id, and safe response summary.
- Avoid over-design: no generic response mapping DSL, no recovery scheduler,
  and no automatic capability inference from model names.

## Non-Goals

- No backward compatibility for the misleading `openai_video` adapter id. The
  implementation may rename it to the verified protocol id and update bundled
  config/resources in the same change.
- No JSONPath, JQ, template, or arbitrary response-path configuration in YAML.
- No automatic video capability declaration from discovered model names such as
  `seedance`, `sora`, `veo`, `video`, or `dreamina`.
- No image-to-video, first-frame, last-frame, reference image, video editing,
  video extension, or batch video generation in this pass.
- No background recovery queue for already-submitted video jobs.
- No UI redesign. Existing media settings should consume capability metadata
  produced by the daemon.
- No declaration of providers whose video protocol has not been implemented and
  tested in Puffer.

## Design Principle

Puffer should be generic at the model catalog layer and explicit at the protocol
layer.

Model catalog entries belong in YAML. If a provider has an implemented video
adapter and a new model uses the same endpoint, request shape, task lifecycle,
and response envelope, adding that model should require only a YAML entry.

Protocol behavior belongs in Rust. Submit bodies, polling responses, terminal
status mapping, provider error envelopes, and output URL extraction must be
typed and unit-tested. This keeps runtime failures close to the adapter that
understands the provider protocol.

## Chosen Approach

Introduce a shared video task lifecycle and move provider-specific protocol
details behind small adapters.

The shared lifecycle handles:

1. Build a local `MediaJob` after submit returns a remote task id.
2. Save queued/running/failed/canceled/succeeded job states.
3. Poll with bounded backoff.
4. Download the terminal video URL through the existing safe downloader.
5. Persist `video/mp4` artifacts.
6. Save a redacted error on deterministic provider failures.

Each protocol adapter handles:

1. Request body construction.
2. Submit response parsing.
3. Poll response parsing.
4. Provider error envelope parsing.
5. Terminal output URL extraction.
6. Status normalization into the shared lifecycle status model.

This avoids duplicating the job/artifact machinery for every provider while
keeping response parsing explicit and testable.

Rejected alternatives:

- Add fallback checks for `data.id`, `task_id`, `result.id`, and similar paths
  to the current parser. This is fast but guessy, and it can turn provider error
  envelopes into misleading task states.
- Add YAML response mappings such as `id_path`, `status_path`, and `url_path`.
  This moves protocol logic into strings, delays validation to runtime, and
  makes every provider descriptor a partial parser.
- Build a fully generic video provider engine. Puffer does not yet have enough
  verified video protocols to justify that abstraction.

## Adapter Naming

Do not keep `openai_video` as the execution id.

The implementation must first probe the existing Relaydance task
`task_TA20QV69xGqQXapyyM3ynyB1elNki8pg` and record the safe response shape. Then
choose the adapter name from evidence:

- Use `newapi_video` only if Relaydance submit and poll responses match a
  documented NewAPI-style envelope with top-level task identity, status, and a
  stable output URL location.
- Use `relaydance_video` if Relaydance wraps task state in a provider-specific
  envelope or has provider-specific error semantics.

The old `openai_video` enum variant, adapter id, tests, and bundled provider
resource should be replaced rather than aliased.

## Existing Provider Video Onboarding

This pass should connect current providers only when Puffer can execute their
video protocol.

| Provider | This pass | Reason |
| --- | --- | --- |
| `relaydance` | Yes | Current configured provider fails at poll parsing; fix and verify this path first. |
| `byteplus` | Yes, after live probe | The local discovery cache contains Seedance/Dreamina video model ids. BytePlus is an existing provider and should be the first non-Relaydance video adapter if its protocol is verified. |
| `minimax` / `minimax-cn` | Audit only | Existing image adapter does not prove video task semantics. Add a video adapter in a later pass after protocol verification. |
| `openai` | Audit only | Official OpenAI video semantics must not be assumed to match the current `/v1/video/generations` path. |
| `openrouter` | Audit only | Video routing is a separate API surface from current chat-image output. |
| `vercel-ai-gateway` | Audit only | Current image support uses images/chat output paths; video needs a stable REST contract before declaration. |
| `xai` | Audit only | Current provider has image descriptors only. |
| `zhipu` | Audit only | Current provider has image descriptors only. |
| Text-only/local providers | No | No bundled video generation protocol. |

The BytePlus addition must be limited to text-to-video models whose request and
response shapes are verified. Candidate model ids from local discovery include
Seedance and Dreamina entries, but a model name alone is not enough to declare
capability. If the BytePlus live probe does not produce a stable executable
contract, keep BytePlus in audit-only status for this pass.

## Provider Descriptor Shape

Keep the existing `media.video` descriptor model:

```yaml
media:
  video:
    discovery:
      adapter: static
    execution:
      adapter: relaydance_video
      path: /v1/video/generations
    models:
      - id: provider-model-id
        display_name: Human Name
        operations:
          - generate
        parameters:
          - name: duration
            label: Duration
            values: ["5", "10"]
            default: "5"
            request_field: seconds
```

`request_field` may continue to support a small request-body convention such as
`metadata.resolution`, because that is construction logic and already has local
tests. Response paths must not be configurable in YAML.

If BytePlus requires a different body layout, create a `byteplus_video` adapter
that maps the same scalar descriptor parameters into that body. Do not encode
BytePlus request structure through a general-purpose YAML template.

## Runtime Data Flow

`VideoGeneration` remains a thin tool over exact media generation:

```text
VideoGeneration
  -> generate_exact_media_with_cache(kind = video)
  -> validate_media_generate_selection
  -> video adapter match
  -> protocol adapter builds submit request
  -> shared lifecycle saves queued job
  -> shared lifecycle polls until terminal
  -> protocol adapter extracts terminal output URL
  -> shared lifecycle downloads and persists artifact
```

The tool contract does not change. Provider and model selection still comes
from saved `media.video` settings, and scalar tool parameters still override
saved defaults before runtime validation.

## Error Handling

Errors should identify the runtime phase:

- `submit video task ...`
- `poll video task ...`
- `download video output ...`

For successful HTTP responses with unexpected JSON, report a safe shape summary
instead of only `missing id`. A good diagnostic includes:

- provider id
- adapter id
- phase
- task id, when known
- top-level JSON keys
- provider error code/message, when present

Example:

```text
poll video task response missing task id: provider=relaydance adapter=relaydance_video task=task_TA20... keys=[code,message,data]
```

If submit succeeds and a later parse/download error is deterministic, update
the local job to `failed` with the redacted error. Do not leave it indefinitely
queued.

## Tests

Add tests before implementation.

Shared lifecycle tests:

- Submit saves a queued job with `providerJobId`.
- Poll running updates the existing job.
- Poll completed downloads one MP4 artifact.
- Poll failed saves a failed job with provider error text.
- Poll parser failure after submit marks the job failed.
- Bounded polling still stops after the configured attempt count.

Relaydance adapter tests:

- Submit parses the verified live fixture.
- Poll running parses the verified live fixture.
- Poll completed extracts the verified output URL location.
- 2xx provider error envelope reports the provider message instead of
  `missing id`.
- `resources/providers/relaydance.yaml` uses the new adapter id.
- Old `openai_video` descriptors are rejected.

BytePlus adapter tests:

- Request body maps scalar descriptor parameters into the verified BytePlus
  text-to-video request shape.
- Submit parses the verified BytePlus task fixture.
- Poll terminal extracts the verified output URL.
- BytePlus provider errors produce redacted, phase-specific errors.
- BytePlus YAML declares only verified text-to-video model ids and parameters.

Resolver/resource tests:

- Only providers with implemented video adapters expose `status = available`
  when authenticated.
- Unauthenticated but declared video providers expose `missing_auth`.
- Providers with no implemented video adapter do not declare `media.video`.
- No discovered model name alone creates a video capability.

## Verification Workflow

Before coding parser changes, run read-only provider probes with stored
credentials:

1. Poll the existing Relaydance task id and save a redacted fixture.
2. Submit a minimal Relaydance text-to-video request only if the existing task
   no longer provides enough evidence.
3. Query BytePlus model discovery and identify the exact text-to-video model ids
   to declare.
4. Submit and poll one minimal BytePlus text-to-video task, saving redacted
   submit and poll fixtures.

Do not print API keys. Fixtures must not include bearer tokens or signed output
URLs unless the URL is already temporary and safe to store; otherwise replace
the URL with a stable redacted stand-in while preserving its field location.

## Stability And Performance

- Capability listing remains local and descriptor-driven.
- Video generation remains one remote task per tool call.
- Polling stays bounded and synchronous in this pass.
- Shared lifecycle code reduces duplicate job/artifact logic across providers.
- Typed adapters keep parsing failures deterministic and cheap.
- No new background worker, scheduler, database migration, or desktop state
  model is needed.

## Source References

- Current failing parser:
  `crates/puffer-core/runtime/media/openai_video.rs`
- Exact media video routing:
  `crates/puffer-core/media_runtime.rs`
- Video workflow tool:
  `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs`
- Provider descriptor model:
  `crates/puffer-provider-registry/src/model.rs`
- Current Relaydance descriptor:
  `resources/providers/relaydance.yaml`
- Local failed job:
  `/Users/zhangxiao/.puffer/media/jobs/296201e7-47a2-473b-bfb4-6af80f671ba5.json`
- Local failing session:
  `/Users/zhangxiao/.puffer/sessions/c6ad17e3-7444-48a5-ba32-0c92bc89788b.session.jsonl`
