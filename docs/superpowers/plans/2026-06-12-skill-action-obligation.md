# Skill Action Obligation Guard — Implementation Plan

> For agentic workers: implement this plan task-by-task. Keep the guard small:
> no workflow DSL, no provider `tool_choice=required`, no short-drama-specific
> runtime path.

**Goal:** Add a generic `requires-action: true` skill frontmatter flag that
prevents promise-only completion after an agentic skill is activated. The guard
allows one corrective reminder, then fails plainly if the model still does not
start tool-driven work.

**Spec:** `docs/superpowers/specs/2026-06-12-skill-action-obligation-design.md`

**Branch:** `feat/video-settings`

**Lean constraints:**

- Activate obligation from successful `ToolInvocation`s, not raw model
  `ToolCallRequest`s.
- Do not recreate the old short-drama internal tool or old
  `short-drama-generation` compatibility.
- Do not add a workflow/phase schema. One boolean frontmatter field is enough.
- Keep loop edits minimal. `agent_loop.rs` is already over the 1000-line repo
  guideline, so put state logic in a new module and avoid adding helper sprawl.

---

## Task 1: Add `requires_action` To Skill Resources

**Purpose:** Make action obligation a declarative skill property with default-off
behavior.

**Files:**

- `crates/puffer-resources/src/model.rs`
- `crates/puffer-resources/src/loader.rs`
- `specs/puffer-resources/111.md`

**Steps:**

- [ ] Add `requires_action: bool` to `SkillSpec` with serde aliases:
  `requires-action` and `requiresAction`.
- [ ] Set `requires_action: false` in `SkillSpec::default()`.
- [ ] Parse `requires-action` / `requiresAction` in both embedded and
  filesystem skill loader paths.
- [ ] Extend `skill_loader_parses_extended_frontmatter_fields` to include
  `requires-action: true` and assert `skill.requires_action`.
- [ ] Add one small default assertion using an existing sample skill or a new
  fixture without the field.
- [ ] Write `specs/puffer-resources/111.md` describing the new frontmatter
  contract and default-off behavior.

**Verification:**

```bash
cargo test -p puffer-resources skill_loader_parses_extended_frontmatter_fields
cargo test -p puffer-resources skill
```

**Commit:** `feat(resources): add skill action obligation flag`

---

## Task 2: Share Skill Tool Input Parsing

**Purpose:** Let the guard identify the activated skill without duplicating
`Skill` tool parsing quirks.

**Files:**

- `crates/puffer-core/runtime/claude_tools/skill.rs`

**Steps:**

- [ ] Replace the private `SkillToolInput` / `normalize_skill_name` flow with a
  small public-in-crate helper, for example:

  ```rust
  pub(crate) fn skill_name_from_tool_input(input: &str) -> Option<String>
  ```

  It should parse the JSON string from `ToolInvocation.input`, apply the same
  `/` and `/skill:` prefix handling as the current executor, and return the
  normalized user-requested skill name.

- [ ] Keep the helper narrow: it should not look up resources, check permissions,
  mutate lambda gates, or render skill prompts.
- [ ] Update `execute_claude_skill_tool` to reuse the helper's parsing path so
  tests prove the guard and executor agree.
- [ ] Add unit tests for:
  - `{"skill":"review-pr"}`
  - `{"skill":"/review-pr"}`
  - `{"skill":"/skill:review-pr"}`
  - invalid JSON / empty skill returns `None` for the guard helper.

**Verification:**

```bash
cargo test -p puffer-core runtime::claude_tools::skill
```

**Commit:** `refactor(core): share skill tool input parsing`

---

## Task 3: Implement `skill_obligation` State Machine

**Purpose:** Put the promise-only guard in a small independent module that can be
tested without provider fake servers.

**Files:**

- `crates/puffer-core/runtime.rs`
- `crates/puffer-core/runtime/skill_obligation.rs`
- `specs/puffer-core/279.md`

**Steps:**

