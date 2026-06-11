# Media Generation Settings Design

Date: 2026-06-11

## Goal

Unify image and video generation settings around provider-declared, product-level
capabilities. Provider YAML remains the source of truth, but it should expose
stable user-facing media axes rather than raw upstream API fields.

This design does not preserve old media settings compatibility. Existing saved
image or video selections that use legacy fields may reset to model defaults.

## Recheck Findings

The design should stay smaller than a generic media-capability framework. The
implementation should:

- Keep the existing `AxisRole` model (`param` and `selector`) instead of adding
  new `canonical` or `runtime` roles.
- Treat `mode` and `ratio` as reserved product-level axis ids, not a new axis
  type system.
- Treat image `Output` as a runtime setting synthesized from a model
  `max_outputs` field, not as a provider request axis.
- Add only a small media mapping table for `mode`, `ratio`, and `mode+ratio`
  request fields.
- First converge the desktop media settings storage contract to the logical
  `providerId/logicalModelId/selections` shape already used by the frontend and
  shared config.

This avoids a rule engine, frontend provider branches, and compatibility code
for old raw provider settings.

## User-Facing Settings

Image settings show only:

- Provider
- Model
- Mode: `1K SD`, `2K HD`
- Ratio: `Auto`, `9:16`, `2:3`, `3:4`, `1:1`, `4:3`, `3:2`, `16:9`, `21:9`
- Output: `1..model_max`, capped at 9

Video settings show:

- Provider
- Model
- Mode: model-supported quality or resolution choices. Existing values such as
  `480p`, `720p`, and `1080p` may remain if they are the intended product
  labels.
- Ratio: canonical ratio choices, displayed as `Ratio`
- Duration: displayed as `Duration`

Video may keep additional visible axes only when they are genuine product-level
choices, such as native audio. Raw provider parameters must not surface in the
settings UI.

## Canonical Axes

Media model descriptors declare product-level axes using reserved ids. The core
reserved ids are:

- `mode`: user-facing quality/resolution class.
- `ratio`: user-facing aspect ratio.

Image output count is not a provider axis. Image models declare `max_outputs`,
and capability normalization synthesizes a user-facing `output` control from
that value.

Video also supports ordinary product axes such as `duration`. Its label must be
`Duration`, not `Length`. Ratio labels must be `Ratio`, not `Video ratio`.

Provider descriptors should no longer expose image wire fields such as
`size`, `quality`, `output_format`, `response_format`, or
`sequential_image_generation` as user settings. Required fixed provider values
belong in variant `base_params` only when they must be sent.

## Descriptor Mapping

Each media model may define a small mapping table that converts reserved axes to
provider request fields. This is not a general expression language.

Image descriptor sketch:

```yaml
max_outputs: 4
axes:
  - { id: mode, label: Mode, role: param, control: !enum { values: ["1K SD", "2K HD"], default: "1K SD" } }
  - { id: ratio, label: Ratio, role: param, control: !enum { values: ["Auto", "1:1", "16:9"], default: "Auto" } }
```

`mode` and `ratio` axes may omit `request_field` only when the model's
`media_map` covers them. Normal provider `param` axes still require
`request_field`.

Aspect-ratio based providers can map `ratio` directly:

```yaml
media_map:
  ratio:
    field: aspect_ratio
    values:
      Auto: null
      "1:1": "1:1"
      "16:9": "16:9"
      "9:16": "9:16"
```

Size-based providers can map `mode + ratio`:

```yaml
media_map:
  size:
    field: size
    values:
      "1K SD":
        Auto: "auto"
        "1:1": "1024x1024"
        "2:3": "1024x1536"
        "3:2": "1536x1024"
      "2K HD":
        "1:1": "2048x2048"
        "16:9": "2048x1152"
        "9:16": "1152x2048"
```

If a ratio cannot be represented exactly for a model, it is omitted from that
model's normalized capabilities and rejected at runtime if manually configured.
The system must not silently approximate ratios.

`Auto` means "let the provider decide" by default. It should not emit a provider
field unless the descriptor explicitly maps it to a provider token such as
`auto` or `adaptive`.

## Capability Normalization

Capability listing normalizes descriptor axes before the UI receives them:

1. Validate provider-declared canonical axes.
2. Intersect canonical ratio values with exact model mappings.
3. Clamp image `output` to `1..min(model_max, 9)`.
4. Synthesize image `output` as a range control.
5. Return only user-facing axes to the UI.

The UI must not perform provider mapping. It only renders normalized capability
axes and persists canonical selections.

Provider-discovered image-output models are not in scope for inferred
mode/ratio support. If a discovered model has no static descriptor mapping, it
should not receive guessed size or ratio controls.

## Request Resolution

Request resolution becomes the only place that converts user-facing selections
to provider parameters:

1. Resolve provider, logical model, variant, and defaults.
2. Validate canonical selections against normalized capability axes.
3. Start with selected variant `base_params`.
4. Apply ordinary parameter axes, such as video `duration`.
5. Apply canonical media mappings for `mode` and `ratio`.
6. Convert image `output` into runtime `count`; do not include it in provider
   request parameters.
7. Drop saved selection keys that are not present in the normalized capability.
8. Pass only resolved provider parameters to adapters.

The resolved request should carry `count` alongside provider id, concrete model
id, adapter id, and parameters. Video remains count 1 unless a future provider
declares multi-output video support.

## Image Output Behavior

`Output` is a persisted image setting. Tool calls may still pass an explicit
`count` override, but runtime validation must enforce the selected model's
maximum output count and the global cap of 9.

