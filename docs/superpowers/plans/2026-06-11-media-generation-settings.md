# Canonical Media Generation Settings — Implementation Plan

> For agentic workers: implement this plan task-by-task. Keep each task small,
> test-backed, and independently reviewable. Do not add a generic media rule
> engine.

**Goal:** Make image and video generation settings peer concepts driven by
provider YAML product-level axes. Image shows Provider, Model, Mode, Ratio, and
Output. Video shows Provider, Model, Mode, Ratio, and Duration. Provider wire
fields stay out of the settings UI and are produced only by request resolution.

**Spec:** `docs/superpowers/specs/2026-06-11-media-generation-settings-design.md`

**Branch:** `feat/video-settings`

**Lean constraints:**

- Keep `AxisRole::{Param, Selector}`. Do not add `canonical` or `runtime` roles.
- Treat `mode` and `ratio` as reserved ids with special resolver behavior.
- Treat image `Output` as synthesized from `max_outputs`, not as a provider
  request axis.
- Keep `media_map` as static lookup tables only.
- No backward compatibility for old raw media settings.
- No frontend provider-specific branches.

---

## Task 1: Converge Desktop Media Settings Storage

**Purpose:** Remove the old desktop-only media storage shape before adding new
capabilities. This prevents the UI from saving `logicalModelId/selections` while
the backend expects `modelId/adapter/parameters`.

**Files:**

- `apps/puffer-desktop/src-tauri/src/backend.rs`
- `apps/puffer-desktop/src-tauri/src/dtos.rs`
- `apps/puffer-desktop/src/lib/types.ts`
- `apps/puffer-desktop/src/lib/api/desktop.ts`
- Existing backend/settings tests

**Steps:**

- [ ] Replace `StoredMediaGenerationConfig { provider_id, model_id, operation, adapter, parameters }`
  with `{ provider_id, logical_model_id, selections }`.
- [ ] Update `MediaGenerationSettingsDto` to serialize `providerId`,
  `logicalModelId`, and `selections`.
- [ ] Update `stored_media_selection_for_kind` and
  `exact_media_generation_request_from_stored` to pass logical selections into
  the media resolver.
- [ ] Delete tests that assert old `parameters` persistence and replace them
  with logical-selection tests.

**Acceptance:**

- Saving settings from the existing frontend shape succeeds.
- Loading settings returns the same logical selection shape.
- Old raw provider settings are not migrated or preserved.

**Verification:**

- `cargo test -p corbina_lib update_config_saves_media_without_mutating_chat_defaults`
- Focused backend tests around `generate_media_requires_*_settings`.

**Commit:** `fix(desktop): store media settings as logical selections`

---

## Task 2: Add Minimal Descriptor Fields

**Purpose:** Add only the descriptor data needed for canonical settings:
`max_outputs` and static `media_map`.

**Files:**

- `crates/puffer-provider-registry/src/model.rs`
- `crates/puffer-provider-registry/src/media_capability.rs`
- `crates/puffer-provider-registry/src/model_tests.rs`
- `crates/puffer-provider-registry/src/lib.rs`

**Steps:**

- [ ] Add `max_outputs: Option<u8>` to `MediaModelDescriptor`.
- [ ] Add `MediaMap` types for:
  - `ratio` one-dimensional mapping: provider field + ratio value map.
  - `size` two-dimensional mapping: provider field + mode-to-ratio value map.
- [ ] Keep the map intentionally narrow. Do not add expressions, predicates, or
  fallback policies.
- [ ] Update validation:
  - `max_outputs <= 9`.
  - `mode` and `ratio` axes may omit `request_field` only if covered by
    `media_map`.
  - ordinary `param` axes still require `request_field`.
  - `ratio` values must be from the canonical list.

**Acceptance:**

- Provider descriptors parse `max_outputs` and `media_map`.
- Invalid maps fail at descriptor validation time.
- Existing non-media providers are unaffected.

**Verification:**

- `cargo test -p puffer-provider-registry media`
- `cargo test -p puffer-provider-registry provider_capability`

**Commit:** `feat(registry): add canonical media map descriptors`

---

## Task 3: Normalize Capabilities For UI

