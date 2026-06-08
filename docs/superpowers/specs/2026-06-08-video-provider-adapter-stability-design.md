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
- Add existing-provider video support only after Puffer verifies the provider
  protocol. BytePlus text-to-video is the first candidate, not an unconditional
  deliverable.
- Keep protocol behavior typed, tested, and adapter-owned.
- Keep lifecycle reuse incremental: extract shared helpers only when a second
  verified adapter would otherwise duplicate the same job/artifact code.
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

## Scope Review Outcome

The first draft overreached by treating a full shared video lifecycle as a
required foundation before the second executable provider is verified. That is
not necessary to fix the current bug and risks creating an abstraction around
one real protocol plus one unknown protocol.

The tightened design is:

1. Verify the Relaydance poll response shape.
2. Fix and rename the current adapter around that verified shape.
3. Improve phase-specific diagnostics and failed-job persistence in the current
   adapter.
4. Probe BytePlus separately.
5. Add `byteplus_video` only if BytePlus produces a stable text-to-video
   contract.
6. Extract shared helpers only at the point where Relaydance and BytePlus have
   concrete duplicated lifecycle code.

## Chosen Approach

Use small, typed protocol adapters with opportunistic helper extraction.

Each protocol adapter handles:

1. Request body construction.
2. Submit response parsing.
3. Poll response parsing.
4. Provider error envelope parsing.
5. Terminal output URL extraction.
6. Status normalization into `MediaJobStatus`.

The current Relaydance path can stay in one focused adapter file while it is the
only implemented gateway-style video provider. If BytePlus lands in this pass,
extract only the duplicated pieces that are obvious after both adapters exist:
remote JSON transport, safe response-shape summaries, status transition helper,
or artifact download/persistence. Do not create a generic trait or framework
just to prepare for future providers.

Rejected alternatives:

- Add broad fallback checks for every plausible task field, such as `result.id`,
  camelCase variants, or unverified nested paths. This is fast but guessy, and
  it can turn provider error envelopes into misleading task states. A parser may
  support only fields verified by a captured fixture or provider protocol
  documentation cited in the implementation notes.
- Add YAML response mappings such as `id_path`, `status_path`, and `url_path`.
  This moves protocol logic into strings, delays validation to runtime, and
  makes every provider descriptor a partial parser.
- Build a fully generic video provider engine or shared lifecycle framework
  before BytePlus is verified. Puffer does not yet have enough real video
  protocols to justify that abstraction.

## Adapter Naming

Use `relaydance_video` as the execution id in this pass.

Do not keep `openai_video`, and do not introduce `newapi_video` yet. The current
evidence is one Relaydance integration, so a provider-specific adapter name is
more honest and avoids overclaiming a cross-provider protocol. If a second
provider later proves it uses the same verified envelope, introduce a separate
rename/refactor spec for a shared protocol adapter.

The old `openai_video` enum variant, adapter id, tests, and bundled provider
resource should be replaced rather than aliased.

## Existing Provider Video Onboarding

This pass should connect current providers only when Puffer can execute their
video protocol.

| Provider | This pass | Reason |
| --- | --- | --- |
| `relaydance` | Yes | Current configured provider fails at poll parsing; fix and verify this path first. |
| `byteplus` | Conditional | The local discovery cache contains Seedance/Dreamina video model ids. BytePlus is the first non-Relaydance candidate, but it stays audit-only if probe, credentials, cost, or protocol evidence is insufficient. |
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

If BytePlus requires a different body layout and the probe succeeds, create a
`byteplus_video` adapter that maps the same scalar descriptor parameters into
that body. Do not encode BytePlus request structure through a general-purpose
YAML template.

For BytePlus, the candidate endpoint family is the ModelArk contents generation
task API:

- Submit: `POST /api/v3/contents/generations/tasks`
- Poll: `GET /api/v3/contents/generations/tasks/{id}`

The implementation must still capture redacted submit and poll fixtures before
declaring any BytePlus model in bundled resources.

## Runtime Data Flow

`VideoGeneration` remains a thin tool over exact media generation:

```text
VideoGeneration
  -> generate_exact_media_with_cache(kind = video)
  -> validate_media_generate_selection
  -> video adapter match
  -> protocol adapter builds submit request
  -> adapter saves queued job
  -> adapter polls until terminal
  -> protocol adapter extracts terminal output URL
  -> adapter downloads and persists artifact
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

Pure JSON parser helpers do not need provider context. Add provider, adapter,
and task id at the adapter boundary, where the local job and provider id are
available.

## Tests

Add tests before implementation.

Adapter lifecycle tests:

- Submit saves a queued job with `providerJobId`.
- Poll running updates the existing job.
- Poll completed downloads one MP4 artifact.
- Poll failed saves a failed job with provider error text.
- Poll parser failure after submit marks the job failed.
- Bounded polling still stops after the configured attempt count.
- Shared helper tests are required only for helpers actually extracted during
  the BytePlus step.

Relaydance adapter tests:

- Submit parses the verified live fixture.
- Poll running parses the verified live fixture.
- Poll completed extracts the verified output URL location.
- 2xx provider error envelope reports the provider message instead of
  `missing id`.
- `resources/providers/relaydance.yaml` uses the new adapter id.
- Old `openai_video` descriptors are rejected.

BytePlus adapter tests, only if the BytePlus probe passes:

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

Before coding parser changes that depend on provider behavior, capture redacted
provider evidence with stored credentials:

1. Poll the existing Relaydance task id and save a redacted fixture.
2. Submit a minimal Relaydance text-to-video request only if the existing task
   no longer provides enough evidence and the user approves the network/cost
   tradeoff.
3. Query BytePlus model discovery and identify the exact text-to-video model ids
   to declare.
4. Submit and poll one minimal BytePlus text-to-video task, saving redacted
   submit and poll fixtures, only if credentials are available and the user
   approves the network/cost tradeoff.

Do not print API keys. Fixtures must not include bearer tokens or signed output
URLs unless the URL is already temporary and safe to store; otherwise replace
the URL with a stable redacted stand-in while preserving its field location.

If any probe cannot run, the implementation must still fix Relaydance using the
best available evidence and keep the unverified provider in audit-only status.

## Stability And Performance

- Capability listing remains local and descriptor-driven.
- Video generation remains one remote task per tool call.
- Polling stays bounded and synchronous in this pass.
- Helper extraction is limited to duplication observed across implemented
  adapters.
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
- BytePlus ModelArk contents generation task docs:
  `https://docs.byteplus.com/en/docs/modelark/1520757` and
  `https://docs.byteplus.com/en/docs/modelark/1521309`
- Local failed job:
  `/Users/zhangxiao/.puffer/media/jobs/296201e7-47a2-473b-bfb4-6af80f671ba5.json`
- Local failing session:
  `/Users/zhangxiao/.puffer/sessions/c6ad17e3-7444-48a5-ba32-0c92bc89788b.session.jsonl`
