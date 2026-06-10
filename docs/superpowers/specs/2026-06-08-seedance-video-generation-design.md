# Video Generation (Relaydance / OpenAI-video) — Design

**Date:** 2026-06-08
**Status:** Approved (design) — supersedes the earlier native-direct Seedance draft
**Branch:** feat/chat-image-generation

## Problem

The desktop "Video generation settings" modal shows no provider or model. Root
cause (verified): **no provider resource declares a `media.video` section**.
Video capability discovery (`resolve_video_capabilities`,
`crates/puffer-core/runtime/media/resolver.rs`) emits a capability only when a
provider's descriptor has `media.video` whose execution adapter resolves to a
supported video `MediaExecutionKind`. Every shipped provider YAML declares only
`media.image`. The async video runtime skeleton (submit → poll → artifacts)
exists and is tested via `replicate_video`, but no real video provider is wired.

## Decision

Wire video generation **exactly like image generation does it**: a **gateway
provider declared in YAML + one generic execution adapter**, not a per-vendor
native integration.

- Add **Relaydance** (`relaydance.com`, a New API / one-api OpenAI-compatible
  gateway) as a normal provider resource (`relaydance.yaml`), the same way
  `vercel-ai-gateway.yaml` is a provider. It already proxies Seedance video.
- Add **one generic `openai_video` execution adapter** that speaks the
  OpenAI-compatible async video shape (`POST /v1/video/generations` →
  `GET /v1/video/generations/{id}`). This shape is the emerging cross-vendor
  standard (OpenAI `/videos`, LiteLLM, New API all implement it).
- Every video model — the 9 Seedance variants today, Kling/Vidu/Sora later — is
  then a **pure-YAML list entry** under `media.video`, exactly like image models
  under `media.image` with `images_json`. New model = one YAML block, no code.

### Why this over native-direct (rationale)

Industry references (LiteLLM, Vercel AI SDK, New API) converge on **one stable
internal interface + per-shape adapter**, with a gateway as an optional channel.
For our self-hosted desktop client the gateway (`relaydance`) is already
provisioned and proxies Seedance; routing through it gives:

- **Image-parity ergonomics:** one adapter, N models as YAML — the property the
  user explicitly wants ("像生图一样").
- **One adapter covers many vendors:** the OpenAI-video shape is vendor-neutral,
  so Kling/Vidu/Sora onboard as YAML later with zero new code.
- **Reuse of the proven async lifecycle:** `MediaJob` + bounded polling +
  `http_support` plumbing are reused unchanged (`replicate_video` already proves
  `MediaJob` carries video jobs).

### Scope: v1 = text-to-video only (verified-minimal)

**v1 covers text-to-video (t2v) + scalar parameters.** This is the entire clean,
config-driven surface and needs **zero new plumbing** — it maps onto the
existing text-prompt + scalar-`request_field` request model precisely as image
generation does.

**Image inputs (i2v, reference images, first/last frame) are explicitly v2.**
Verified gap (with file:line): the media pipeline is **text-prompt + scalar-
parameters only end to end** — `GenerateMediaParams{kind, prompt, count}`
(`daemon.rs:127`), `ExactMediaGenerationRequest{prompt, parameters:
BTreeMap<String,String>, ...}` (`media_runtime.rs:72`), `MediaCapabilityParameter
{values: Vec<String>, ...}` (`capabilities.rs:34`), and no adapter accepts an
input image (`image_url` appears only when parsing **responses**). Supporting
first/last frame requires a new image-input subsystem spanning frontend file
picker → TS types → daemon params → request DTO → parameter model → adapter
`metadata.content[]` with role tags. That is an independent subsystem and gets
its **own spec/plan**; forcing it into v1 is the over-engineering this design
rejects.

### Explicit non-goals (anti-over-engineering)

- **No generic multi-gateway abstraction.** One `openai_video` adapter for the
  OpenAI-video shape; that is sufficient and reusable. No "describe any vendor in
  config" engine (YAGNI; rule-of-three not met).
- **No `content[]` / `content_role` / `metadata.content` machinery in v1.** t2v
  has no role-tagged inputs, so none is needed. It belongs to the v2 image-input
  subsystem.
- **No image-input plumbing in v1** (frontend picker, request-DTO image fields,
  param-model extension) — deferred to v2.
- Do not touch image-generation code paths.
- Do not remove or change `replicate_video` (tested; serves as a second async
  reference). It stays.
