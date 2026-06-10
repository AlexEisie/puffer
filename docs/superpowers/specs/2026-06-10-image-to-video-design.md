# Image-to-Video Input Design

Date: 2026-06-10

## Status

Approved design direction. This document is a design artifact only; it does not
change implementation behavior.

## Goal

Add image-to-video (i2v) support to the exact video generation runtime so that
**any model that declares image input is usable**, through a single shared input
field. BytePlus Seedance is the first end-to-end implementation; Relaydance is
folded into the same abstraction now so it requires no redesign later — only its
provider-specific body serialization differs.

The design optimizes for long-term stability, low runtime cost, and a minimal
surface. Backward compatibility with existing saved video settings or job
sidecars is **not** a requirement. Speculative generality is explicitly avoided:
the model is a single optional first-frame image, not a frame list.

Scope of this iteration:

- **First-frame i2v (single image)**, end-to-end, through the **agent
  `VideoGeneration` tool**.
- Shared runtime field + capability declaration + BytePlus serialization.
- Relaydance consumes the same field; its body mapping is wired once the gateway
  request field name is confirmed.

Out of scope (see Non-goals): first+last frame, Replicate i2v, desktop/CLI UI
wiring, local image hosting.

## Background: current text-to-video flow

```
VideoGeneration tool
  -> VideoGenerationInput { prompt, parameters, purpose }   (deny_unknown_fields)
     crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs:25
  -> build_video_request: provider/model/operation/adapter from config.media.video
  -> ExactMediaGenerationRequest { kind, provider_id, model_id, operation,
     adapter, prompt, parameters, count }                   media_runtime.rs:76
  -> generate_exact_video_from_media_request                media_runtime_video.rs:31
       - validate_media_generate_selection (resolver.rs:97) validates capability + params
       - dispatch by adapter string: replicate_video / relaydance_video / byteplus_video
            - XxxVideoRequest::request_body() -> submit -> poll_until_terminal
              -> download_bytes -> write artifact
```

Today the only input is the text `prompt`. BytePlus sends
`content: [{type:"text", text}]` (byteplus_video.rs:26). Relaydance sends a flat
`{model, prompt, n, metadata}` body (relaydance_video.rs:38).

Two intentional guards must be opened deliberately:

1. `VideoGenerationInput` uses `deny_unknown_fields`, the tool schema
   `resources/tools/video_generation.yaml` sets `additionalProperties: false`,
   and its description explicitly states the tool "does not accept existing
   images, reference images, first frames, or last frames". A test
   (`rejects_unknown_video_generation_fields`, video_generation.rs:534) asserts
   the rejection. All four must be updated together.
2. The image cannot be modeled as a `MediaParameterSpec`: those are select-only
   with an enumerated `values` allow-list, and `validate_parameter_values`
   (resolver.rs:411) rejects any free-form value. The image must be a
   first-class request field.

## Design

### 1. Shared request field

The shared seam is a single normalized URL on the runtime request. Ingestion
produces it; every video adapter consumes it. There is no `ImageSource` enum and
no frame list — a normalized URL string covers both the inline and remote cases,
and a single optional image covers first-frame i2v.

```
ExactMediaGenerationRequest.image_url: Option<String>
```

- Inline local file -> a `data:<mime>;base64,<...>` URL built during ingestion.
- `http(s)://` or `data:` input -> passed through unchanged (ModelArk accepts
  data URLs natively).
- An adapter that needs bare base64 strips the `data:` prefix itself.

This single field also fully serves Relaydance (which likewise takes one
first-frame image), so folding Relaydance in requires no shared-layer change.

### 2. Capability modeling: input-modality declaration

Following mainstream practice (OpenAI / Replicate / ModelArk treat image as an
input modality of generation, not a separate operation), `MediaOperation` stays
`Generate`. The model declares its accepted image input instead, avoiding an
operation-enum explosion.

Add to `MediaModelDescriptor`
(crates/puffer-provider-registry/src/model.rs:383):

```
#[serde(default)]
image_input: Option<ImageInputSpec>

struct ImageInputSpec {
    required: bool,   // true = pure i2v model that cannot run without an image
}
```

`required: bool` is the only field. Frame count and per-frame role are not
modeled because they serve first+last frame, which is a non-goal; they attach
here later if needed.

`resolve_video_capabilities` (resolver.rs:265) surfaces `image_input` into
`MediaCapability` (capabilities.rs:17) so the tool, desktop, and agent can tell
whether a model accepts (or requires) an image.

### 3. Ingestion (agent tool input layer)

In `video_generation.rs`, mirror the existing `prompt_text` pattern
(video_generation.rs:188):

- Add `image: Option<String>` to `VideoGenerationInput`.
- Resolve the value:
  - `data:` or `http(s)://` -> pass through as `image_url`.
  - otherwise treat as a path relative to `cwd`: reuse `safe_relative_path`
    (video_generation.rs:210) for traversal safety, then `std::fs::read`. A path
    under `.puffer/media/images/...` is a normal file read, so
    generated-image -> video chaining works with no extra logic.
- Enforce a raw-size limit before encoding (source image <= 8 MB; base64 inflates
  ~33%, keeping bodies bounded). Reference: files.rs `WRITE_HARD_MAX_BYTES = 5 MB`,
  `READ_HARD_MAX_BYTES = 24 MB`.
- Detect MIME by reusing the existing `detect_image_format` (images_json.rs:407,
  recognizes png/jpeg/webp). Extract it to a shared module
  (e.g. `runtime/media/http_support.rs` or a small image util) so both the
  images_json output path and ingestion use one implementation. Unknown
  signature -> error.
- Build `data:<mime>;base64,<...>` using the existing `base64` dependency
  (minimax_image.rs:14, chat_image_output.rs:14) and set `image_url`.

