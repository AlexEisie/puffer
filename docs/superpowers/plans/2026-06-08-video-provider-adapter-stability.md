# Video Provider Adapter Stability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the Relaydance video polling failure, replace the misleading `openai_video` adapter id, and add BytePlus text-to-video only when provider probes prove an executable protocol.

**Architecture:** Keep `VideoGeneration` as a thin workflow tool over exact media runtime. Rename the current gateway adapter to `relaydance_video`, parse Relaydance responses from redacted fixtures, and keep BytePlus behind a hard verification gate. Extract shared video helpers only after two verified adapters expose concrete duplicated code.

**Tech Stack:** Rust (`puffer-core`, `puffer-provider-registry`, `puffer-resources`), YAML provider resources, Cargo unit tests, local `.puffer` auth for gated provider probes.

**Spec:** `docs/superpowers/specs/2026-06-08-video-provider-adapter-stability-design.md`

**External API References:** BytePlus ModelArk contents generation task docs:
`https://docs.byteplus.com/en/docs/modelark/1520757` and
`https://docs.byteplus.com/en/docs/modelark/1521309`.

---

## Recheck Outcome

The reviewed design removes two sources of over-design:

- Do not build a generic video lifecycle framework before BytePlus is verified.
- Do not introduce `newapi_video` from one Relaydance integration.

The implementation must first repair Relaydance with a provider-specific
`relaydance_video` adapter. BytePlus is conditional: if credentials, cost
approval, or probe evidence are unavailable, the implementation keeps BytePlus
in audit-only status and stops before BytePlus code.

## File Structure

- Modify: `docs/superpowers/specs/2026-06-08-video-provider-adapter-stability-design.md`
  - Records tightened scope and probe gates.
- Create: `crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json`
  - Redacted Relaydance poll response fixture captured before parser work.
- Create if BytePlus probe passes: `crates/puffer-core/runtime/media/fixtures/byteplus_submit_task.json`
  - Redacted BytePlus submit response fixture.
- Create if BytePlus probe passes: `crates/puffer-core/runtime/media/fixtures/byteplus_poll_task.json`
  - Redacted BytePlus poll response fixture.
- Rename: `crates/puffer-core/runtime/media/openai_video.rs` -> `crates/puffer-core/runtime/media/relaydance_video.rs`
  - Owns Relaydance request building, response parsing, polling, and artifact persistence.
- Modify: `crates/puffer-core/runtime/media/mod.rs`
  - Exports `relaydance_video` instead of `openai_video`.
- Modify: `crates/puffer-core/media_runtime.rs`
  - Routes `relaydance_video`; removes the `openai_video` match arm.
- Modify: `crates/puffer-core/runtime/media/resolver.rs`
  - Marks `MediaExecutionKind::RelaydanceVideo` executable for video and maps the adapter id.
- Modify: `crates/puffer-provider-registry/src/model.rs`
  - Replaces `OpenAiVideo` with `RelaydanceVideo`.
- Modify: `crates/puffer-provider-registry/src/model_tests.rs`
  - Tests `relaydance_video` parsing and `openai_video` rejection.
- Modify: `resources/providers/relaydance.yaml`
  - Uses `adapter: relaydance_video`.
- Create if BytePlus probe passes: `crates/puffer-core/runtime/media/byteplus_video.rs`
  - Owns BytePlus text-to-video protocol.
- Modify if BytePlus probe passes: `resources/providers/byteplus.yaml`
  - Declares only verified text-to-video models.
- Create: `specs/puffer-core/257.md`
  - Component update spec for video adapter stability.
- Create: `specs/puffer-provider-registry/08.md`
  - Component update spec for the renamed media execution kind.
- Create: `specs/puffer-resources/94.md`
  - Component update spec for provider video resource declarations.

Do not modify desktop Svelte files in this plan. Existing dirty desktop files
are unrelated and must remain untouched.

---

## Task 1: Provider Evidence Gate

**Files:**
- Create: `crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json`
- Create if BytePlus probe passes: `crates/puffer-core/runtime/media/fixtures/byteplus_submit_task.json`
- Create if BytePlus probe passes: `crates/puffer-core/runtime/media/fixtures/byteplus_poll_task.json`
- Modify: `docs/superpowers/specs/2026-06-08-video-provider-adapter-stability-design.md`

- [ ] **Step 1: Verify local auth keys without printing secrets**

Run:

```bash
jq -r '
  {
    relaydance: (.providers.relaydance.kind // "missing"),
    byteplus: (.providers.byteplus.kind // "missing")
  }
' "$HOME/.puffer/auth.json"
```

Expected:

```json
{
  "relaydance": "api_key",
  "byteplus": "api_key"
}
```

If `byteplus` is missing, keep BytePlus audit-only and skip all BytePlus tasks.
Relaydance is required for this plan; if Relaydance auth is missing, stop and
ask the user to configure it.

