# Image Generation Settings Persistence Design

## Summary

Image generation settings should reflect saved provider, model, adapter, and
parameter changes immediately after the user saves them, and the same values
should survive settings reloads and daemon restarts.

The long-term fix is intentionally small:

- consume the fresh `SettingsSnapshot` returned by `update_config`;
- propagate that snapshot back to the app-level settings owner;
- treat `media` as a user-level preference in config layering;
- add focused UI and config tests for the save-and-reopen path.

This design does not add a frontend settings store, daemon settings events, or
session-specific media defaults. It also avoids a second settings reload after
save; the daemon response is already the fresh source of truth.

## Problem

The image generation settings modal currently builds the correct `media` patch
and sends it to `update_config`, but it discards the returned settings snapshot
and closes. The next time the modal opens, it initializes from the unchanged
parent `settingsSnapshot`, so the UI shows the old provider/model selection.

The daemon-side `update_config` path already updates in-memory config, saves the
user config file, and returns a refreshed `SettingsSnapshot`. The missing step is
using that returned snapshot in the desktop app state.

There is also a config layering gap. `media` is a user preference like browser
and network settings, but `puffer-config::load_config` currently restores only a
subset of user-level fields after workspace config is merged. If the workspace
root has a workspace config, user media settings can be lost on reload.

## Goals

- Saving image generation settings updates the modal source of truth without a
  page reload or second backend request.
- Reopening the modal immediately shows the saved provider, model, adapter, and
  parameters.
- User-level `media` config survives workspace config layering.
- The fix follows existing settings patterns in the app.
- Test coverage catches stale-snapshot regressions.

## Non-Goals

- Do not introduce a global frontend settings store.
- Do not add daemon `settings_changed` events.
- Do not refactor all settings screens.
- Do not add session-specific media defaults.
- Do not change media capability discovery or exact image execution.
- Do not migrate old config fields or support legacy media shapes.

## Frontend Contract

`updateConfig` already returns a fresh `SettingsSnapshot`. For this modal, the
returned snapshot must be passed to the app-level settings owner. Do not add a
follow-up settings refresh as part of this save path.

For the image generation modal, the preferred contract is:

```ts
type Props = {
  kind: MediaKind;
  sessionCwd: string;
  settings: MediaSettings;
  settingsReady?: boolean;
  onSaved: (snapshot: SettingsSnapshot) => void;
  onClose: () => void;
};
```

The modal save flow becomes:

1. Build the `media` patch from current controls.
2. Call `const snapshot = await updateConfig({ media })`.
3. Call `onSaved(snapshot)`.
4. Close the modal.

`onSaved` must run before `onClose`, so a fast reopen reads from the updated
parent snapshot instead of the previous props. If the save fails, do not call
`onSaved`.

The callback should thread through the existing component chain:

`App -> AgentDetail -> AgentDetailContent -> ConversationView -> MediaSettingsModal`

`App` remains the owner of `settingsSnapshot` and updates it directly when
`onSaved` receives a snapshot. If the app already has a helper for applying
settings snapshots, use it; otherwise the implementation can assign
`settingsSnapshot = snapshot` directly. No extra refresh request should be made
for this modal save, because that adds latency and can reintroduce ordering
races.

This keeps ownership explicit and avoids a new state-management layer. The
callback is narrow: it does not expose partial mutation helpers or component
internals. Thread it only through the active chat component path that opens the
modal, not through unrelated settings or side-panel surfaces.

## Backend Contract

The daemon `update_config` behavior remains the source of truth:

- accept `media` as a complete media settings branch;
- validate and deserialize it into `MediaConfig`;
- update the daemon's in-memory config under the config lock;
- save user config;
- return a fresh `SettingsSnapshot`.

No backend API shape change is needed for this issue.

## Config Layering

`media` should be classified as a user-level preference. Workspace config may
provide media defaults, but user config wins when both exist.

The current anonymous tuple used by `load_config` is easy to miss when new
user-level preferences are added. Prefer replacing it with a small private
`UserPreferenceSnapshot` struct if that keeps the code clearer than extending
the tuple. This is an internal readability guard only; do not introduce a
public abstraction or a new config API. The captured fields are:

- `default_provider`
- `default_model`
- `theme`
- `editor_mode`
- `fast_mode`
- `effort_level`
- `copy_full_response`
- `browser`
- `network`
- `media`

After workspace config is merged, restore those captured fields. This is a
targeted cleanup that reduces the chance of missing another user preference in
future changes without changing public config types.

No backward-compatible migration is required.

## Error Handling

If `updateConfig` fails, keep the modal open and show the existing error message.
Do not optimistically mutate app state before the daemon confirms the save.

If `onSaved` runs and parent state updates, the modal can close immediately. The
next open should read the saved values from the updated `settingsSnapshot`.

If media capabilities change between open and save, existing modal validation
continues to control whether save is enabled. This design does not change
capability resolution.

## Testing

Add focused coverage:

- Playwright: extend the existing image-generation settings modal test to seed
  at least two image capabilities, select a non-default provider/model or
  adapter, save, close, reopen, and assert the new provider/model/parameters
  are displayed.
- Playwright: keep the existing assertion that `update_config` receives the
  expected `media` payload and that the fake daemon response is the snapshot
  consumed by the UI.
- Rust daemon test: keep or verify existing coverage that `update_config`
  returns the saved media branch in `response["config"]["media"]`; add a new
  daemon test only if that coverage is missing.
- Rust config test: user media config is not overwritten by workspace config.
- Rust config test: workspace media config applies when user config is absent.

The UI test should use the fake daemon's returned snapshot instead of forcing a
manual settings refresh. That specifically verifies the stale snapshot fix.
Avoid a separate UI test file unless the existing focused test becomes too hard
to read.

## Scope Guard

Implementation should remain in:

- `MediaSettingsModal.svelte`
- the narrow component prop chain from `ConversationView` to `App`
- `puffer-config::load_config`
- targeted tests

If implementation requires a store, event bus, session schema, media runtime
changes, manual settings reload after save, or large settings refactor, stop
and reduce scope.
