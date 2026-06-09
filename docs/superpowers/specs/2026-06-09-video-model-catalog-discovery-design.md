# Video Model Catalog Discovery Design

Date: 2026-06-09

## Summary

Video generation model selection should follow the mature image capability
pattern: static provider descriptors define the executable catalog, while a
small trusted discovery overlay validates availability for selected providers.
The UI remains a pure capability consumer. It must not infer models, parameter
schemas, or provider behavior.

This design intentionally does not preserve older video catalog assumptions.
The long-term contract is that every selectable video model has an explicit
descriptor with model-specific parameters, defaults, request-field mapping, and
adapter selection.

## Problem

`Video generation settings` currently shows only one model for BytePlus and one
model for RelayDance because the video resolver only expands
`media.video.models` from static provider YAML. That is locally consistent but
incomplete: public provider data shows multiple supported video model SKUs.

The missing piece is not a frontend model-picker bug. The picker displays the
capabilities returned by `list_media_capabilities(kind=video)`. The catalog is
small because the descriptors are small and video discovery does not yet add
availability context.

## Goals

- Keep the model picker accurate for BytePlus, RelayDance, and future video
  providers.
- Use static descriptors as the executable source of truth for video models.
- Reuse the existing media discovery cache shape and TTL behavior where
  practical.
- Avoid dynamic video parameter generation because video model constraints are
  provider- and model-specific.
- Keep settings responsive when provider discovery is slow, unavailable, or
  unauthenticated.
- Make new model additions mostly resource work, with tests guarding adapter
  compatibility.

## Non-Goals

- No full dynamic video schema system.
- No provider-specific UI for video settings.
- No background sync service or model marketplace.
- No automatic parameter inference from pricing pages.
- No compatibility layer for old, incomplete video model catalogs.
- No attempt to display models that lack a verified execution adapter.

## Chosen Approach

Use a `Static Catalog + Trusted Video Discovery Overlay` model.

Static provider YAML remains the only source that can create selectable video
capabilities. A video model is selectable only when it has:

- a concrete model id;
- `generate` operation support;
- a supported video execution adapter;
- model-specific parameters with valid defaults;
- request-field mappings that the adapter can serialize.

Trusted discovery can then annotate or filter those static models for provider
availability. It cannot create a new selectable video model by itself.

This mirrors image handling where static descriptors and trusted discovery cache
entries are merged before the UI sees capabilities, but video keeps a stricter
catalog rule because video parameters are not uniform across models.

## Architecture

Extend the existing media discovery cache to carry video availability alongside
image discovery:

```text
MediaDiscoveryCache
  image_models: Vec<CachedImageMediaModel>
  video_models: Vec<CachedVideoMediaModel>
```

`CachedVideoMediaModel` should be intentionally smaller than image discovered
models:

```text
provider_id
model_id
source
checked_at_ms
availability metadata
```

It should not carry a generated `MediaModelDescriptor`. The descriptor must
come from provider resources.

## Resolver Behavior

The video resolver should:

1. Load static video models from each provider descriptor.
2. Validate model id, operation, adapter availability, auth state, and declared
   parameter defaults.
3. Apply trusted discovery overlay when available.
4. Return capability views to the UI.

Discovery outcomes:

- Static model found in trusted discovery: keep available when auth and adapter
  are valid.
- Static model missing from trusted discovery: provider policy decides whether
  to mark unavailable or keep static availability.
- Discovery failure: keep static descriptors usable and mark source as static.
- Discovery returns an undeclared model: ignore for UI selection; keep it only
  as diagnostic input in tests or logs if needed.

## Provider Strategy

### BytePlus

BytePlus video models should be statically declared from official ModelArk
documentation. The standard and fast Seedance 2.0 variants need distinct model
descriptors because their resolution and pricing constraints differ.

Do not add a BytePlus dynamic discovery client until there is a stable,
authenticated API that returns video model ids with enough trust to validate
availability. Official documentation and resource descriptors are sufficient
for the first pass.

### RelayDance

RelayDance can use its public pricing endpoint as a trusted availability
overlay because it returns concrete model names and pricing metadata. The
pricing endpoint should not define parameters. It only confirms whether a
statically declared RelayDance model is currently advertised.

RelayDance static descriptors should include only models compatible with the
existing `relaydance_video` adapter request shape. Models whose request shape
requires new fields or media-reference behavior should wait for a separate
adapter contract.

## UI Behavior

The desktop settings modal should remain unchanged at the architectural level:

- call `list_media_capabilities(kind=video)`;
- show providers from available video capabilities;
- show models from capabilities matching the selected provider;
- render parameters declared on the selected capability.

The UI must not call provider APIs, parse pricing endpoints, or infer missing
models.

## Error Handling

Discovery errors are non-fatal. They should not blank the settings modal or
remove all static models. The resolver should return useful static capabilities
when possible.

Unavailable capabilities should continue to carry a reason such as
`missing_auth` or `adapter_unavailable`. If a trusted overlay determines a
declared model is unavailable, use a distinct reason such as
`provider_model_unavailable`.

## Performance

Reuse the existing media discovery TTL behavior. Discovery should happen in the
backend/daemon cache path, not in the frontend. Requests should be bounded with
short timeouts and limited provider concurrency.

The common path remains static descriptor rendering, so opening video settings
does not require live provider calls when the cache is fresh or discovery is
disabled for the provider.

## Testing

Coverage should include:

- video resolver lists multiple static models for BytePlus and RelayDance;
- model-specific parameters differ where provider constraints differ;
- RelayDance trusted discovery filters or annotates only declared models;
- undeclared discovered video models are not shown in settings;
- discovery failure leaves static video capabilities usable;
- UI shows multiple models for one provider without custom frontend logic;
- generation serializes the selected model id through the existing adapter.

## Rejected Alternatives

### Static Catalog Only

This is simplest but repeats the current failure mode. It relies entirely on
manual catalog maintenance and cannot notice provider-side availability drift.

### Fully Dynamic Video Discovery

This is too broad for current needs. Video parameters vary by model and provider:
duration, resolution, audio support, reference media support, and request fields
cannot be safely inferred from a model id or pricing row. A dynamic schema
system would increase complexity without improving near-term reliability.

### Provider-Specific UI

This would fragment the settings modal and make each provider harder to test.
The existing capability-driven UI is the right boundary; provider differences
belong in descriptors and adapters.

## Implementation Boundary

This design is ready for an implementation plan, but implementation should stay
small:

1. Expand static video descriptors for confirmed models.
2. Add video availability entries to `MediaDiscoveryCache`.
3. Add a trusted RelayDance pricing discovery parser.
4. Apply video discovery overlay in the resolver.
5. Keep the UI unchanged except for tests that assert more model options.
