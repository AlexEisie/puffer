# Video Model Catalog Discovery Design

Date: 2026-06-09

## Summary

Video generation model selection should use the same capability boundary as
image generation: provider resources define exact executable capabilities, the
resolver validates them, and the UI renders the resulting capability list.

For the first stable version, video should stay **static-catalog driven**. Do
not add runtime video discovery yet. BytePlus and RelayDance video models have
model-specific parameter boundaries, and the available public endpoints do not
return enough structured parameter metadata to safely generate model
descriptors. A dynamic discovery layer can be added later only if a provider
offers a stable, trusted endpoint that maps cleanly onto predeclared
descriptors.

## Problem

`Video generation settings` currently shows only one model for BytePlus and one
model for RelayDance because the video resolver only expands
`media.video.models` from static provider YAML. That is locally consistent but
incomplete: public provider data shows multiple supported video model SKUs.

The missing piece is not a frontend model-picker bug. The picker displays the
capabilities returned by `list_media_capabilities(kind=video)`. The catalog is
small because the descriptors are small.

## Recheck Findings

The previous design proposed a `Trusted Video Discovery Overlay`. After
rechecking against the current image implementation, that is too much for this
slice:

- Image discovery is used only for trusted router/gateway providers that return
  useful image-output metadata.
- Video models need more than model ids. They need exact duration, resolution,
  request-field, audio, reference-media, and adapter constraints.
- RelayDance `/api/pricing` is useful evidence that more models exist, but a
  pricing endpoint is not an executable schema contract.
- BytePlus public docs confirm distinct Seedance 2.0 / 2.0 fast products, but
  model ids and parameter constraints should still be entered as verified
  descriptors rather than inferred dynamically.

The corrected v1 design is therefore: **Exact Static Video Catalog With
Image-Style Capability Governance**.

## Goals

- Show all verified BytePlus and RelayDance video models that the current
  adapters can execute.
- Keep provider YAML as the executable source of truth for video models.
- Match image handling at the capability boundary: resource descriptors in,
  validated capability views out, UI as a pure consumer.
- Keep settings responsive without live provider calls.
- Add governance tests so resource catalog drift is caught at build/test time.
- Keep future video discovery possible without implementing it prematurely.

## Non-Goals

- No runtime video discovery in this slice.
- No full dynamic video schema system.
- No provider-specific UI for video settings.
- No background sync service or model marketplace.
- No automatic parameter inference from pricing pages.
- No compatibility layer for old, incomplete video model catalogs.
- No attempt to display models that lack a verified execution adapter.

## Chosen Approach

Use an exact static catalog:

1. Declare each supported video model in provider YAML.
2. Give each model its own parameter list and defaults.
3. Let the existing video resolver emit one capability per declared model.
4. Let the existing UI show those capabilities without custom provider logic.

A video model is selectable only when it has:

- a concrete model id;
- `generate` operation support;
- a supported video execution adapter;
- model-specific parameters with valid defaults;
- request-field mappings that the adapter can serialize.

Do not create selectable video capabilities from discovered model ids. Unknown
video ids are not useful unless the repository also knows their adapter and
parameter contract.

## Catalog Rules

Every declared video model must satisfy these rules:

- The model id is non-empty, not `auto`, and contains no wildcard or regex
  markers.
- `operations` includes `generate`.
- The model or provider declares an execution adapter supported by
  `MediaKind::Video`.
- Every parameter has at least one value.
- Every parameter default appears in its values.
- Single-value parameters are allowed and should render as read-only values in
  the existing settings UI.
- Resolution-specific provider SKUs should not expose invalid resolution
  choices. If the model id is already resolution-specific, the descriptor should
  either omit resolution or declare it as a single-value parameter.
- Fast and standard variants must not share parameter lists when their
  supported resolutions, durations, audio behavior, or request fields differ.

## Provider Strategy

### BytePlus

BytePlus video descriptors should come from official ModelArk documentation or
authenticated ModelArk evidence. Add only model ids whose exact API id and
parameter constraints are verified.