- [ ] Add `mod skill_obligation;` in `crates/puffer-core/runtime.rs`.
- [ ] Create `crates/puffer-core/runtime/skill_obligation.rs`.
- [ ] Implement:

  ```rust
  pub(crate) struct SkillActionObligation { ... }

  pub(crate) enum NoToolDecision {
      Complete,
      ContinueWithReminder(String),
      FailNotStarted(String),
  }
  ```

- [ ] Implement:

  ```rust
  pub(crate) fn observe_invocations(
      &mut self,
      resources: &LoadedResources,
      invocations: &[ToolInvocation],
  );

  pub(crate) fn no_tool_decision(&mut self) -> NoToolDecision;
  ```

- [ ] In `observe_invocations`, only a successful `tool_id == "Skill"` invocation
  may activate pending. Use `skill_name_from_tool_input(&inv.input)` and then
  `skill_by_name(resources, &name)`.
- [ ] Any successful or failed non-`Skill` invocation satisfies pending. The goal
  is to prove the model started tool-driven work, not that the work succeeded.
- [ ] If the same batch contains `Skill(short-drama)` followed by `Write`, the
  state should end as complete.
- [ ] Keep reminder/failure text in this module as constants or small builders.
- [ ] Add unit tests covering:
  - requires-action skill activates pending.
  - non-requires-action skill does nothing.
  - failed Skill invocation does nothing.
  - first no-tool returns reminder.
  - second no-tool returns fail.
  - non-Skill invocation satisfies pending.
  - same-batch Skill + Write satisfies pending.
  - second requires-action skill replaces the pending skill name.
- [ ] Write `specs/puffer-core/279.md` with the runtime contract, hook points,
  and failure semantics.

**Verification:**

```bash
cargo test -p puffer-core skill_obligation
```

**Commit:** `feat(core): add skill action obligation state machine`

---

## Task 4: Wire The Streaming Agent Loop

**Purpose:** Prevent the streaming loop from returning promise-only text after an
obligated skill activates.

**Files:**

- `crates/puffer-core/runtime/agent_loop.rs`
- Existing or new runtime loop tests under `crates/puffer-core/runtime/tests/`

**Steps:**

- [ ] Instantiate `SkillActionObligation` once near the loop's existing
  per-run state.
- [ ] In the no-tool branch, call `no_tool_decision()` before `run_turn_hooks`.
- [ ] For `ContinueWithReminder`:
  - append `turn.pre_tool_items` to `items`;
  - append `ConversationItem::user_message(reminder)`;
  - continue the loop without running hooks or returning.
- [ ] For `FailNotStarted`, return the failure assistant text through the same
  finalization path as ordinary final text.
- [ ] After `execute_tool_batch` returns `new_invocations`, call
  `obligation.observe_invocations(inputs.resources, &new_invocations)`.
- [ ] Preserve cancellation checks, `ToolCallsRequested`, `ToolInvocations`,
  reflection, compaction, and unanimous `terminate` behavior.
- [ ] If line count grows, extract a small existing no-tool/finalization helper
  rather than putting state-machine logic into `agent_loop.rs`.

**Focused test:**

- [ ] Add an OpenAI Responses loop test where:
  - response 1 calls `Skill(short-drama)`;
  - response 2 returns pure text `"I'll start..."`;
  - response 3 calls `Write`;
  - response 4 returns final text.
- [ ] Assert there are 4 upstream requests, the second text did not end the turn,
  and the final `TurnExecution` contains the `Write` invocation.
- [ ] Add a second streaming test where response 3 is still pure text and assert
  the returned assistant text contains `No work was started`.

**Verification:**

```bash
cargo test -p puffer-core openai_responses_skill_action_obligation
```

**Commit:** `feat(core): enforce skill action obligation in streaming loop`

---

## Task 5: Wire The Blocking Loop

**Purpose:** Keep non-streaming Anthropic/spawned-agent paths behaviorally aligned
with the streaming loop.

**Files:**