When no explicit `count` is provided, image generation uses the persisted
`output` setting.

`max_outputs` is distinct from request batching. `max_outputs` controls the
user-visible and runtime-validated total output count. Existing batch settings
still describe how the adapter splits or packs provider requests.

## UI Behavior

The existing shared media settings modal remains the main UI component. It
renders normalized axes for both image and video.

Behavior:

- Switching provider or model refreshes `mode`, `ratio`, `duration`, and
  `output` choices from normalized capabilities.
- Image settings persist only canonical selections.
- Video labels are normalized to `Mode`, `Ratio`, and `Duration`.
- Provider-specific tokens such as `adaptive`, `720p`, `metadata.ratio`, or
  `1024x1536` are never shown unless they are intentionally chosen as the
  product-level label.
- Folder display is not a generation option. It may remain in the modal as
  separate output-location information, but not in the core settings list.

## Errors

Runtime remains authoritative:

- Unknown provider or model: fail with a clear media model error.
- Unsupported ratio or mode: fail before adapter dispatch.
- Missing mapping: fail before adapter dispatch.
- Output above model max or 9: fail before adapter dispatch.
- Provider HTTP errors: adapter returns the provider failure without trying a
  fallback ratio, mode, or model.

Saved selections that no longer match a capability reset to defaults in the UI.
No compatibility migration is required.

## Stability And Performance

The design keeps capability resolution deterministic and local. No extra network
round trip is needed for mapping. Capability normalization is descriptor-driven
and small enough to run on every settings open.

Runtime validation duplicates UI constraints so hand-edited configuration cannot
send invalid provider requests.

No generic rule engine, provider-specific frontend code, or dynamic expression
language is introduced.

The mapping tables are small static lookups, so performance should remain
effectively the same as current axis resolution. Avoid precomputing global
capability caches unless profiling shows repeated normalization is material.

## Tests

Add focused tests for:

- Provider YAML governance:
  - Image models expose only canonical `mode` and `ratio` settings.
  - Image capabilities synthesize `output` from `max_outputs`.
  - Video settings use labels `Mode`, `Ratio`, and `Duration`.
  - Ratio values come from the canonical list.
  - Image `max_outputs` is at most 9.
- Capability normalization:
  - Unsupported ratios are removed from the capability returned to UI.
  - `Auto` is represented consistently.
  - Output maximum reflects the selected model.
- Request resolution:
  - Aspect-ratio models emit `aspect_ratio`.
  - Size-based models emit the exact mapped `size`.
  - Hidden legacy fields are not emitted from stale selections.
  - Invalid hand-written mode, ratio, or output fails before adapter dispatch.
- Runtime:
  - Image uses persisted output as default count.
  - Explicit tool count overrides persisted output but remains bounded.
  - Video count remains unaffected.
- UI:
  - Image modal shows Provider, Model, Mode, Ratio, Output.
  - Video modal shows Provider, Model, Mode, Ratio, Duration.
  - Changing models refreshes ratio and output choices.

## Non-Goals

- Backward-compatible migration of old media settings.
- A generic YAML expression language.
- Provider-specific frontend branches.
- Silent ratio approximation.
- Sending hidden options merely because they existed in older saved selections.

## Execution Plan

1. **Unify media settings storage**
   - Replace the desktop backend's old `modelId/operation/adapter/parameters`
     stored media shape with `providerId/logicalModelId/selections`.
   - Update desktop DTOs and tests to match the frontend/shared config contract.
   - Drop compatibility behavior for old raw provider settings.

2. **Add minimal descriptor fields**
   - Add `max_outputs` to image media models, defaulting to 1 when omitted.
   - Add a small `media_map` structure for `mode`, `ratio`, and size
     `mode+ratio` lookups.
   - Update provider-registry validation so reserved `mode` and `ratio` axes may
     omit `request_field` only when backed by `media_map`.

3. **Normalize capabilities**
   - Normalize image and video capability axes before returning them to desktop.
   - Clamp image output to `1..min(max_outputs, 9)`.
   - Remove unmappable ratio values from returned capabilities.
   - Keep labels normalized to `Mode`, `Ratio`, and `Duration`.

4. **Resolve canonical selections**
   - Extend resolved media requests with `count`.
   - Map `mode` and `ratio` through `media_map`.
   - Convert image `output` into `count`.
   - Ignore stale selection keys and fail on invalid canonical values before
     adapter dispatch.

5. **Update provider descriptors**
   - Rewrite OpenAI, MiniMax, MiniMax CN, and BytePlus image settings to
     product-level `mode` and `ratio` axes plus `max_outputs`.
   - Normalize BytePlus, Relaydance, and Kling video labels from `Video ratio`
     to `Ratio` and from `Length` to `Duration`.
   - Keep video mode values as existing product labels unless a provider needs a
     clearer label.

6. **Update image generation tools and desktop generation**
   - Make ImageGeneration `count` optional.
   - Use persisted `output` when no explicit count is provided.
   - Enforce model max and global cap 9 in runtime.
   - Keep VideoGeneration count fixed at 1.

7. **Update UI behavior**
   - Render normalized axes as-is.
   - Ensure image modal core settings are Provider, Model, Mode, Ratio, Output.
   - Ensure video modal core settings are Provider, Model, Mode, Ratio,
     Duration, plus only explicit product-level extras.

8. **Verification**
   - Run focused Rust tests for provider registry, media resolver/runtime, and
     desktop backend media settings.
   - Run focused frontend tests for media axis controls and settings modal
     behavior.
   - Run the smallest integration path that exercises saving settings and
     resolving an image request without sending hidden raw provider fields.
