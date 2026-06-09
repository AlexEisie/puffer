# Image-to-Video Input Design

Date: 2026-06-10

## Status

Approved design direction. This document is a design artifact only; it does not
change implementation behavior.

## Goal

Add image-to-video (i2v) support to the exact video generation runtime so that
**any model that declares image input is usable**, through a single shared input
pathway. BytePlus Seedance is the first end-to-end implementation; Relaydance is
folded into the same abstraction now so it requires no redesign later, only its
provider-specific body serialization.

The design optimizes for long-term stability, low runtime cost, and a small
surface. Backward compatibility with existing saved video settings or job
sidecars is **not** a requirement. Over-engineering is explicitly avoided: only
the irreducible per-provider body mapping differs between adapters.

Scope of this iteration: **first-frame i2v (single image)**, end-to-end. The
internal types reserve an optional frame `role` so first+last frame is a later
field-add, not a redesign. Replicate i2v is out of scope this iteration; the
shared interface is in place for it.

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

1. `VideoGenerationInput` uses `deny_unknown_fields`, and a test
   (`rejects_unknown_video_generation_fields`, video_generation.rs:534)
   currently rejects an image field. Adding the field makes it known.
2. The image cannot be modeled as a `MediaParameterSpec`: those are select-only
   with an enumerated `values` allow-list, and `validate_parameter_values`
   (resolver.rs:411) rejects any free-form value. The image must be a
   first-class request field.

## Design

### 1. Shared image input abstraction

A normalized, provider-agnostic image input is the contract that prevents future
redesign. Every shared layer (tool input, ingestion, capability declaration,
request struct, validation) operates on this type; only the final per-adapter
body serialization differs.

```
enum ImageSource {
    Inline { bytes: Vec<u8>, mime: String },   // local file / generated artifact, read into memory
    RemoteUrl(String),                          // http(s):// or data: URL, passed through
}

enum ImageRole { FirstFrame, LastFrame }        // reserved; first-frame is the default

struct MediaImageInput {
    source: ImageSource,
    role: Option<ImageRole>,                    // None = first frame default
}
```

Carried on the runtime request as a `Vec`, so first+last frame is a zero-struct
change later:

```
ExactMediaGenerationRequest.image_inputs: Vec<MediaImageInput>   // 1 entry this iteration
```

`Vec` (not `Option`) is deliberate: it admits the ordered first/last-frame case
without reshaping the request.

### 2. Capability modeling: input-modality declaration

Following mainstream practice (OpenAI / Replicate / ModelArk all treat image as
an input modality of generation, not a separate operation), `MediaOperation`
stays `Generate`. The model declares its accepted image input instead, avoiding
an operation-enum explosion as modalities combine (image-only / text+image /
first+last frame / future video-to-video).

Add to `MediaModelDescriptor`
(crates/puffer-provider-registry/src/model.rs:383):

```
image_input: Option<ImageInputSpec>

struct ImageInputSpec {
    required: bool,        // model cannot run without an image (pure i2v model)
    max_images: u8,        // 1 this iteration; >1 enables first+last later
    supports_role: bool,   // whether per-image role is meaningful
}
```

`resolve_video_capabilities` (resolver.rs:265) surfaces this into
`MediaCapability` (capabilities.rs:17) so the tool, desktop UI, and agent can
tell whether a model accepts (or requires) an image.

### 3. Ingestion (tool input layer)

In `video_generation.rs`, mirror the existing `prompt_text` pattern
(video_generation.rs:188):

- Add `image: Option<String>` to `VideoGenerationInput`; update the
  `deny_unknown_fields` rejection test to assert `image` is now accepted.
- Resolve the value:
  - `data:` or `http(s)://` -> `ImageSource::RemoteUrl` (pass through; ModelArk
    natively accepts data URLs).
  - otherwise treat as a path relative to `cwd`: reuse `safe_relative_path`
    (video_generation.rs:210) for traversal safety, then `std::fs::read`. A path
    under `.puffer/media/images/...` is just a normal file read, so the
    generated-image -> video chaining works with no extra logic.
- Enforce a raw-size limit before encoding (source image <= 8 MB; base64 inflates
  ~33%, keeping request bodies well bounded). Reference existing limits:
  files.rs `WRITE_HARD_MAX_BYTES = 5 MB`, `READ_HARD_MAX_BYTES = 24 MB`.
- Infer MIME by magic bytes (`FFD8`=jpeg, `89 50 4E 47`=png, `RIFF....WEBP`),
  falling back to extension.