- [ ] **Step 2: Capture the existing Relaydance poll response**

Run this only after network approval:

```bash
set -euo pipefail
mkdir -p crates/puffer-core/runtime/media/fixtures
RELAYDANCE_KEY="$(jq -r '.providers.relaydance.key' "$HOME/.puffer/auth.json")"
curl -sS \
  -H "Authorization: Bearer ${RELAYDANCE_KEY}" \
  "https://relaydance.com/v1/video/generations/task_TA20QV69xGqQXapyyM3ynyB1elNki8pg" \
  | jq '
      walk(
        if type == "object" then
          with_entries(
            if (.key | test("authorization|token|secret|key"; "i")) then
              .value = "[redacted-secret]"
            elif (.key | test("url|uri|href"; "i")) and (.value | type == "string") then
              .value = "[redacted-url]"
            else
              .
            end
          )
        else
          .
        end
      )
    ' > crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json
```

Expected:

```bash
jq -e 'type == "object"' crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json
```

returns exit code `0`.

- [ ] **Step 3: Audit the Relaydance fixture for secrets**

Run:

```bash
rg -n "sk-|Bearer|Authorization|api[_-]?key|token|secret|https?://" crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json
```

Expected: no output.

If this command prints a line, redact that field and rerun the command until it
prints nothing.

- [ ] **Step 4: Decide whether Relaydance submit fixture is needed**

Run:

```bash
jq -r 'keys_unsorted | join(",")' crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json
```

If the fixture contains enough fields to identify status and terminal output,
do not submit a new Relaydance task. If it does not contain enough fields
because the task expired or the provider returned a not-found envelope, ask the
user before submitting a minimal paid text-to-video task.

- [ ] **Step 5: Capture BytePlus fixtures only if approved**

Run this only if BytePlus auth exists and the user approves the network/cost
tradeoff. BytePlus ModelArk video generation uses
`/api/v3/contents/generations/tasks` for task creation and
`/api/v3/contents/generations/tasks/{id}` for retrieval.

First confirm the candidate model id exists in Puffer's local discovery cache:

```bash
jq -e '.entries.byteplus.models[]?.id | select(. == "dreamina-seedance-2-0-260128")' "$HOME/.puffer/model_discovery_cache.json"
```

Expected: prints `"dreamina-seedance-2-0-260128"` and exits `0`.

If the local cache does not contain the candidate, refresh evidence from the
existing BytePlus `/models` discovery endpoint before any paid video task:

```bash
BYTEPLUS_KEY="$(jq -r '.providers.byteplus.key' "$HOME/.puffer/auth.json")"
curl -sS \
  -H "Authorization: Bearer ${BYTEPLUS_KEY}" \
  "https://ark.ap-southeast.bytepluses.com/api/v3/models" \
  | jq -e '.data[]?.id | select(. == "dreamina-seedance-2-0-260128")'
```

Expected: prints `"dreamina-seedance-2-0-260128"` and exits `0`. If discovery
does not list that id, skip BytePlus for this pass.

After the model id is confirmed, run:

```bash
set -euo pipefail
mkdir -p crates/puffer-core/runtime/media/fixtures
BYTEPLUS_KEY="$(jq -r '.providers.byteplus.key' "$HOME/.puffer/auth.json")"
BYTEPLUS_RAW_SUBMIT="/tmp/puffer-byteplus-video-submit.json"
curl -sS \
  -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${BYTEPLUS_KEY}" \
  "https://ark.ap-southeast.bytepluses.com/api/v3/contents/generations/tasks" \
  -d '{
    "model": "dreamina-seedance-2-0-260128",
    "content": [
      {
        "type": "text",
        "text": "A calm sunrise over a mountain lake, cinematic camera move."
      }
    ]
  }' > "${BYTEPLUS_RAW_SUBMIT}"
jq '
  walk(
    if type == "object" then
      with_entries(
        if (.key | test("authorization|token|secret|key"; "i")) then
          .value = "[redacted-secret]"
        elif (.key | test("url|uri|href"; "i")) and (.value | type == "string") then
          .value = "[redacted-url]"
        else
          .
        end
      )
    else
      .
    end
  )
' "${BYTEPLUS_RAW_SUBMIT}" > crates/puffer-core/runtime/media/fixtures/byteplus_submit_task.json
BYTEPLUS_TASK_ID="$(jq -r '.id // empty' "${BYTEPLUS_RAW_SUBMIT}")"
test -n "${BYTEPLUS_TASK_ID}"
sleep 10
curl -sS \
  -H "Authorization: Bearer ${BYTEPLUS_KEY}" \
  "https://ark.ap-southeast.bytepluses.com/api/v3/contents/generations/tasks/${BYTEPLUS_TASK_ID}" \
  | jq '
      walk(
        if type == "object" then
          with_entries(
            if (.key | test("authorization|token|secret|key"; "i")) then
              .value = "[redacted-secret]"
            elif (.key | test("url|uri|href"; "i")) and (.value | type == "string") then
              .value = "[redacted-url]"
            else
              .
            end
          )
        else
          .
        end
      )
    ' > crates/puffer-core/runtime/media/fixtures/byteplus_poll_task.json
rm -f "${BYTEPLUS_RAW_SUBMIT}"
```

