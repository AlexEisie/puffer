# Chat Composer Autosize Design

## Summary

The primary Chat composer in Corbina should grow vertically as the user types
multi-line content, then cap at a fixed maximum height and scroll internally for
very long prompts. The change is limited to the main agent chat composer in
`apps/puffer-desktop/src/lib/screens/agent/ConversationView.svelte`.

This deliberately does not introduce a shared composer component or apply the
behavior to the mini window, Ask Puffer, or legacy `ConversationPane`. The goal
is to improve the main coding chat input while keeping the implementation small,
stable, and easy to reason about.

## Reference

`agentenv-monorepo/packages/agent-sdk/src/components/Composer.tsx` uses a simple
native textarea autosize pattern:

- keep the textarea as a native `<textarea>`
- set height to `auto`
- read `scrollHeight`
- apply a clamped pixel height
- cap very long input and let the textarea scroll internally

Puffer should adopt this core behavior only. It should not copy unrelated
agent-sdk features such as file mentions, highlighted overlay text, scheduling
controls, or a generalized component hierarchy.

## Current State

`ConversationView.svelte` renders the main composer textarea as:

- `value={draft}`
- `oninput={(event) => updateDraft(event.currentTarget.value)}`
- `onkeydown={onKeydown}`
- `disabled={composerDisabled}`

The existing visual styling comes from `src/lib/design/chat.css`:

- `.pf-composer textarea`
- `resize: none`
- `min-height: 26px`
- `max-height: 200px`

Because no runtime height is applied, long prompts do not expand the composer
before the max-height scroll behavior is reached.

## Goals

- Grow the main Chat composer textarea as content grows.
- Keep the compact height for empty and short prompts.
- Cap growth at a fixed maximum height and use internal vertical scrolling after
  that point.
- Preserve existing keyboard behavior: Enter sends, Shift+Enter inserts a
  newline, IME composition Enter does not submit.
- Preserve existing draft persistence, per-session draft isolation, attachment
  handling, model controls, permission controls, and submit payloads.
- Keep the change local and minimal.

## Non-Goals

- Do not refactor `ConversationView.svelte` into a new composer component.
- Do not add a shared Svelte action/helper yet.
- Do not modify mini window, Ask Puffer, legacy `ConversationPane`, settings
  textareas, file editor textareas, or workflow textareas.
- Do not use contenteditable.
- Do not depend on browser-native `field-sizing` support.

## Design

Add local constants in `ConversationView.svelte`:

- `COMPOSER_MIN_HEIGHT_PX`
- `COMPOSER_MAX_HEIGHT_PX`

Add a local textarea ref:

- `composerTextareaEl: HTMLTextAreaElement | undefined`

Add two local functions:

- `resizeComposerTextarea(textarea = composerTextareaEl)`
- `scheduleComposerResize()`

`resizeComposerTextarea` sets the textarea height to `auto`, reads
`scrollHeight`, clamps the value between the min and max constants, and writes
the final pixel height back to the element. It also sets the element's vertical
overflow so text scrolls only once content exceeds the cap.

`scheduleComposerResize` waits until the current Svelte DOM update has landed
before measuring. It can use `tick()` and should avoid redundant work when the
textarea is unavailable.

Wire the behavior into the existing textarea:

- bind the textarea with `bind:this={composerTextareaEl}`
- on input, update the draft and resize the event target
- when `draft` changes programmatically, schedule a resize
- when switching sessions, schedule a resize after the restored draft is applied
- when submit clears the draft, schedule a resize
- when a failed submit restores the previous draft, schedule a resize after
  restoration

The submit request shape and message formatting remain unchanged.

## CSS Contract

The existing `.pf-composer textarea` rule should continue owning typography and
visual styling. It should keep:

- `resize: none`
- transparent background
- no border
- current font and padding

The CSS max-height should either match `COMPOSER_MAX_HEIGHT_PX` or be removed if
the runtime clamp becomes the sole height cap. The runtime and CSS caps must not
disagree.

## Performance

The design performs one synchronous `scrollHeight` read per input event and for
rare programmatic draft changes. This is acceptable because it touches one
textarea only, not the transcript. No ResizeObserver, MutationObserver, hidden
mirror element, or debounce is needed.

The implementation should avoid measuring in loops and should not introduce a
global layout observer.

## Edge Cases

- Empty draft: height returns to the minimum.
- Short single-line draft: height remains compact.
- Multi-line draft: height grows up to the cap.
- Very long draft: height stays capped and the textarea scrolls internally.
- Session switch: restored draft gets the correct height for that session.
- Submit accepted: cleared draft collapses the composer.
- Submit rejected or failed: restored draft gets the correct height.
- Attachments: preview strip layout stays above the textarea and is not part of
  textarea height measurement.
- Disabled composer: height still represents the current draft value.

## Testing

Add a focused Playwright test to `apps/puffer-desktop/tests/chat-session-ui.spec.ts`.

The test should:

1. Open a chat session with an enabled composer.
2. Capture the initial textarea bounding-box height.
3. Fill a prompt with several newline-separated lines.
4. Assert the textarea height is greater than the initial height.
5. Fill a very long multi-line prompt.
6. Assert the height is capped near the configured max and that
   `scrollHeight > clientHeight`.
7. Submit or clear the draft.
8. Assert the textarea height returns near the initial height.

If implementation touches session restoration directly, add a small assertion
that switching away and back to a session restores both the draft value and a
matching grown height.

Existing IME, draft recovery, attachment, and submit tests should continue to
pass without semantic updates.