**Purpose:** Return only user-facing axes to desktop. The UI should not know
provider wire fields or mapping rules.

**Files:**

- `crates/puffer-media/src/media/resolver.rs`
- `crates/puffer-media/src/media/capabilities.rs`
- `crates/puffer-media/src/runtime.rs`
- `apps/puffer-desktop/src-tauri/src/media_capabilities.rs`
- `apps/puffer-desktop/src/lib/types.ts`

**Steps:**

- [ ] Add a normalization pass before `MediaCapabilityView` is returned.
- [ ] For image capabilities, synthesize an `output` range axis using
  `1..min(max_outputs.unwrap_or(1), 9)`.
- [ ] Intersect `ratio` values with exact mappings from `media_map`.
- [ ] Preserve `mode` values from descriptors.
- [ ] Normalize video labels to `Mode`, `Ratio`, and `Duration`.
- [ ] Do not infer mode/ratio controls for provider-discovered image-output
  models without static mappings.

**Acceptance:**

- Image capabilities expose `mode`, `ratio`, and synthesized `output`.
- Video capabilities expose `Mode`, `Ratio`, and `Duration` labels.
- Unsupported ratios are absent from returned capabilities.

**Verification:**

- `cargo test -p puffer-media resolver`
- `cargo test -p puffer-media runtime`
- Tauri backend capability tests for image and video payloads.

**Commit:** `feat(media): normalize canonical media capability axes`

---

## Task 4: Resolve Canonical Selections

**Purpose:** Convert product-level selections into provider request params and
runtime count in one backend path.

**Files:**

- `crates/puffer-media/src/media/resolver.rs`
- `crates/puffer-media/src/runtime.rs`
- `crates/puffer-media/src/video.rs`
- `crates/puffer-media/src/media/planner.rs`
- Adapter tests under `crates/puffer-media/src/media/*_tests.rs`

**Steps:**

- [ ] Add `count` to `ResolvedMediaRequest`.
- [ ] Apply ordinary param axes first, excluding reserved `mode`, `ratio`, and
  synthesized `output`.
- [ ] Map `ratio` through one-dimensional `media_map` when present.
- [ ] Map `mode + ratio` through size `media_map` when present.
- [ ] Treat `Auto: null` as "do not emit a field".
- [ ] Convert image `output` into `ResolvedMediaRequest.count`.
- [ ] Enforce `count <= max_outputs` and `count <= 9`.
- [ ] Drop stale saved selection keys that are not in normalized axes.

**Acceptance:**

- Aspect-ratio models emit `aspect_ratio`.
- Size-based models emit exact `size`.
- Hidden legacy fields cannot leak into request params.
- Invalid mode, ratio, or output fails before adapter dispatch.

**Verification:**

- `cargo test -p puffer-media resolver_tests`
- `cargo test -p puffer-media runtime_tests`
- Focused tests for OpenAI-size, MiniMax-aspect, and BytePlus-size cases.

**Commit:** `feat(media): resolve canonical mode ratio and output`

---

## Task 5: Rewrite Provider Descriptors

**Purpose:** Make provider YAML expose product-level media settings.

**Files:**

- `resources/providers/openai.yaml`
- `resources/providers/minimax.yaml`
- `resources/providers/minimax-cn.yaml`
- `resources/providers/byteplus.yaml`
- `resources/providers/relaydance.yaml`
- `resources/providers/kling.yaml`
- `crates/puffer-resources/tests/provider_capability_axes.rs`
- `crates/puffer-resources/tests/image_catalog_governance.rs`
- `crates/puffer-resources/tests/media_generation_skills.rs`

**Steps:**

- [ ] OpenAI image models: replace `size/quality/output_format` axes with
  `mode/ratio`, add `media_map`, add `max_outputs`.
- [ ] MiniMax and MiniMax CN image models: replace visible
  `response_format` with hidden/default behavior, keep ratio through mapping,
  add `mode` only where it has real provider meaning.
- [ ] BytePlus image models: replace `size/output_format/response_format/sequential_image_generation`
  axes with product-level `mode/ratio`, add exact size mappings and
  `max_outputs`.
- [ ] Video providers: relabel `Video ratio` to `Ratio`, `Length` to
  `Duration`.