The standard and fast Seedance 2.0 variants should be separate descriptors. Do
not let a fast model expose `1080p` if official docs restrict it to
`480p`/`720p`.

Do not add a BytePlus dynamic discovery client in v1.

### RelayDance

RelayDance descriptors should include only models compatible with the existing
`relaydance_video` adapter request shape.

RelayDance pricing data can be used as research evidence, not as runtime
discovery. It confirms that multiple public SKUs exist, but the descriptor must
still decide:

- whether the model id is compatible with `/v1/video/generations`;
- whether resolution is fixed by the model id or passed through
  `metadata.resolution`;
- which duration values are valid;
- whether audio/reference-media fields require a separate adapter contract.

Models whose request shape requires new fields should wait for a separate
adapter design.

## Resolver Behavior

The video resolver should remain simple:

1. Iterate provider `media.video.models`.
2. Filter invalid model ids and unsupported operations.
3. Resolve provider-level or model-level execution.
4. Emit unavailable capabilities for missing auth or unavailable adapters.
5. Emit available capabilities for connected providers with supported adapters.

No video discovery cache changes are needed for v1. `MediaDiscoveryCache` stays
image-only until a provider gives a trusted video endpoint that adds real value.

## UI Behavior

The desktop settings modal should remain architecturally unchanged:

- call `list_media_capabilities(kind=video)`;
- show providers from available video capabilities;
- show models from capabilities matching the selected provider;
- render parameters declared on the selected capability.

The UI must not call provider APIs, parse pricing endpoints, or infer missing
models.

## Error Handling

Catalog errors should be caught by tests, not by runtime guessing. At runtime:

- missing auth keeps provider capabilities unavailable with `missing_auth`;
- unsupported adapters produce `adapter_unavailable`;
- generation validation rejects unavailable capabilities;
- stale saved selections continue to show the existing unavailable warning.

## Performance

The common path is descriptor rendering only. Opening video settings does not
require network calls beyond the existing backend request for
`list_media_capabilities`.

This keeps performance stable and avoids coupling settings latency to provider
availability, pricing endpoints, or auth state beyond the existing capability
resolver.

## Testing

Coverage should include:

- provider YAML governance for all declared video models;
- BytePlus and RelayDance expose multiple verified video capabilities when
  connected;
- model-specific parameters differ where provider constraints differ;
- single-value parameters render as read-only fields in video settings;
- generation serializes the selected model id through the existing adapter;
- unavailable video capabilities cannot validate generation selections;
- UI shows multiple models for one provider without custom frontend logic.

## Future Discovery Gate

Video discovery can be revisited only when all of these are true:

- the provider exposes a stable endpoint;
- the endpoint is authenticated or otherwise trustworthy;
- returned model ids map to known static descriptors or include enough
  structured metadata to validate parameters;
- discovery failures can safely fall back to static descriptors;
- tests prove unknown discovered models do not appear in the UI.

Until then, discovery belongs in research and catalog maintenance, not runtime.

## Rejected Alternatives

### Runtime Trusted Video Discovery Overlay

This was the previous recommendation. It is deferred because current provider
evidence is not a sufficient executable schema contract. Adding it now would
increase runtime complexity without solving the immediate model-list problem.

### Fully Dynamic Video Discovery

This is too broad. Video parameters vary by model and provider: duration,
resolution, audio support, reference media support, and request fields cannot be
safely inferred from a model id or pricing row.

### Provider-Specific UI

This would fragment the settings modal and make each provider harder to test.
The existing capability-driven UI is the right boundary; provider differences
belong in descriptors and adapters.

## Implementation Boundary

This design is ready for a small implementation plan:

1. Verify model ids and constraints from provider evidence.
2. Expand static video descriptors only for verified executable models.
3. Add catalog governance tests.
4. Add resolver/backend/UI tests that prove multiple models flow through the
   existing capability path.
5. Do not add runtime video discovery in this slice.