- No defensive handling for gateway concurrency limits — one `/video` command
  submits one task; `validate_video_count` already caps count at 1.

## Architecture

| | Image (existing) | Video (this design, v1) |
|---|---|---|
| Provider | YAML resource, e.g. `vercel-ai-gateway` | new `relaydance` YAML resource |
| YAML | `media.image` + `adapter: images_json` | `media.video` + `adapter: openai_video` |
| Adapter module | `images_json` | new `openai_video.rs` (reuses `http_support` plumbing like `images_json`; reuses `replicate_video` async job lifecycle) |
| Protocol | sync `POST /images/generations` | async `POST /v1/video/generations` → poll `GET /v1/video/generations/{id}` |
| Param mapping | scalar → top-level request field | scalar → top-level field **or** `metadata.<key>` via dotted `request_field` |
| New model | YAML list entry | YAML list entry |

## Components

### 1. `resources/providers/relaydance.yaml` (new)

A normal provider resource (auto-discovered from `resources/providers/`; no code
registers it — same as every other YAML).

```yaml
id: relaydance
display_name: Relaydance
base_url: https://relaydance.com
default_api: openai-completions
auth_modes:
  - api_key
discovery:
  path: /v1/models
  response: open_ai_models
  api: openai-completions
  context_window: 128000
  max_output_tokens: 16384
  supports_reasoning: false
media:
  video:
    discovery:
      adapter: static
    execution:
      adapter: openai_video
      path: /v1/video/generations
    models:
      # Start with ONE verified model id (see Task 0). Add the other 8 Seedance
      # variants (and later Kling/Vidu/Sora) as further list entries — pure YAML.
      - id: doubao-seedance-2-0-720p
        display_name: Seedance 2.0 (720p)
        operations: [generate]
        parameters:
          - { name: duration,   label: Duration,     values: ["5","10"],            default: "5",    request_field: seconds }
          - { name: resolution, label: Resolution,   values: ["720p","1080p"],      default: "720p", request_field: metadata.resolution }
          - { name: ratio,      label: Aspect ratio, values: ["16:9","9:16","1:1"], default: "16:9", request_field: metadata.ratio }
```

`request_field` reuses the existing schema. **Dotted convention (the only new
adapter rule):** a `request_field` of `metadata.<k>` places the value under the
request body's `metadata` object; an undotted `request_field` places it at the
top level. UI, capability resolution, and parameter validation flow through
existing logic — **no frontend changes**.

### 2. `crates/puffer-core/runtime/media/openai_video.rs` (new)

Reuses the image path's HTTP plumbing (`http_support.rs`) and the
`replicate_video` async job *lifecycle*. Like `images_json`, it does **not**
hardcode base URL/path — it builds the URL from `provider.base_url` +
`execution.path` via `provider_execution_url()`, authenticates via
`bearer_token()`, downloads via `download_image_url()`, redacts via
`provider_error_secrets()` / `redact_secrets()`.

- `OpenAiVideoRequest { model, prompt, params: Vec<(request_field, value)> }`;
  `request_body()` builds `{ model, prompt, n: 1, <top-level params>, metadata: {
  <metadata.* params> } }` (omit `metadata` if empty). The dotted-`request_field`
  split is the only genuinely new logic — isolated and unit-tested.
- `OpenAiVideoTransport` trait: `submit_task(url, key, body) -> Value`,
  `poll_task(url, key) -> Value`, `download_bytes(url) -> Vec<u8>`. URLs are
  passed in (built by the adapter), not constructed in the transport, so unit
  tests use a fake transport with canned JSON/bytes — no real HTTP server.
- `ReqwestOpenAiVideoTransport`: blocking `reqwest` POST/GET with
  `Authorization: Bearer`; `download_bytes` delegates to the shared
  `http_support::download_image_url` (https/loopback enforcement reused).
- `OpenAiVideoTask::from_value`: parses the OpenAI-video envelope (verified from
  New API `dto/openai_video.go`): `id`, `status` ∈
  `queued|in_progress|completed|failed`, video URL at **`metadata.url`**,
  `error.{message,code}`. `media_status()` maps to `MediaJobStatus`
  (`queued`→Queued, `in_progress`→Running, `completed`→Succeeded,
  `failed`→Failed).