After capture, run:

```bash
rg -n "sk-|Bearer|Authorization|api[_-]?key|token|secret|https?://" crates/puffer-core/runtime/media/fixtures/byteplus_*.json
```

Expected: no output.

If BytePlus cannot be probed, skip every later step marked "if BytePlus probe
passes".

- [ ] **Step 6: Commit provider evidence**

If BytePlus was skipped, run:

```bash
git add -f crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json
git commit -m "test(media): add video provider response fixtures"
```

If BytePlus fixtures were captured, run:

```bash
git add -f crates/puffer-core/runtime/media/fixtures/relaydance_poll_task.json
git add -f crates/puffer-core/runtime/media/fixtures/byteplus_submit_task.json crates/puffer-core/runtime/media/fixtures/byteplus_poll_task.json
git commit -m "test(media): add video provider response fixtures"
```

Expected: commit includes only fixture files.

---

## Task 2: Rename Adapter Contract From openai_video To relaydance_video

**Files:**
- Modify: `crates/puffer-provider-registry/src/model.rs`
- Modify: `crates/puffer-provider-registry/src/model_tests.rs`
- Modify: `crates/puffer-core/runtime/media/resolver.rs`
- Modify: `resources/providers/relaydance.yaml`
- Rename: `crates/puffer-core/runtime/media/openai_video.rs` -> `crates/puffer-core/runtime/media/relaydance_video.rs`
- Modify: `crates/puffer-core/runtime/media/mod.rs`
- Modify: `crates/puffer-core/media_runtime.rs`
- Modify: `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs`

- [ ] **Step 1: Add failing provider-registry tests**

In `crates/puffer-provider-registry/src/model_tests.rs`, replace the current
`media_execution_kind_parses_openai_video` test with:

```rust
#[test]
fn media_execution_kind_parses_relaydance_video() {
    let kind: MediaExecutionKind = serde_yaml::from_str("relaydance_video").expect("parse");
    assert_eq!(kind, MediaExecutionKind::RelaydanceVideo);
}

#[test]
fn media_execution_kind_rejects_openai_video() {
    let error = serde_yaml::from_str::<MediaExecutionKind>("openai_video").unwrap_err();
    assert!(error.to_string().contains("unknown variant"));
}
```

- [ ] **Step 2: Run provider-registry tests and verify failure**

Run:

```bash
cargo test -p puffer-provider-registry media_execution_kind_
```

Expected: fails because `RelaydanceVideo` does not exist and `openai_video`
still parses.

- [ ] **Step 3: Rename the enum variant**

In `crates/puffer-provider-registry/src/model.rs`, replace:

```rust
#[serde(rename = "openai_video")]
OpenAiVideo,
```

with:

```rust
RelaydanceVideo,
```

`#[serde(rename_all = "snake_case")]` makes this parse as `relaydance_video`.

- [ ] **Step 4: Update resolver adapter mapping tests**

In `crates/puffer-core/runtime/media/resolver.rs`, update tests that mention
OpenAI video so the adapter is Relaydance-specific. The test names should use:

```rust
fn relaydance_video_execution_adapter_is_available()
fn connected_relaydance_video_descriptor_is_available()
```

Expected assertions:

```rust
assert!(execution_adapter_is_available_for_kind(
    MediaKind::Video,
    MediaExecutionKind::RelaydanceVideo
));
assert_eq!(capabilities[0].adapter, "relaydance_video");
```

- [ ] **Step 5: Update resolver implementation**

In `crates/puffer-core/runtime/media/resolver.rs`, replace adapter matching:

```rust
(MediaKind::Video, MediaExecutionKind::OpenAiVideo)
```

with:

```rust
(MediaKind::Video, MediaExecutionKind::RelaydanceVideo)
```

and replace `adapter_id` mapping:

```rust
MediaExecutionKind::OpenAiVideo => "openai_video",
```

with:

```rust
MediaExecutionKind::RelaydanceVideo => "relaydance_video",
```

- [ ] **Step 6: Rename the runtime module and types**

Run:

```bash
git mv crates/puffer-core/runtime/media/openai_video.rs crates/puffer-core/runtime/media/relaydance_video.rs
```

In the renamed file, rename public crate-local symbols:

```rust
OpenAiVideoRequest -> RelaydanceVideoRequest
openai_video_request_from_parameters -> relaydance_video_request_from_parameters
OpenAiVideoTransport -> RelaydanceVideoTransport
ReqwestOpenAiVideoTransport -> ReqwestRelaydanceVideoTransport
OpenAiVideoTask -> RelaydanceVideoTask
OpenAiVideoPollingConfig -> RelaydanceVideoPollingConfig
OpenAiVideoAdapter -> RelaydanceVideoAdapter
OPENAI_VIDEO_ADAPTER -> RELAYDANCE_VIDEO_ADAPTER
```

Set the adapter constant to:

```rust
const RELAYDANCE_VIDEO_ADAPTER: &str = "relaydance_video";
```

- [ ] **Step 7: Update module exports and media runtime imports**

In `crates/puffer-core/runtime/media/mod.rs`, replace:

```rust
pub(crate) mod openai_video;
```

with:

```rust
pub(crate) mod relaydance_video;
```

In `crates/puffer-core/media_runtime.rs`, replace the import with:

```rust
use crate::runtime::media::relaydance_video::{
    relaydance_video_request_from_parameters, RelaydanceVideoAdapter,
    RelaydanceVideoPollingConfig,
};
```

Replace the match arm:

```rust
"openai_video" => {
```

with:

```rust
"relaydance_video" => {
```

and update constructor/function names in that arm to the Relaydance names.

- [ ] **Step 8: Update Relaydance YAML**

In `resources/providers/relaydance.yaml`, replace:

```yaml
adapter: openai_video
```

with:

```yaml
adapter: relaydance_video
```

- [ ] **Step 9: Update video generation tests**

In `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs`,
replace test expectations:

```rust
adapter: "openai_video".to_string(),
assert_eq!(request.adapter, "openai_video");
```

with:

```rust
adapter: "relaydance_video".to_string(),
assert_eq!(request.adapter, "relaydance_video");
```

Rename helper `spawn_openai_video_server` to:

```rust
fn spawn_relaydance_video_server() -> (String, thread::JoinHandle<Vec<String>>)
```

- [ ] **Step 10: Run focused rename tests**

Run:

```bash
cargo test -p puffer-provider-registry media_execution_kind_
cargo test -p puffer-core relaydance_video
cargo test -p puffer-core video_generation
cargo test -p puffer-core media::resolver
```

Expected: provider-registry tests pass; puffer-core tests compile far enough to
surface parser expectations addressed in Task 3.

- [ ] **Step 11: Commit adapter rename**

Run:

```bash
git add crates/puffer-provider-registry/src/model.rs crates/puffer-provider-registry/src/model_tests.rs
git add crates/puffer-core/runtime/media/mod.rs crates/puffer-core/runtime/media/resolver.rs crates/puffer-core/media_runtime.rs
git add crates/puffer-core/runtime/media/relaydance_video.rs crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs
git add resources/providers/relaydance.yaml
git commit -m "refactor(media): rename relaydance video adapter"
```

---

## Task 3: Fix Relaydance Response Parsing And Job Failure State

**Files:**
- Modify: `crates/puffer-core/runtime/media/relaydance_video.rs`
- Test: `crates/puffer-core/runtime/media/relaydance_video.rs`

- [ ] **Step 1: Add fixture-backed parser tests**

In `crates/puffer-core/runtime/media/relaydance_video.rs`, add tests that load
the captured fixture:

```rust
fn relaydance_poll_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/relaydance_poll_task.json")).expect("fixture")
}

#[test]
fn parses_relaydance_poll_fixture() {
    let task = RelaydanceVideoTask::from_value(relaydance_poll_fixture(), "poll video task")
        .expect("task");
    assert!(!task.id.trim().is_empty());
    assert!(!task.status.trim().is_empty());
}

#[test]
fn relaydance_shape_summary_lists_top_level_keys() {
    let summary = relaydance_response_shape_summary(&relaydance_poll_fixture());
    assert!(summary.contains("keys=["));
}

#[test]
fn parses_relaydance_completed_task_with_task_id_and_url() {
    let task = RelaydanceVideoTask::from_value(
        json!({
            "task_id": "task-1",
            "status": "succeeded",
            "url": "https://example.com/video.mp4"
        }),
        "poll video task",
    )
    .expect("task");

    assert_eq!(task.id, "task-1");
    assert_eq!(task.status, "succeeded");
    assert_eq!(task.video_url.as_deref(), Some("https://example.com/video.mp4"));
}
```

- [ ] **Step 2: Add missing-id diagnostic test**

Add:

```rust
#[test]
fn relaydance_missing_task_id_reports_phase_and_keys() {
    let error = RelaydanceVideoTask::from_value(
        json!({
            "code": "ok",
            "message": "accepted",
            "data": { "status": "running" }
        }),
        "poll video task",
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("poll video task response missing task id"));
    assert!(error.contains("keys=[code,data,message]"));
}
```