- `crates/puffer-core/runtime/blocking_loop.rs`
- Existing or new focused runtime tests

**Steps:**

- [ ] Instantiate the same `SkillActionObligation` near blocking-loop state.
- [ ] Mirror the no-tool branch behavior from Task 4:
  `Complete`, `ContinueWithReminder`, `FailNotStarted`.
- [ ] Append `turn.pre_tool_items` before appending reminder on
  `ContinueWithReminder`.
- [ ] Observe `new_invocations` after each tool batch.
- [ ] Preserve usage accounting, cancellation, reflection, compaction, and
  unanimous `terminate` behavior.

**Focused test:**

- [ ] Add the smallest practical blocking-loop coverage. Prefer a direct fake
  `TurnSession` unit-style test if available; otherwise use the existing
  provider test harness.
- [ ] Cover at least: `Skill(short-drama)` succeeds, next turn pure text,
  reminder continues, then a non-Skill tool satisfies pending.

**Verification:**

```bash
cargo test -p puffer-core blocking_loop_skill_action_obligation
```

**Commit:** `feat(core): enforce skill action obligation in blocking loop`

---

## Task 6: Enable The Guard For Short Drama

**Purpose:** Restore the short-drama promise-only guard while keeping short-drama
as a pure skill.

**Files:**

- `resources/skills/short-drama/SKILL.md`
- `crates/puffer-resources/tests/media_generation_skills.rs`

**Steps:**

- [ ] Add `requires-action: true` to short-drama frontmatter.
- [ ] Extend the short-drama resource/frontmatter test to assert the field is
  present. If the local test struct only checks selected fields, add
  `requires_action: Option<bool>` or `bool` with serde alias.
- [ ] Do not add short-drama-specific runtime code.

**Verification:**

```bash
cargo test -p puffer-resources --test media_generation_skills short_drama
```

**Commit:** `feat(skill): require action after short-drama activation`

---

## Task 7: Run The Focused Gate

**Purpose:** Catch cross-crate fallout without running the full workspace first.

**Steps:**

- [ ] Run resources tests:

  ```bash
  cargo test -p puffer-resources skill
  cargo test -p puffer-resources --test media_generation_skills
  ```

- [ ] Run core focused tests:

  ```bash
  cargo test -p puffer-core skill_obligation
  cargo test -p puffer-core skill_action_obligation
  cargo test -p puffer-core runtime::claude_tools::skill
  ```

- [ ] Run a wider core runtime slice if focused tests pass:

  ```bash
  cargo test -p puffer-core runtime::tests
  cargo test -p puffer-core runtime::tests::agent_loop_e2e
  ```

**Acceptance:**

- Non-obligated skills still finish normally on no-tool text.
- Obligated skills cannot finish with promise-only text immediately after
  activation.
- One reminder is the maximum retry; the second no-tool response fails plainly.

**Commit:** no separate commit unless test-only fixes were needed after prior
commits.

---

## Task 8: Final Review Before Implementation PR

**Steps:**

- [ ] Check `git diff --stat` and confirm no unrelated files changed.
- [ ] Check Rust file lengths for touched runtime/resource files:

  ```bash
  wc -l crates/puffer-core/runtime/agent_loop.rs \
        crates/puffer-core/runtime/blocking_loop.rs \
        crates/puffer-core/runtime/skill_obligation.rs \
        crates/puffer-resources/src/model.rs \
        crates/puffer-resources/src/loader.rs
  ```

- [ ] If `agent_loop.rs` grows further, consider moving finalization helper code
  into a small module or local helper as part of this change. Do not refactor
  unrelated loop behavior.
- [ ] Review the actual final prompt injected by `ContinueWithReminder` for
  clarity and absence of provider-specific language.
- [ ] Run:

  ```bash
  git diff --check
  ```

- [ ] If time allows, run:

  ```bash
  cargo test --workspace
  ```

**Final commit if needed:** `test(core): cover skill action obligation guard`