- For inline sources the adapter builds the `data:<mime>;base64,<...>` URL using
  the existing `base64` dependency (minimax_image.rs:14, chat_image_output.rs:14).

### 4. Validation

Image-modality validation lives near `generate_exact_video_from_media_request`
(media_runtime_video.rs), after capability resolution:

- model `image_input.required == true` but no image provided -> error.
- model has no `image_input` but an image was provided -> error.
- `image_inputs.len() > image_input.max_images` -> error.

Errors are raised before any provider HTTP call and carry provider/model
context. Image input never flows through `validate_parameter_values`.

### 5. Per-provider serialization seam (the only per-provider difference)

The runtime threads `&[MediaImageInput]` into each adapter's request builder
uniformly; each adapter maps it to its own wire format. This is the same seam
where `request_body()` already differs per adapter today, not a redesign.

- **BytePlus (implemented this iteration):** append to the `content[]` array
  `{ "type": "image_url", "image_url": { "url": <url> }, [ "role": <role> ] }`.
  Changes: add an image field to `BytePlusVideoRequest` (byteplus_video.rs:18)
  and extend `request_body()` (byteplus_video.rs:26). The existing
  `generate_audio: false` default and ordered param expansion are preserved.

- **Relaydance (same abstraction, deferred serialization):** flat body. The
  request-side image field name must be confirmed from the New API gateway
  `dto/relaydance_video.go` **request struct** (the response side was confirmed
  from the same file, relaydance_video.rs:153; the request side is not in this
  repo). Once confirmed, fill it into `RelaydanceVideoRequest` /
  `request_body()` (relaydance_video.rs:38) — roughly 10 lines, touching no
  shared layer.

- **Replicate:** per-model-version input schemas differ; not implemented this
  iteration. The shared interface is already in place.

**Only external dependency:** the Relaydance image field name. It does **not**
affect the design or implementation of the shared abstraction; it gates only
Relaydance's ~10-line serialization and can be confirmed in parallel during
implementation.

### 6. Persistence and polling

`MediaJob` (jobs.rs:27) optionally records an image-input reference for
reproducibility (a lightweight descriptor, not the raw bytes). Polling, artifact
download, and orphan reclaim (media_runtime_video.rs:155) depend only on
`provider_job_id` and the response video URL; they do not resend the image and
are therefore unchanged.

### 7. Error handling summary

- Ingestion: missing file / over-size / unrecognized MIME -> explicit error
  before the provider call (parity with `prompt_text`).
- Path traversal: reuse `safe_relative_path`.
- Modality mismatch: capability-stage error with provider/model context.
- Secret redaction path (media_runtime_video.rs) is unchanged.

### 8. Testing

- Ingestion: local image -> data URL; URL pass-through; over-size rejection;
  MIME sniffing; path-traversal rejection.
- BytePlus `request_body()`: includes the `image_url` item and optional `role`;
  coexists with `generate_audio: false` and param expansion.
- Capability: a model declaring `image_input` surfaces the correct spec;
  required-but-missing image rejected; image-but-not-supported rejected.
- Tool schema: add the `image` property in
  `crates/puffer-resources/tests/video_tool_schema.rs`.
- Reuse the existing scripted transports for submit/poll regression
  (byteplus_video.rs tests_support, relaydance_video.rs tests_support).

## Affected files

**Shared (provider-agnostic):**

- `crates/puffer-core/media_runtime.rs` — `ExactMediaGenerationRequest.image_inputs`,
  `MediaImageInput` / `ImageSource` / `ImageRole`.
- `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs` —
  `image` input field, ingestion (read / sniff / size-limit / base64-or-url).
- `crates/puffer-provider-registry/src/model.rs` — `ImageInputSpec`,
  `MediaModelDescriptor.image_input`.
- `crates/puffer-core/runtime/media/resolver.rs` +
  `crates/puffer-core/runtime/media/capabilities.rs` — surface + validate.
- `crates/puffer-core/media_runtime_video.rs` — modality validation, thread
  `image_inputs` into adapter request builders.
- `crates/puffer-resources/tests/video_tool_schema.rs` — tool schema.

**Per-provider seam:**

- `crates/puffer-core/runtime/media/byteplus_video.rs` — implemented this iteration.
- `crates/puffer-core/runtime/media/relaydance_video.rs` — wired once the gateway
  request field name is confirmed.

## Non-goals

- Replicate i2v.
- First+last frame end-to-end (types reserve it; not wired).
- Local image hosting / provider file-upload APIs (inline base64 / URL
  pass-through only).
- Extracting a unified `VideoAdapter` trait (the existing string dispatch is
  sufficient; noted as optional future cleanup, not part of this work).