- [ ] **Step 3: Add parser-failure marks-job-failed test**

Add a scripted transport test:

```rust
#[test]
fn poll_parser_failure_marks_job_failed() {
    let dir = tempfile::tempdir().unwrap();
    let service = MediaGenerationService::new(dir.path());
    let adapter = RelaydanceVideoAdapter::with_transport(
        "token",
        "https://relaydance.com/v1/video/generations",
        "relaydance",
        ScriptedTransport {
            submit: json!({ "id": "task-1", "status": "queued" }),
            polls: RefCell::new(vec![json!({ "data": { "status": "running" } })]),
        },
    );
    let request = RelaydanceVideoRequest {
        model: "m".into(),
        prompt: "a cat".into(),
        params: vec![],
    };
    let job = adapter
        .submit(&service, request, BTreeMap::new(), 1)
        .expect("submit");

    let error = adapter.poll(&service, job.clone(), 2).unwrap_err().to_string();
    assert!(error.contains("poll video task response missing task id"));

    let saved = service.load_job(&job.id).expect("saved job");
    assert_eq!(saved.status, MediaJobStatus::Failed);
    assert!(saved.error.as_deref().is_some_and(|value| {
        value.contains("poll video task response missing task id")
    }));
    assert!(saved
        .error
        .as_deref()
        .is_some_and(|value| value.contains("provider=relaydance")));
    assert!(saved
        .error
        .as_deref()
        .is_some_and(|value| value.contains("adapter=relaydance_video")));
    assert!(saved
        .error
        .as_deref()
        .is_some_and(|value| value.contains("task=task-1")));
}
```

- [ ] **Step 4: Run focused parser tests and verify failure**

Run:

```bash
cargo test -p puffer-core relaydance_video -- --nocapture
```

Expected: parser fixture or failed-job tests fail with current top-level-only
logic.

- [ ] **Step 5: Implement provider error and shape summary helpers**

Add helpers near the parser:

```rust
fn relaydance_response_shape_summary(value: &serde_json::Value) -> String {
    let keys = value
        .as_object()
        .map(|object| {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys.join(",")
        })
        .unwrap_or_else(|| value_type_name(value).to_string());
    format!("keys=[{keys}]")
}

fn value_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn relaydance_error_message(value: &serde_json::Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .or_else(|| value.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_string)
}

fn relaydance_task_error_context(
    provider_id: &str,
    task_id: Option<&str>,
    error: &anyhow::Error,
) -> String {
    format!(
        "{error:#}: provider={provider_id} adapter={RELAYDANCE_VIDEO_ADAPTER} task={}",
        task_id.unwrap_or("unknown")
    )
}
```

- [ ] **Step 6: Implement fixture-backed task parser**

Change the parser signature to include phase:

```rust
impl RelaydanceVideoTask {
    pub(crate) fn from_value(value: Value, phase: &str) -> Result<Self> {
        let body = value.get("data").unwrap_or(&value);
        let id = string_field(body, &["id", "task_id"])
            .or_else(|| string_field(&value, &["id", "task_id"]))
            .ok_or_else(|| {
                anyhow!(
                    "{phase} response missing task id: {}",
                    relaydance_response_shape_summary(&value)
                )
            })?;
        let status = string_field(body, &["status"])
            .or_else(|| string_field(&value, &["status"]))
            .ok_or_else(|| {
                anyhow!(
                    "{phase} response missing status: {}",
                    relaydance_response_shape_summary(&value)
                )
            })?;
        let video_url = relaydance_video_url(body).or_else(|| relaydance_video_url(&value));
        let error = relaydance_error_message(body).or_else(|| relaydance_error_message(&value));
        Ok(Self {
            id,
            status,
            video_url,
            error,
        })
    }
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn relaydance_video_url(value: &Value) -> Option<String> {
    value
        .get("metadata")
        .and_then(|metadata| metadata.get("url"))
        .or_else(|| value.get("url"))
        .or_else(|| value.get("video_url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
```

Do not add additional task id, status, or URL field names unless they appear in
the Relaydance fixture or in protocol documentation cited by the implementation
commit.

- [ ] **Step 7: Pass phase into submit and poll parsing**

In submit:

```rust
let task = RelaydanceVideoTask::from_value(response, "submit video task").with_context(|| {
    format!(
        "provider={} adapter={RELAYDANCE_VIDEO_ADAPTER} task=unknown",
        self.provider_id
    )
})?;
```

In poll:

```rust
let task = match RelaydanceVideoTask::from_value(response, "poll video task") {
    Ok(task) => task,
    Err(error) => {
        let mut failed = job;
        let diagnostic = relaydance_task_error_context(
            &self.provider_id,
            failed.provider_job_id.as_deref(),
            &error,
        );
        failed.error = Some(diagnostic.clone());
        failed.transition(MediaJobStatus::Failed, now_ms)?;
        service.save_job(&failed)?;
        return Err(anyhow!(diagnostic));
    }
};
```

