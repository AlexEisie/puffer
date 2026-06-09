# Video Model Catalog Implementation Plan

**Goal:** Make `Video generation settings` show every verified executable
BytePlus and RelayDance video model through the existing media capability path,
without adding runtime video discovery or provider-specific UI.

**Spec:** `docs/superpowers/specs/2026-06-09-video-model-catalog-discovery-design.md`

**Architecture:** Use exact static provider descriptors as the video model
catalog. The resolver already emits one capability per declared video model;
the implementation should expand descriptors, tighten governance tests, and add
focused backend/UI/runtime coverage.

**Scope:** Provider resources, resolver/backend tests, desktop settings tests,
and component update specs. No runtime discovery, no pricing parser, no
frontend provider special cases, and no media-reference adapter work.

---

## Scope Check

In scope:

- BytePlus and RelayDance `media.video.models` descriptors;
- model-specific parameter values/defaults;
- governance tests for video model descriptor quality;
- `list_media_capabilities(kind=video)` coverage;
- desktop settings coverage for multiple models under one provider;
- runtime serialization tests proving the selected model id reaches the
  adapter request body.

Out of scope:

- `TrustedVideoDiscoveryClient`;
- changes to `MediaDiscoveryCache`;
- live provider calls in settings;
- automatic parsing of RelayDance pricing data;
- dynamic parameter schema generation;
- provider-specific Svelte UI;
- generated video poster/preview changes;
- support for image/video/audio reference inputs beyond the current text prompt
  adapter shape.

---

## Expected File Touches

- Modify: `resources/providers/byteplus.yaml`
  - Add only officially verified BytePlus video model ids.
  - Keep model-specific resolution and duration constraints.

- Modify: `resources/providers/relaydance.yaml`
  - Add only RelayDance video models compatible with `relaydance_video`.
  - Use single-value resolution parameters for resolution-specific SKUs.

- Modify: `crates/puffer-resources/tests/image_catalog_governance.rs`
  - Add reusable video descriptor governance helpers.
  - Assert BytePlus and RelayDance model ids and parameter boundaries.

- Modify: `crates/puffer-core/runtime/media/resolver.rs`
  - Add or adjust tests proving multiple static video models become
    capabilities.
  - No resolver behavior change expected unless a test exposes a defect.

- Modify: `crates/puffer-cli/src/daemon.rs`
  - Update focused daemon media capability tests to assert multiple video
    capabilities where appropriate.

- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Add a focused multi-model video settings test using fake capabilities.