- [ ] Keep current video mode labels (`480p`, `720p`, `1080p`) unless a provider
  descriptor already has a clearer product label.

**Acceptance:**

- Image settings no longer expose raw provider fields.
- Video labels match requested names.
- Governance tests reject future raw image settings.

**Verification:**

- `cargo test -p puffer-resources provider_capability_axes`
- `cargo test -p puffer-resources image_catalog_governance`
- `cargo test -p puffer-resources media_generation_skills`

**Commit:** `feat(resources): declare canonical image and video settings`

---

## Task 6: Update ImageGeneration And Desktop Generation

**Purpose:** Make persisted `Output` the default image count while preserving
explicit tool overrides.

**Files:**

- `resources/internal_tools/image_generation.yaml`
- `crates/puffer-core/runtime/claude_tools/workflow/image_generation.rs`
- `crates/puffer-core/runtime/claude_tools/workflow/image_generation/tests/image_generation_tool_tests.rs`
- `apps/puffer-desktop/src-tauri/src/backend.rs`
- `apps/puffer-desktop/src/lib/types.ts`

**Steps:**

- [ ] Make ImageGeneration `count` optional in the tool schema.
- [ ] Default count from persisted image `output` when no explicit count is
  supplied.
- [ ] Keep explicit `count` as a per-call override.
- [ ] Enforce model max and global cap 9 after resolving selected capability.
- [ ] Keep VideoGeneration count fixed at 1.
- [ ] Remove old `aspect` to `size/aspect_ratio` override path if it conflicts
  with canonical `ratio`; if per-call ratio override remains, route it through
  the same canonical resolver.

**Acceptance:**

- Image generation uses settings output by default.
- Explicit count override works but cannot exceed model max.
- Video generation behavior is unchanged except label/settings normalization.

**Verification:**

- `cargo test -p puffer-core --lib image_generation`
- `cargo test -p puffer-media runtime_tests`
- Desktop backend tests for `generate_media` count behavior.

**Commit:** `feat(media): use image output setting as generation count`

---

## Task 7: Update Desktop UI

**Purpose:** Render normalized capabilities without provider-specific UI logic.

**Files:**

- `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
- `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.ts`
- `apps/puffer-desktop/src/lib/screens/agent/mediaAxisControls.test.ts`
- `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts`
- Existing settings/chat UI tests

**Steps:**

- [ ] Render normalized axes as the existing modal already does.
- [ ] Ensure image core fields are Provider, Model, Mode, Ratio, Output.
- [ ] Ensure video core fields are Provider, Model, Mode, Ratio, Duration, plus
  only explicit product-level extras.
- [ ] Keep output-folder display visually separate from generation options.
- [ ] Ensure switching model normalizes stale selections and refreshes output
  max.

**Acceptance:**

- No raw provider settings appear in the image modal.
- `Video ratio` and `Length` do not appear in video settings.
- Output options reflect selected model max.

**Verification:**

- `npm --prefix apps/puffer-desktop test -- mediaAxisControls`
- Focused Playwright/settings UI test if existing harness supports modal
  interaction.

**Commit:** `fix(desktop): render canonical media generation settings`

---

## Task 8: Component Specs And Final Verification

**Purpose:** Keep repo documentation and verification aligned with the behavior
change.

**Files:**

- Next numbered spec under `specs/puffer-provider-registry/`
- Next numbered spec under `specs/puffer-media/`
- Next numbered spec under `specs/puffer-resources/`
- Any desktop/app spec location already used for media settings, if present

**Steps:**

- [ ] Add concise update specs for changed components.
- [ ] Run the focused test set from prior tasks.
- [ ] Run broader checks only after focused tests pass.
- [ ] Review `git diff` for hidden raw provider fields in image request paths.

**Final verification:**

- `cargo test -p puffer-provider-registry`
- `cargo test -p puffer-resources`
- `cargo test -p puffer-media`
- `cargo test -p puffer-core --lib image_generation`
- `npm --prefix apps/puffer-desktop test -- media`
- If time allows before merge: `cargo test --workspace`

**Commit:** `docs: record canonical media settings component updates`