- [ ] **Step 8: Run focused tests**

Run:

```bash
cargo test -p puffer-core relaydance_video -- --nocapture
cargo test -p puffer-core video_generation -- --nocapture
```

Expected: all Relaydance and VideoGeneration tests pass.

- [ ] **Step 9: Commit Relaydance parser fix**

Run:

```bash
git add crates/puffer-core/runtime/media/relaydance_video.rs
git commit -m "fix(media): parse relaydance video task responses"
```

---

## Task 4: BytePlus Verification Gate And Conditional Adapter

**Files if BytePlus probe passes:**
- Create: `crates/puffer-core/runtime/media/byteplus_video.rs`
- Modify: `crates/puffer-core/runtime/media/mod.rs`
- Modify: `crates/puffer-core/media_runtime.rs`
- Modify: `crates/puffer-core/runtime/media/resolver.rs`
- Modify: `crates/puffer-provider-registry/src/model.rs`
- Modify: `crates/puffer-provider-registry/src/model_tests.rs`
- Modify: `resources/providers/byteplus.yaml`

- [ ] **Step 1: Check whether BytePlus fixtures exist**

Run:

```bash
test -s crates/puffer-core/runtime/media/fixtures/byteplus_submit_task.json
test -s crates/puffer-core/runtime/media/fixtures/byteplus_poll_task.json
```

Expected: both commands return exit code `0`.

If either file is missing, skip the rest of Task 4 and create no BytePlus code
or YAML declarations.

- [ ] **Step 2: Add failing provider-registry test for BytePlus adapter**

In `crates/puffer-provider-registry/src/model_tests.rs`, add:

```rust
#[test]
fn media_execution_kind_parses_byteplus_video() {
    let kind: MediaExecutionKind = serde_yaml::from_str("byteplus_video").expect("parse");
    assert_eq!(kind, MediaExecutionKind::BytePlusVideo);
}
```

Run:

```bash
cargo test -p puffer-provider-registry media_execution_kind_parses_byteplus_video
```

Expected: fails because `BytePlusVideo` does not exist.

- [ ] **Step 3: Add `BytePlusVideo` execution kind**

In `crates/puffer-provider-registry/src/model.rs`, add:

```rust
BytePlusVideo,
```

to `MediaExecutionKind`.

- [ ] **Step 4: Wire BytePlus adapter availability**

In `crates/puffer-core/runtime/media/resolver.rs`, add:

```rust
(MediaKind::Video, MediaExecutionKind::BytePlusVideo)
```

to `execution_adapter_is_available_for_kind`, and add:

```rust
MediaExecutionKind::BytePlusVideo => "byteplus_video",
```

to `adapter_id`.

- [ ] **Step 5: Create BytePlus adapter tests from fixtures**

Create `crates/puffer-core/runtime/media/byteplus_video.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn submit_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("fixtures/byteplus_submit_task.json")).expect("fixture")
    }

    fn poll_fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("fixtures/byteplus_poll_task.json")).expect("fixture")
    }

    #[test]
    fn parses_byteplus_submit_fixture() {
        let task = BytePlusVideoTask::from_submit_value(submit_fixture()).expect("task");
        assert!(!task.id.trim().is_empty());
    }

    #[test]
    fn parses_byteplus_poll_fixture() {
        let task = BytePlusVideoTask::from_poll_value(poll_fixture()).expect("task");
        assert!(!task.status.trim().is_empty());
    }

    #[test]
    fn byteplus_request_body_contains_model_and_prompt() {
        let request = BytePlusVideoRequest {
            model: "dreamina-seedance-2-0-260128".to_string(),
            prompt: "a cat".to_string(),
            params: vec![],
        };
        let body = request.request_body();
        assert_eq!(body["model"], json!("dreamina-seedance-2-0-260128"));
        assert_eq!(body["content"][0]["type"], json!("text"));
        assert_eq!(body["content"][0]["text"], json!("a cat"));
    }
}
```

Run:

```bash
cargo test -p puffer-core byteplus_video -- --nocapture
```

Expected: fails because adapter types do not exist.

- [ ] **Step 6: Implement BytePlus adapter only against verified fixture fields**

Implement:

```rust
use super::capabilities::MediaCapabilityParameter;
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(crate) struct BytePlusVideoRequest {
    pub(crate) model: String,
    pub(crate) prompt: String,
    pub(crate) params: Vec<(String, String)>,
}

impl BytePlusVideoRequest {
    /// Builds a BytePlus ModelArk text-to-video task request.
    pub(crate) fn request_body(&self) -> Value {
        let mut body = Map::new();
        body.insert("model".to_string(), json!(self.model.trim()));
        body.insert(
            "content".to_string(),
            json!([
                {
                    "type": "text",
                    "text": self.prompt.trim()
                }
            ]),
        );
        for (field, value) in &self.params {
            body.insert(field.trim().to_string(), json!(value.trim()));
        }
        Value::Object(body)
    }

    fn validate(&self) -> Result<()> {
        if self.model.trim().is_empty() {
            bail!("video model is required");
        }
        if self.prompt.trim().is_empty() {
            bail!("video prompt is required");
        }
        Ok(())
    }
}

/// Maps a validated selection into a BytePlus text-to-video request.
pub(crate) fn byteplus_video_request_from_parameters(
    model_id: String,
    prompt: String,
    capability_parameters: &[MediaCapabilityParameter],
    selected: &BTreeMap<String, String>,
) -> Result<BytePlusVideoRequest> {
    let mut params = Vec::new();
    for parameter in capability_parameters {
        let Some(field) = parameter.request_field.clone() else {
            continue;
        };
        let value = selected
            .get(&parameter.name)
            .cloned()
            .unwrap_or_else(|| parameter.default.clone());
        params.push((field, value));
    }
    let request = BytePlusVideoRequest {
        model: model_id,
        prompt,
        params,
    };
    request.validate()?;
    Ok(request)
}

pub(crate) struct BytePlusVideoTask {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) video_url: Option<String>,
    pub(crate) error: Option<String>,
}

impl BytePlusVideoTask {
    /// Parses the BytePlus task creation response.
    pub(crate) fn from_submit_value(value: Value) -> Result<Self> {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("submit video task response missing task id")?
            .to_string();
        Ok(Self {
            id,
            status: "queued".to_string(),
            video_url: None,
            error: byteplus_error_message(&value),
        })
    }

    /// Parses the BytePlus task retrieval response.
    pub(crate) fn from_poll_value(value: Value) -> Result<Self> {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("poll video task response missing task id")?
            .to_string();
        let status = value
            .get("status")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("poll video task response missing status")?
            .to_string();
        Ok(Self {
            id,
            status,
            video_url: byteplus_video_url(&value),
            error: byteplus_error_message(&value),
        })
    }
}

fn byteplus_video_url(value: &Value) -> Option<String> {
    value
        .get("content")
        .and_then(|content| content.get("video_url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn byteplus_error_message(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
```

If the captured BytePlus fixtures do not match this documented shape, stop
Task 4, leave BytePlus audit-only, and update the spec before writing adapter
code. Do not copy Relaydance fallback field names into BytePlus.

- [ ] **Step 7: Wire BytePlus runtime match arm**

In `crates/puffer-core/runtime/media/mod.rs`, add:

```rust
pub(crate) mod byteplus_video;
```

In `crates/puffer-core/media_runtime.rs`, add a `"byteplus_video"` arm with the
same control flow as the Relaydance arm:

```rust
"byteplus_video" => {
    let request = byteplus_video_request_from_parameters(
        selection.model_id.clone(),
        prompt.to_string(),
        &selection.capability.parameters,
        &selected_parameters,
    )?;
    let adapter = BytePlusVideoAdapter::new(
        api_key,
        provider_execution_url(provider, &selection.capability.execution)?,
        provider.id.clone(),
    )?;
    let job = adapter.submit(&service, request, selected_parameters, now_ms())?;
    let terminal = adapter.poll_until_terminal(
        &service,
        job,
        BytePlusVideoPollingConfig::default(),
        std::thread::sleep,
        now_ms,
    )?;
    Ok(terminal)
}
```

Use the Relaydance adapter as the lifecycle template, but replace parser calls
with `BytePlusVideoTask::from_submit_value` and
`BytePlusVideoTask::from_poll_value`. Keep helper extraction out of this step
unless both adapters have identical compiled code after BytePlus passes tests.

- [ ] **Step 8: Add verified BytePlus YAML model declarations**

In `resources/providers/byteplus.yaml`, add `media.video` only for verified
text-to-video model ids. The first declaration must use:

```yaml
media:
  video:
    discovery:
      adapter: static
    execution:
      adapter: byteplus_video
      path: /contents/generations/tasks
    models:
      - id: dreamina-seedance-2-0-260128
        display_name: Dreamina Seedance 2.0
        operations:
          - generate
```

Do not add duration, ratio, resolution, audio, image, first-frame, or reference
parameters in this pass unless the BytePlus fixtures prove scalar request
fields that work for text-to-video. Do not declare image-to-video model ids.

- [ ] **Step 9: Run BytePlus tests**

Run:

```bash
cargo test -p puffer-provider-registry media_execution_kind_parses_byteplus_video
cargo test -p puffer-core byteplus_video -- --nocapture
cargo test -p puffer-core media::resolver
```

Expected: all pass.

- [ ] **Step 10: Commit BytePlus adapter if implemented**

