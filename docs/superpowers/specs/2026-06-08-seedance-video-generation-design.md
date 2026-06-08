# Seedance Video Generation — Design

**Date:** 2026-06-08
**Status:** Approved (design)
**Branch:** feat/chat-image-generation

## Problem

The desktop "Video generation settings" modal shows no available provider or
model. Root cause (verified): **no provider resource declares a `media.video`
section**. Video capability discovery (`resolve_video_capabilities`,
`crates/puffer-core/runtime/media/resolver.rs`) emits a capability only when a
provider's registry descriptor has `media.video` whose execution adapter
resolves to a supported video `MediaExecutionKind`. Every shipped provider YAML
declares only `media.image`. The video runtime skeleton (submit → poll →
artifacts) exists and is tested via the `replicate_video` adapter, but no real
video provider is wired.

### Provider audit (code side × reality side)

- 18 providers; 8 declare `media:` (all image-only); **0 declare `media.video`**.
- Only video execution adapter implemented: `replicate_video` (Replicate-style
  async `POST /v1/predictions` → `GET` poll). No real provider uses it.
- Reality side: 7 providers' real APIs support video (byteplus/Seedance,
  minimax/Hailuo ×2, zhipu/CogVideoX, xai/Grok, openai/Sora, openrouter). All
  use async submit→poll; none is OpenAI-compatible for video. The remaining
  providers are text-only inference or local runtimes and will never have video.

## Decision

Wire up **BytePlus Seedance**, direct to ModelArk, via a dedicated
`seedance_video` execution adapter — **fully symmetric with how image
generation is already handled** (byteplus direct + `images_json` adapter +
structured params declared in YAML).

Rationale (constraints: no backward compat, long-term ROI, stability,
performance, avoid over-engineering):

- The image path is the proven prescription: direct-to-provider +
  per-provider adapter + YAML-declared structured params. Mirroring it yields a
  video path structurally identical to existing code — lowest cognitive cost,
  most stable, least over-designed.
- BytePlus is already the configured image provider (`byteplus.yaml`,
  `base_url: https://ark.ap-southeast.bytepluses.com/api/v3`). Seedance video
  lives under the **same base_url** (`/contents/generations/tasks`). No new
  provider, no new auth, no gateway layer.
- OpenRouter would cover more models with one adapter but introduces a gateway
  dependency inconsistent with the current self-hosted direct-connect baseline.
  Rejected for first cut.

### Explicit non-goals (anti-over-engineering)

- No generic "video adapter" trait abstraction — only one video provider exists
  (YAGNI). Keep the existing per-adapter match-arm dispatch in
  `generate_exact_video_from_media_request`.
- Do not remove or change `replicate_video` (tested, no provider references it,
  serves as a second reference; removing it is unrelated cleanup).
- Do not touch any image-generation code path.
- No defensive handling for ModelArk concurrency limits (3 tasks / QPS 2) — a
  single `/video` command submits one task; `validate_video_count` already caps
  count at 1.

## Architecture

| | Image (existing) | Video (this design) |
|---|---|---|
| Provider | byteplus, direct ModelArk | same byteplus, same base_url |
| YAML | `media.image` + `adapter: images_json` | new `media.video` + `adapter: seedance_video` |
| Adapter module | `images_json` | new `seedance_video.rs` (reuses `http_support` plumbing like `images_json`; reuses `replicate_video` async job lifecycle) |
| Protocol | sync `POST /images/generations` | async `POST /contents/generations/tasks` → poll `/tasks/{id}` |
| Param mapping | structured → request fields | structured → prompt-inline `--ratio/--duration/--resolution` (in adapter) |

## Components

### 1. `byteplus.yaml` — add `media.video`

```yaml
media:
  image: { ... }            # unchanged
  video:
    discovery:
      adapter: static
    execution:
      adapter: seedance_video
      path: /contents/generations/tasks
    models:
      # Start with ONE verified model. Add the Fast / other variants as further
      # list entries once confirmed against the account (see Verification
      # prerequisites). Exact id/values below are research defaults, pending
      # confirmation.
      - id: dreamina-seedance-2-0-260128
        display_name: Seedance 2.0
        operations: [generate]
        parameters:
          - { name: resolution, label: Resolution,  values: [480p, 720p, 1080p], default: 1080p, request_field: resolution }
          - { name: ratio,      label: Aspect ratio, values: ["16:9","9:16","1:1"], default: "16:9", request_field: ratio }
          - { name: duration,   label: Duration,     values: ["5","10"],          default: "5",    request_field: duration }
```

`request_field` reuses the existing schema; for video its semantics =
the `--` flag name. The adapter builds `--resolution 1080p --ratio 16:9
--duration 5`. UI, capability resolution, and parameter validation all flow
through existing logic — no new frontend.

### 2. `crates/puffer-core/runtime/media/seedance_video.rs` (new)

**Reuses the image path's HTTP plumbing (`http_support.rs`); reuses the
`replicate_video` async job *lifecycle*.** This is the key refinement: the
image adapter (`images_json`) does NOT hardcode its base URL/path — it builds
the URL from `provider.base_url` + `execution.path` via
`provider_execution_url()`, authenticates via `bearer_token()`, downloads via
`download_image_url()`, and redacts via `provider_error_secrets()` /
`redact_secrets()`. `replicate_video` is the odd one out (hardcoded
`DEFAULT_REPLICATE_BASE_URL` + `/v1/predictions`) because Replicate has one
fixed host. Seedance runs on byteplus's own `base_url`, so it MUST use the
shared `http_support` helpers — which is also less code.

