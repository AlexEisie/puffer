# Image Generation Settings Persistence Implementation Plan

**Goal:** Fix the image generation settings modal so saved provider, model,
adapter, and parameter changes are visible immediately after save/reopen and
survive config reloads.

**Spec:** `docs/superpowers/specs/2026-06-06-image-generation-settings-persistence-design.md`

---

## Scope Check

This is a narrow state propagation and config layering fix.

In scope:

- use the `SettingsSnapshot` returned by `update_config`;
- update the app-level `settingsSnapshot` before closing the modal;
- preserve user `media` settings across workspace config layering;
- extend focused UI coverage for save, close, reopen;
- add focused `puffer-config` layering coverage.

Out of scope:

- global frontend settings store;
- daemon settings-changed events;
- manual settings refresh after media save;
- settings-screen refactor;
- session-specific media defaults;
- media capability or runtime changes;
- backward-compatible migration for older media shapes.

---

## Phase 1: Frontend Snapshot Propagation

Files to touch:

- `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
- `apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte`
- `apps/puffer-desktop/src/lib/screens/agent/AgentDetailContent.svelte`
- `apps/puffer-desktop/src/lib/screens/agent/AgentDetail.svelte`
- `apps/puffer-desktop/src/App.svelte`

Steps:

1. Import the `SettingsSnapshot` type where the modal save callback is declared.
2. Add `onSaved: (snapshot: SettingsSnapshot) => void` to the modal props.
3. Change `save()` to call `const snapshot = await updateConfig({ media })`.
4. Call `onSaved(snapshot)` before `close()`.
5. Keep the existing error behavior: failed saves leave the modal open and do
   not mutate parent state.
6. Thread the callback through the existing active chat component chain only.
7. In `App.svelte`, update the existing `settingsSnapshot` owner directly from
   the returned snapshot. Recompute onboarding only if the local helper pattern
   already does that for settings snapshot updates.

Acceptance:

- Saving image settings performs one `update_config` request and no follow-up
  settings reload.
- Reopening the modal reads the newly saved values from parent state.
- The callback is not exposed as a broad settings mutation API.

Verification:

- `npm --prefix apps/puffer-desktop run check`

---

## Phase 2: Config Layering

File to touch:

- `crates/puffer-config/src/lib.rs`

Steps:

1. Treat `media` as a user-level preference in `load_config`.
2. Prefer a small private `UserPreferenceSnapshot` struct if it is clearer than
   extending the existing tuple; do not add a public type or API.
3. Capture `media` after the user config merge.
4. Restore `media` after workspace config merge and Claude statusline fallback.
5. Keep workspace-only media defaults working when no user config exists.

Acceptance:

- User media settings win over workspace media settings.
- Workspace media settings still apply when user config is absent.
- Existing workspace-overrides-user behavior for non-user preferences remains
  unchanged.

Verification:

- `cargo test -p puffer-config`

---

## Phase 3: UI Regression Test

Files to touch:

- `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
- `apps/puffer-desktop/tests/support/fakeDaemon.ts` only if current fake daemon
  support is insufficient

Steps:

1. Extend the existing image generation settings modal test instead of creating
   a new broad test.
2. Seed a second image capability if the fake daemon default still has only one
   image model, then choose the non-default provider/model or adapter.
3. Keep the assertion that `update_config` receives the expected `media` patch.
4. After save, assert the modal closes.
5. Reopen Image generation settings from the composer menu.
6. Assert provider, model, and image parameters reflect the saved values from
   the fake daemon's returned `SettingsSnapshot`.
7. Do not add a manual settings refresh to the test.

Acceptance:

- The test fails against the current stale-snapshot behavior.
- The test passes when the app consumes the returned snapshot.

Verification:

- `npm --prefix apps/puffer-desktop run test:desktop-ui -- tests/chat-session-ui.spec.ts -g "composer image generation settings modal saves media config"`

---

## Phase 4: Existing Daemon Coverage Check

File to inspect:

- `crates/puffer-cli/src/daemon.rs`

Steps:

1. Confirm `update_config_accepts_media_defaults` still asserts
   `response["config"]["media"]`.
2. Do not add another daemon test if that assertion still covers saved and reset
   media responses.
3. If the existing assertion has been removed or weakened, restore focused
   daemon coverage in the same test module.

Acceptance:

- Daemon response shape remains covered without duplicate test cases.

Verification:

- Covered by the smallest relevant `puffer-cli` daemon test command available
  during implementation, or by the existing full test target if a focused
  filter is not practical.

---

## Phase 5: Final Verification

Steps:

1. Run the focused frontend check and UI test.
2. Run `cargo test -p puffer-config`.
3. Run a focused daemon test only if Phase 4 required daemon test changes.
4. Inspect `git diff` for accidental changes outside the scoped files.
5. Confirm unrelated existing modified files remain untouched.

Stop and revisit the spec if implementation appears to require:

- a frontend settings store;
- an event bus or daemon push notification;
- a second settings request after save;
- media runtime/tool changes;
- session schema changes;
- broad settings-screen rewiring.