The base64 dependency is already in the workspace; no new crates are added.

### 4. Validation

Image-modality validation lives in `generate_exact_video_from_media_request`
(media_runtime_video.rs), after capability resolution, before any HTTP call:

- model `image_input.required == true` but `image_url` is `None` -> error.
- model has no `image_input` but `image_url` is `Some` -> error.

Errors carry provider/model context. The image never flows through
`validate_parameter_values`.

### 5. Per-provider serialization seam (the only per-provider difference)

The runtime threads `image_url: Option<&str>` into each adapter request builder
uniformly; each adapter maps it to its own wire format. This is the same seam
where `request_body()` already differs per adapter, not a redesign.

- **BytePlus (implemented this iteration):** when `image_url` is set, append to
  `content[]`: `{ "type": "image_url", "image_url": { "url": <image_url> } }`.
  Changes: add an `image_url: Option<String>` field to `BytePlusVideoRequest`
  (byteplus_video.rs:18) and extend `request_body()` (byteplus_video.rs:26). The
  `generate_audio: false` default and ordered param expansion are preserved.

- **Relaydance (same field, deferred serialization):** flat body. The
  request-side image field name must be confirmed from the New API gateway
  `dto/relaydance_video.go` **request struct** (the response side was confirmed
  from the same file, relaydance_video.rs:153; the request side is not in this
  repo). Once confirmed, fill it into `RelaydanceVideoRequest` /
  `request_body()` (relaydance_video.rs:38) — roughly 10 lines, touching no
  shared layer.

- **Replicate:** per-model-version input schemas differ; not implemented this
  iteration. The shared field is already passed to its builder.

**Only external dependency:** the Relaydance image field name. It does not affect
the design or implementation of the shared field; it gates only Relaydance's
~10-line serialization and can be confirmed in parallel.

### 6. Request build sites (gap: there are three)

`ExactMediaGenerationRequest` is constructed in three places. Adding `image_url`
forces all three to compile, but only the agent tool populates it this iteration:

- Agent tool: `video_generation.rs:120` (`exact_media_request`) — **populated**
  from ingestion.
- Desktop: `backend.rs:3769` (`exact_media_generation_request_from_stored`) —
  sets `image_url: None`. Desktop UI wiring is a follow-up.
- CLI daemon: `daemon.rs:2498` (`exact_media_generation_request`) — sets
  `image_url: None`. CLI wiring is a follow-up.

`#[serde(default)]` on the new struct field keeps deserialization of older
payloads infallible. Setting `None` in desktop/CLI is mechanical and is not a
redesign; their UI/param surfaces opting in later reuse the same field.

### 7. Persistence and polling

No `MediaJob` change. The image is needed only at submit time; polling, artifact
download, and orphan reclaim (media_runtime_video.rs:155) depend only on
`provider_job_id` and the response video URL. The image is deliberately not
persisted to the job sidecar (avoids storing multi-MB data URLs and adds no
field).

### 8. Error handling summary

- Ingestion: missing file / over-size / unrecognized MIME -> explicit error
  before the provider call (parity with `prompt_text`).
- Path traversal: reuse `safe_relative_path`.
- Modality mismatch: capability-stage error with provider/model context.
- Secret redaction path (media_runtime_video.rs) is unchanged.

### 9. Testing

- Ingestion: local image -> data URL; URL pass-through; over-size rejection;
  MIME sniffing; path-traversal rejection.
- BytePlus `request_body()`: includes the `image_url` content item; coexists with
  `generate_audio: false` and param expansion; omits the item when no image.
- Capability: a model declaring `image_input` surfaces the spec;
  required-but-missing rejected; image-but-not-supported rejected.
- Tool schema: `resources/tools/video_generation.yaml` gains the `image`
  property and an updated description; `video_tool_schema.rs` and
  `rejects_unknown_video_generation_fields` are updated to assert acceptance.
- Reuse the existing scripted transports for submit/poll regression
  (byteplus_video.rs tests_support, relaydance_video.rs tests_support).

## Affected files

**Shared (provider-agnostic):**

- `crates/puffer-core/media_runtime.rs` — `ExactMediaGenerationRequest.image_url`.
- `crates/puffer-provider-registry/src/model.rs` — `ImageInputSpec`,
  `MediaModelDescriptor.image_input`.
- `crates/puffer-core/runtime/media/resolver.rs` +
  `crates/puffer-core/runtime/media/capabilities.rs` — surface + validate.
- `crates/puffer-core/media_runtime_video.rs` — modality validation, thread
  `image_url` into adapter request builders.
- `crates/puffer-core/runtime/media/images_json.rs` (+ shared util) — extract
  and reuse `detect_image_format`.
- `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs` —
  `image` input field, ingestion, populate `image_url`.
- `resources/tools/video_generation.yaml` — schema `image` property + description.
- `crates/puffer-resources/tests/video_tool_schema.rs` — schema test.

**Build sites set to `None` (mechanical):**

- `apps/puffer-desktop/src-tauri/src/backend.rs:3769`.
- `crates/puffer-cli/src/daemon.rs:2498`.

**Per-provider seam:**

- `crates/puffer-core/runtime/media/byteplus_video.rs` — implemented this iteration.
- `crates/puffer-core/runtime/media/relaydance_video.rs` — wired once the gateway
  request field name is confirmed.

## Non-goals

- First+last frame (single optional image only; `ImageInputSpec` and `image_url`
  extend to it later).
- Replicate i2v.
- Desktop and CLI i2v UI/param wiring (their constructors pass `None`).
- Local image hosting / provider file-upload APIs (inline base64 / URL
  pass-through only).
- Extracting a unified `VideoAdapter` trait (existing string dispatch suffices).
