# Video Generation Failure Contract Implementation Plan

> **For agentic workers:** implement this plan task-by-task. Use checkbox
> (`- [ ]`) syntax for tracking progress. Keep the change narrow: no retry
> policy, no provider health, no default-provider changes, and no media runtime
> redesign.

**Goal:** Make `videogen` output self-contained for failed remote video jobs by
returning persisted job diagnostics: `providerJobId`, `remoteStatus`, and
`error`.

**Architecture:** Keep `MediaJob` as the persisted source of truth. Add optional
diagnostic fields to `ExactMediaGenerationResult`, populate them in the shared
media result builder, and expose them from the `VideoGeneration` workflow JSON.
Always include the three diagnostic keys in workflow output; use `null` when
absent.

**Spec:** `docs/superpowers/specs/2026-06-12-video-generation-failure-contract-design.md`

---

## File Structure

- Modify: `crates/puffer-media/src/runtime.rs`  
  Add exact media diagnostic fields and populate them from `MediaJob`.
- Modify: `crates/puffer-media/src/runtime_tests.rs`  
  Add focused result-shaping coverage for failed video jobs.
- Modify: `crates/puffer-media/src/media/worldrouter_video_tests.rs`  
  Add focused remote-failure persistence coverage if no equivalent test exists.
- Modify: `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs`  
  Emit diagnostic keys in the `videogen` JSON output and add workflow output
  coverage.

---

## Task 1: Extend Exact Media Result Shape

**Files:**

- `crates/puffer-media/src/runtime.rs`
- `crates/puffer-media/src/runtime_tests.rs`

- [ ] Add optional fields to `ExactMediaGenerationResult`:

```rust
pub provider_job_id: Option<String>,
pub remote_status: Option<String>,
pub error: Option<String>,
```

- [ ] Populate the fields in `exact_media_generation_result(job, artifacts)`
      from `job.provider_job_id`, `job.remote_status`, and `job.error`.
- [ ] Do not add these fields to `ExactImageGenerationResult`.
- [ ] Do not expose `remote_get_url`, prompt text, adapter id, raw payloads, or
      provider response bodies.
- [ ] Add a `puffer-media` runtime test that builds a failed video `MediaJob`
      with a provider job id, remote status, and error, then asserts the exact
      media result carries those fields.
- [ ] Run:

```bash
cargo test -p puffer-media exact_media_generation
```

Expected: focused runtime tests pass.

---

## Task 2: Lock WorldRouter Failure Persistence

**Files:**

- `crates/puffer-media/src/media/worldrouter_video_tests.rs`

- [ ] Add or adjust a WorldRouter adapter test where polling returns:

```json
{
  "id": "task-123",
  "status": "failed",
  "error": {
    "message": "The service encountered an unexpected internal error."
  }
}
```

- [ ] Assert the resulting job has:
  - `status == MediaJobStatus::Failed`
  - `provider_job_id == Some("task-123")` or the submitted task id used by the
    fixture
  - `remote_status == Some("failed")`
  - `error == Some("The service encountered an unexpected internal error.")`
- [ ] Keep this test provider-specific. Do not add a generic error taxonomy.
- [ ] Run:

```bash
cargo test -p puffer-media worldrouter_video
```

Expected: WorldRouter adapter tests pass.

---

## Task 3: Emit Diagnostics From VideoGeneration

**Files:**

- `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs`

- [ ] Extend `video_generation_output` to include:

```json
"providerJobId": result.provider_job_id,
"remoteStatus": result.remote_status,
"error": result.error
```

- [ ] Include the keys on every response. Let `serde_json::json!` serialize
      missing values as `null`.
- [ ] Keep existing `jobId`, `kind`, `requestedCount`, `artifacts`, `provider`,
      `model`, `status`, `parameters`, and `purpose` fields unchanged.
- [ ] Add a unit test in the existing `video_generation.rs` test module that
      constructs an `ExactMediaGenerationResult` with:
  - `status: "failed"`
  - `provider_job_id: Some(...)`
  - `remote_status: Some("failed")`
  - `error: Some(...)`
  and asserts the output JSON contains `providerJobId`, `remoteStatus`, and
  `error`.
- [ ] Update the existing successful execution test to assert the same three
      keys are present and equal to JSON `null` when absent.
- [ ] Run:

```bash
cargo test -p puffer-core video_generation
```

Expected: `VideoGeneration` tests pass and successful artifact assertions remain
unchanged.

---

## Task 4: Focused Verification

- [ ] Run the focused media/core test set:

```bash
cargo test -p puffer-media exact_media_generation
cargo test -p puffer-media worldrouter_video
cargo test -p puffer-core video_generation
```

- [ ] Run a broader compile/test pass for touched crates if the focused tests
      reveal shared type fallout:

```bash
cargo test -p puffer-media
cargo test -p puffer-core
```

- [ ] Inspect the final diff and verify it does not include:
  - retry logic
  - provider health state
  - default provider/model changes
  - desktop UI redesign
  - image generation behavior changes
  - `remoteGetUrl`, prompt, adapter id, raw payload, or credential exposure in
    `videogen` output

---

## Task 5: Commit

- [ ] Stage only the intended implementation and test files.
- [ ] Commit with:

```bash
git commit -m "fix(media): surface video job failure diagnostics"
```

Final expected behavior: a Milhous-style failed WorldRouter video job returns
`status: "failed"` plus `providerJobId`, `remoteStatus`, and `error` directly in
the `videogen` JSON output.