- Modify only if tests require it:
  - `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
  - Expected no architectural change; only small fixes if model switching or
    single-value parameter rendering is already wrong.

- Add/update component specs:
  - `specs/puffer-resources/<next>.md`
  - `specs/puffer-core/<next>.md` if resolver/runtime tests change behavior.
  - `specs/puffer-cli/<next>.md` if daemon expectations change.
  - `specs/puffer-desktop/<next>.md` if desktop behavior or tests are updated.

---

## Task 1: Evidence Gate And Model Matrix

- [ ] Re-read current provider descriptors and the design spec.
- [ ] Confirm the exact RelayDance candidate ids from provider evidence:
  - `doubao-seedance-2-0-720p`
  - `doubao-seedance-2-0-1080p`
  - `doubao-seedance-2-0-fast-260128`
- [ ] Confirm which RelayDance candidates use the existing
  `/v1/video/generations` request shape.
- [ ] Confirm exact BytePlus video model ids from official ModelArk docs or an
  authenticated ModelArk source.
- [ ] Do not add a BytePlus fast descriptor if the exact API id cannot be
  verified during implementation.
- [ ] Build a short model matrix in implementation notes before editing YAML:
  provider, model id, display name, resolution values, duration values, ratio
  values, adapter, and source evidence.

Exit criteria:

- Every model to be added has source evidence and a known adapter shape.
- Unverified candidates are explicitly skipped, not guessed.

---

## Task 2: Expand Static Video Descriptors

- [ ] Update RelayDance descriptors for verified executable models.
- [ ] Keep the existing `relaydance_video` provider-level execution path.
- [ ] Use distinct display names when multiple RelayDance SKUs share the
  Seedance family name.
- [ ] For resolution-specific RelayDance model ids, use a fixed single-value
  `resolution` parameter or omit the parameter if the adapter does not need it.
- [ ] Update BytePlus descriptors only for verified official model ids.
- [ ] Keep BytePlus standard and fast variants as separate descriptors.
- [ ] Do not share a parameter list when model constraints differ.

Implementation guard:

- Do not add a new adapter.
- Do not add reference-media fields unless the existing adapter already maps
  them correctly.
- Do not infer model ids from display names.

Exit criteria:

- `resources/providers/*.yaml` parse successfully.
- Each added model has valid defaults and no invalid resolution choices.

---

## Task 3: Add Resource Governance Tests

- [ ] Add helper coverage for declared video model ids:
  - non-empty id;
  - no `auto`;
  - no wildcard or regex markers;
  - `generate` operation present;
  - supported video adapter present;
  - parameter values non-empty;
  - defaults included in values;
  - no duplicate model ids per provider.
- [ ] Assert the expected BytePlus video model set.
- [ ] Assert the expected RelayDance video model set.
- [ ] Assert model-specific resolution constraints, especially standard vs fast
  and resolution-specific RelayDance SKUs.
- [ ] Keep existing image catalog tests intact.

Verification:

```bash
cargo test -p puffer-resources --test image_catalog_governance
```

Exit criteria:

- Governance tests fail on invalid model defaults or accidental model drift.

---

## Task 4: Add Resolver And Daemon Coverage

- [ ] Add or update resolver tests proving one connected provider can emit
  multiple available video capabilities.
- [ ] Assert unavailable status still works for missing auth.
- [ ] Assert generation validation rejects unavailable video capabilities.
- [ ] Update daemon tests so `list_media_capabilities(kind=video)` asserts
  presence of specific provider/model/adapter triples instead of exact count
  unless exact count is the behavior under test.
- [ ] Keep `source` as `static`.

Verification:

```bash
cargo test -p puffer-core connected_relaydance_video_descriptor_is_available
cargo test -p puffer-core unavailable_video_capability_cannot_validate_generation_selection
cargo test -p puffer-cli daemon_list_media_capabilities_returns
```

Exit criteria:

- The backend returns multiple static video capabilities without discovery.
- Existing unavailable-provider behavior remains intact.

---

## Task 5: Add Desktop Settings Coverage

- [ ] Add a fake video capability set with one provider and multiple models.
- [ ] Open `Video generation settings`.
- [ ] Assert the Provider field remains BytePlus or RelayDance.
- [ ] Assert the Model control lists multiple model options.
- [ ] Switch models and assert model-specific parameter controls update.
- [ ] Assert single-value parameters render read-only if the existing modal
  supports that behavior.
- [ ] Save settings and assert the selected provider/model/adapter/parameters
  are persisted.

Verification:

```bash
env PUFFER_DESKTOP_TEST_PORT=1537 npx playwright test tests/chat-session-ui.spec.ts -g "video generation settings"
```

Exit criteria:

- UI consumes the expanded capability list without provider-specific code.

---

## Task 6: Runtime Serialization Coverage

- [ ] Add focused tests for RelayDance request construction for each new
  descriptor shape that differs from the current model.
- [ ] Add focused tests for BytePlus request construction only for added
  BytePlus models.
- [ ] Assert the selected model id is serialized as request `model`.
- [ ] Assert fixed/defaulted parameters serialize through the declared
  `request_field`.
- [ ] Do not add live provider integration tests.

Verification:

```bash
cargo test -p puffer-core relaydance_video_request
cargo test -p puffer-core byteplus_video_request
cargo test -p puffer-core execute_uses_exact_video_generation_and_returns_artifacts
```

Exit criteria:

- Runtime tests prove descriptor selection changes the request body as expected.

---

## Task 7: Specs, Final Verification, And Commit

- [ ] Add concise component update specs for each touched component directory.
- [ ] Run Svelte diagnostics:

```bash
npm --prefix apps/puffer-desktop run check
```

- [ ] Run focused desktop tests:

```bash
env PUFFER_DESKTOP_TEST_PORT=1537 npx playwright test tests/chat-session-ui.spec.ts -g "video generation settings"
```

- [ ] Run focused Rust tests from Tasks 3, 4, and 6.
- [ ] Inspect staged diff to confirm no poster/preview changes are included.
- [ ] Commit only the catalog/model-settings work.

Exit criteria:

- All focused tests pass.
- The final diff contains no runtime discovery, pricing parser, or poster work.

---

## Stop Conditions

Stop and revisit the design if implementation appears to require:

- adding `TrustedVideoDiscoveryClient`;
- changing `MediaDiscoveryCache`;
- calling provider APIs from the frontend;
- parsing RelayDance pricing at runtime;
- a new video adapter;
- image/video/audio reference input support;
- provider-specific UI branches;
- committing generated video poster/preview changes.