- `OpenAiVideoAdapter`: built with `(api_token, submit_url)` where `submit_url =
  provider_execution_url(provider, execution, …)`; poll URL = `submit_url/{id}`.
  Lifecycle mirrors `replicate_video`: `submit()` creates a queued `MediaJob`
  (`MediaKind::Video`, task id in `provider_job_id`), `poll_until_terminal()`
  loops with `OpenAiVideoPollingConfig`, on `completed` downloads `metadata.url`
  and `MediaGenerationService` persists the MP4 (same artifact scheme as images).

**Reuse, do not duplicate:** the production transport's `download_bytes`
delegates to `download_image_url` (content-agnostic, already enforces
https/loopback) — do not rename it. `MediaJob` is reused unchanged.
`MediaCapabilityParameter.request_field` is `Option<String>`; only parameters
with a `request_field` are emitted.

### 3. Wiring points (all small)

1. `crates/puffer-provider-registry/src/model.rs` (`MediaExecutionKind`): add
   `#[serde(rename = "openai_video")] OpenAiVideo` (snake_case would otherwise
   yield `open_ai_video`).
2. `crates/puffer-core/runtime/media/resolver.rs`: (a)
   `execution_adapter_is_available_for_kind` add `(Video, OpenAiVideo)`; (b)
   `adapter_id` mapping → `"openai_video"`; (c) add
   `resolve_video_execution_descriptor` (sibling of
   `resolve_image_execution_descriptor`, reading `media.video`).
3. `crates/puffer-core/runtime/media/mod.rs`: add `pub(crate) mod openai_video;`.
4. `crates/puffer-core/media_runtime.rs`
   (`generate_exact_video_from_media_request`): add `"openai_video"` match arm —
   resolve provider + video execution descriptor, `bearer_token`, build
   `OpenAiVideoAdapter`, submit, poll, load artifacts. Symmetric with the
   existing `replicate_video` arm.

## Data flow

`/video <prompt>` → daemon `generate_media_job("video", …)` →
`generate_exact_media_with_cache` → `generate_exact_video_from_media_request` →
`validate_media_generate_selection` (existing param validation) → `openai_video`
arm → `OpenAiVideoAdapter.submit()` → gateway task id →
`poll_until_terminal()` → `metadata.url` → download MP4 →
`MediaGenerationService` persists artifact (same scheme as images).

## Error handling & stability

- **Bounded polling:** `OpenAiVideoPollingConfig` (interval ~3s, minute-scale
  total timeout) — module-local, mirroring `replicate_video`.
- **Status normalization:** `failed` → clear error carrying the gateway
  `error.message`/`code`; unknown status → explicit error; never silent.
- **Download validation:** reuse `download_image_url` (https/loopback enforced).
- **Secret redaction:** reuse `provider_error_secrets` + `redact_secrets` so a
  gateway error body never leaks the API key — same as the image path.
- **Count:** `validate_video_count` already enforces count == 1.

## Verification prerequisites (Task 0 — before YAML is final)

Confirm against a live Relaydance key:

- **Model id(s):** exact id(s) from `GET /v1/models` (memory lists e.g.
  `doubao-seedance-2-0-720p`, `doubao-seedance-2-0-1080p`,
  `seedance-1-5-pro-with-audio`). Start with **one** verified id.
- **Param placement:** confirm which params are top-level (e.g. `seconds`) vs
  `metadata.*` (e.g. `resolution`, `ratio`) in `POST /v1/video/generations`, and
  the allowed value sets (they drive UI options + pre-flight validation).
- **Poll envelope:** confirm `GET /v1/video/generations/{id}` returns `status` ∈
  `queued|in_progress|completed|failed` and the video URL at `metadata.url`.

## Testing

Mirror existing `replicate_video` and `daemon.rs` patterns — no new paradigm:

- `openai_video.rs` unit tests with a fake transport: `request_body` splits
  top-level vs `metadata.*` correctly; submit saves a queued job; poll downloads
  the completed MP4 artifact; `failed` surfaces the gateway error; unknown status
  errors.
- daemon test: capability discovery returns the Relaydance video capability;
  `generate_media` rejects a stale/mismatched adapter with a clear error.

## Out of scope (v2 / separate specs)

- **Image inputs subsystem** → i2v, reference images, first/last frame
  (role-tagged `metadata.content[]`). Independent full-stack project.
- Native-direct BytePlus ModelArk routing (no gateway). Orthogonal; the dotted-
  `request_field` adapter + a second transport could add it later without a
  rewrite, but not now.
- Other providers' native video (minimax/zhipu/xai/openai) — onboard via the
  same `openai_video` adapter as YAML if they are gateway-proxied, else separate
  specs.