Run:

```bash
git add crates/puffer-provider-registry/src/model.rs crates/puffer-provider-registry/src/model_tests.rs
git add crates/puffer-core/runtime/media/mod.rs crates/puffer-core/runtime/media/resolver.rs crates/puffer-core/media_runtime.rs
git add crates/puffer-core/runtime/media/byteplus_video.rs resources/providers/byteplus.yaml
git commit -m "feat(media): add verified byteplus video adapter"
```

---

## Task 5: Component Update Specs

**Files:**
- Create: `specs/puffer-core/257.md`
- Create: `specs/puffer-provider-registry/08.md`
- Create: `specs/puffer-resources/94.md`

- [ ] **Step 1: Write puffer-core update spec**

Create `specs/puffer-core/257.md`:

```markdown
# Video Provider Adapter Stability

Date: 2026-06-08

## Summary

The video runtime now uses a provider-specific `relaydance_video` adapter
instead of the misleading `openai_video` name. Relaydance response parsing is
fixture-backed, phase-specific, and marks local jobs failed when polling returns
an unexpected deterministic shape.

## Runtime Contract

- `VideoGeneration` still routes through exact media generation.
- Relaydance text-to-video remains one remote task per tool call.
- Polling is bounded.
- Successful terminal jobs persist one `video/mp4` artifact.
- Parser failures after submit save a failed job with a redacted diagnostic.

## Compatibility

There is no compatibility alias for `openai_video`. Bundled provider resources
and saved settings must use `relaydance_video`.
```

- [ ] **Step 2: Write provider-registry update spec**

Create `specs/puffer-provider-registry/08.md`:

```markdown
# Relaydance Video Execution Kind

Date: 2026-06-08

## Summary

`MediaExecutionKind` now declares `relaydance_video` for the verified Relaydance
video protocol. The former `openai_video` execution id is intentionally not
accepted.

## Contracts

- Provider YAML uses `adapter: relaydance_video`.
- Unknown or removed adapter ids fail provider parsing.
- Future shared protocol names require separate verification across at least two
  providers.
```

- [ ] **Step 3: Write resources update spec**

Create `specs/puffer-resources/94.md`:

```markdown
# Video Provider Resource Declarations

Date: 2026-06-08

## Summary

Relaydance video resources use the verified `relaydance_video` adapter. BytePlus
video resources are declared only when live probes produce stable text-to-video
fixtures.

## Provider Rules

- Model names discovered from provider model APIs do not create video
  capabilities automatically.
- Provider resources may declare video only when Puffer has an implemented,
  tested execution adapter.
- Image-to-video models remain undeclared until image input plumbing exists.
```

- [ ] **Step 4: Commit component specs**

Run:

```bash
git add specs/puffer-core/257.md specs/puffer-provider-registry/08.md specs/puffer-resources/94.md
git commit -m "docs(media): record video adapter stability changes"
```

---

## Task 6: Final Verification

**Files:**
- No new files.

- [ ] **Step 1: Search for stale adapter id**

Run:

```bash
rg -n "openai_video|OpenAiVideo|openai-video" crates resources specs docs --glob '!docs/superpowers/plans/2026-06-08-video-provider-adapter-stability.md'
```

Expected: no runtime/resource references. Historical docs may mention the old
id only when describing the removed behavior.

- [ ] **Step 2: Run focused test suite**

Run:

```bash
cargo test -p puffer-provider-registry media_execution_kind_
cargo test -p puffer-core relaydance_video
cargo test -p puffer-core video_generation
cargo test -p puffer-core media::resolver
```

Expected: all pass.

- [ ] **Step 3: Run broader media tests**

Run:

```bash
cargo test -p puffer-core media
cargo test -p puffer-resources provider
```

Expected: all pass.

- [ ] **Step 4: Review diff for unrelated files**

Run:

```bash
git status --short
git diff --stat
```

Expected: only files named in this plan are changed. Existing unrelated desktop
changes may still appear as unstaged; do not stage or revert them.

- [ ] **Step 5: Commit final verification fixes if needed**

If Step 1 or tests required small corrections, first list the changed files:

```bash
git diff --name-only
```

Stage only the exact corrected files from this plan. For example:

```bash
git add crates/puffer-core/runtime/media/relaydance_video.rs
git add crates/puffer-core/media_runtime.rs
git add crates/puffer-provider-registry/src/model.rs
git add resources/providers/relaydance.yaml
git commit -m "test(media): verify video adapter stability"
```

If no corrections were needed, do not create an empty commit.

---

## Execution Notes

- Do not print API keys or bearer tokens.
- Do not stage existing desktop UI changes.
- Do not add a generic JSON response mapping system.
- Do not add video recovery queues or background schedulers.
- Do not declare BytePlus video capability without fixtures.
- Do not add image-to-video model declarations in this pass.