- `SeedanceVideoTransport` trait: `submit_task(url, key, body) -> {id}`,
  `poll_task(url, key) -> Value`, **`download_bytes(url) -> Vec<u8>`**. URLs are
  passed in (built by the adapter via `provider_execution_url`), not constructed
  inside the transport. Download goes through the trait (mirroring
  `replicate_video`) so unit tests use a fake transport with canned bytes — no
  real HTTP server / extra dev-dependency.
- `ReqwestSeedanceVideoTransport`: blocking `reqwest` POST/GET with
  `Authorization: Bearer`; its `download_bytes` delegates to the shared
  `http_support::download_image_url` (https/loopback enforcement reused).
- `SeedanceVideoAdapter`: built with `(api_token, submit_url)` where
  `submit_url = provider_execution_url(provider, execution, "...")`; poll URL =
  that URL joined with `/{task_id}`. Lifecycle mirrors `replicate_video`:
  `submit()` creates a queued `MediaJob` (`MediaKind::Video`, task id stored in
  `provider_job_id`), `poll_until_terminal()` loops with `SeedancePollingConfig`,
  status normalization (`succeeded` → done; `failed`/`expired` → error with
  ModelArk code/message), then `transport.download_bytes()` fetches the MP4 and
  `MediaGenerationService` persists it (same artifact path scheme as images).
- `seedance_request_from_parameters()`: the only genuinely new logic — maps
  structured params into the prompt-inline `--` string and assembles the
  `content` array. Isolated and unit-testable.

**Reuse, do not duplicate:** the production transport's `download_bytes`
delegates to `download_image_url` (already enforces https/loopback, is
content-agnostic) — do not rename it (renaming churns the image path for no
behavior change). `MediaJob` is reused unchanged (no model edits) — replicate
already proves it carries video jobs. `MediaCapabilityParameter.request_field`
is `Option<String>`: only parameters with a `request_field` become ModelArk
flags.

### 3. Wiring points (all small)

1. `crates/puffer-provider-registry/src/model.rs` (`MediaExecutionKind` enum):
   add `SeedanceVideo` (serde snake_case → `"seedance_video"`).
2. `crates/puffer-core/runtime/media/resolver.rs`:
   (a) `execution_adapter_is_available_for_kind`: add
   `(MediaKind::Video, MediaExecutionKind::SeedanceVideo)`; (b) `adapter_id`
   mapping; (c) add `resolve_video_execution_descriptor` — a small sibling of
   the existing `resolve_image_execution_descriptor` that reads `media.video`
   (needed so the match arm can obtain `base_url` + `execution.path`).
3. `crates/puffer-core/runtime/media/mod.rs`: add
   `pub(crate) mod seedance_video;`.
4. `crates/puffer-core/media_runtime.rs`
   (`generate_exact_video_from_media_request`): add match arm
   `"seedance_video" => { ... }` — resolve provider + video execution descriptor,
   `bearer_token`, construct `SeedanceVideoAdapter`, submit, poll, load
   artifacts. Symmetric with the existing `replicate_video` arm.

## Data flow

`/video <prompt>` → backend/daemon `generate_media_job("video", …)` →
`generate_exact_media_with_cache` → `generate_exact_video_from_media_request` →
`validate_media_generate_selection` (existing param validation) →
`seedance_video` arm → `SeedanceVideoAdapter.submit()` → ModelArk task id →
`poll_until_terminal()` → `content.video_url` → download MP4 →
`MediaGenerationService` persists artifact (same path scheme as images).

## Error handling & stability

- **Bounded polling:** `SeedancePollingConfig` (interval ~3s, generous
  minute-scale total timeout, since video is slow) — a tiny module-local config,
  mirroring `replicate_video`'s `poll_until_terminal` pattern. No new mechanism.
- **Status normalization:** `failed`/`expired` → clear error carrying ModelArk
  error code/message; never silent.
- **Download validation:** reuse `download_image_url` (already enforces
  https/loopback); persist via `MediaGenerationService`.
- **Secret redaction:** reuse `provider_error_secrets` + `redact_secrets` so a
  ModelArk error body never leaks the API key — same as the image path.
- **Count:** `validate_video_count` already enforces count == 1.

## Verification prerequisites (before/during implementation)

These came from web research and MUST be confirmed against the actual ModelArk
account before the YAML is considered final:

- **Model id(s):** confirm the exact Seedance model id(s) available on the
  account (e.g. `dreamina-seedance-2-0-260128`). Start with **one** verified
  model; the Fast variant and any others are a trivial YAML list addition once
  confirmed — do not declare unverified ids.
- **Parameter names & values:** confirm whether resolution/ratio/duration are
  expressed as prompt-inline `--` flags (Seedance 2.0) and the allowed value
  sets, since the capability `values` drive UI options and pre-flight
  validation.
- **Submit/poll path & response shape:** confirm `execution.path`
  (`/contents/generations/tasks`), the task-id field on submit, and the
  terminal status string + `video_url` location on poll.

## Testing

Mirror existing `replicate_video` and `daemon.rs` cases — no new test paradigm:

- `seedance_video.rs` unit tests with a fake transport: submit saves queued job;
  poll downloads completed MP4 artifact; `seedance_request_from_parameters`
  builds the correct prompt-inline `--` string and `content` array; `failed`
  status surfaces ModelArk error.
- daemon/backend tests: capability discovery returns the Seedance video
  capability; `generate_media` rejects a stale/mismatched adapter with a clear
  error.

## Out of scope

- Image-to-video (first_frame / last_frame), reference images, audio.
- Other providers (minimax/zhipu/xai/openai/openrouter) — separate specs.
- Relaydance gateway routing — current image path is direct ModelArk; video
  mirrors that. Gateway is a later, orthogonal decision.
