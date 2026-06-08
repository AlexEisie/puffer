# Video Generation (Relaydance / OpenAI-video) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make video generation appear and work in the desktop "Video generation settings" modal by adding a generic `openai_video` execution adapter and a new `relaydance.yaml` gateway provider declaring `media.video` (text-to-video only).

**Architecture:** Mirror the image path. Add `relaydance` as a normal provider resource (auto-discovered from `resources/providers/`, like `vercel-ai-gateway.yaml`). Add one generic `openai_video` adapter speaking the OpenAI-compatible async video shape (`POST /v1/video/generations` → `GET /v1/video/generations/{id}`), reusing `http_support` helpers (`provider_execution_url`, `bearer_token`, `download_image_url`, secret redaction) and the `replicate_video` async job lifecycle (queued → poll → terminal `MediaJob`). Every video model is a pure-YAML entry; new model = no code. Scope is text-to-video + scalar params only — image inputs (i2v / first-last frame) are a separate v2 subsystem.

**Tech Stack:** Rust, `reqwest::blocking`, `serde_json`, `anyhow`, existing `MediaJob`/`MediaGenerationService` runtime.

**Spec:** `docs/superpowers/specs/2026-06-08-seedance-video-generation-design.md`

---

## File Structure

- `crates/puffer-provider-registry/src/model.rs` — add `OpenAiVideo` enum variant (modify).
- `crates/puffer-core/runtime/media/resolver.rs` — allow `(Video, OpenAiVideo)`, `adapter_id` mapping, new `resolve_video_execution_descriptor` (modify).
- `crates/puffer-core/runtime/media/openai_video.rs` — new generic adapter module (create).
- `crates/puffer-core/runtime/media/mod.rs` — register module (modify).
- `crates/puffer-core/media_runtime.rs` — add `openai_video` match arm (modify).
- `resources/providers/relaydance.yaml` — new gateway provider with `media.video` (create).

---

## Task 0: Verify Relaydance OpenAI-video wire shape (gate — do first)

The model id, parameter placement, and poll envelope below are research-backed defaults (confirmed: live routes exist; `dto/openai_video.go` defines the status enum + `metadata` map). Confirm the remaining specifics against a live Relaydance key before the YAML is final. If reality differs, update constants in Task 6 (YAML) and Task 4 (parsing) — the code structure does not change.

- [ ] **Step 1: Confirm and record**

With a real Relaydance API key, confirm:
- A Seedance video model id from `GET https://relaydance.com/v1/models` (default assumed: `doubao-seedance-2-0-720p`).
- `POST /v1/video/generations` body field placement: that `duration` is top-level `seconds`, and `resolution`/`ratio` are under `metadata` (i.e. `metadata.resolution`, `metadata.ratio`), plus their allowed value sets.
- `POST` returns a JSON object with an `id` string and `status` (one of `queued|in_progress|completed|failed`).
- `GET /v1/video/generations/{id}` returns `status` and, on `completed`, the video URL at `metadata.url`.

No code change. Record any deltas as notes on Tasks 4 and 6.

---

## Task 1: Add `OpenAiVideo` execution kind

**Files:**
- Modify: `crates/puffer-provider-registry/src/model.rs` (`MediaExecutionKind`, ~line 418)
- Test: `crates/puffer-provider-registry/src/model_tests.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/puffer-provider-registry/src/model_tests.rs`:

```rust
#[test]
fn media_execution_kind_parses_openai_video() {
    let kind: MediaExecutionKind = serde_yaml::from_str("openai_video").expect("parse");
    assert_eq!(kind, MediaExecutionKind::OpenAiVideo);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p puffer-provider-registry media_execution_kind_parses_openai_video`
Expected: FAIL — unknown variant `openai_video` / `OpenAiVideo` is not a variant.

- [ ] **Step 3: Add the variant**

In `crates/puffer-provider-registry/src/model.rs`, add the variant with an explicit serde rename (the enum is `rename_all = "snake_case"`, which would otherwise produce `open_ai_video`):

```rust
pub enum MediaExecutionKind {
    ImagesJson,
    ChatImageOutput,
    MinimaxImage,
    ReplicateVideo,
    #[serde(rename = "openai_video")]
    OpenAiVideo,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p puffer-provider-registry media_execution_kind_parses_openai_video`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/puffer-provider-registry/src/model.rs crates/puffer-provider-registry/src/model_tests.rs
git commit -m "feat(media): add OpenAiVideo execution kind"
```

---

## Task 2: Wire resolver (availability, adapter_id, video execution descriptor)

**Files:**
- Modify: `crates/puffer-core/runtime/media/resolver.rs`
  - `execution_adapter_is_available_for_kind` (~line 301)
  - `adapter_id` (the fn mapping `MediaExecutionKind` → `&str`)
  - add `resolve_video_execution_descriptor` (mirror `resolve_image_execution_descriptor`)

- [ ] **Step 1: Write the failing test**

Add to the resolver tests module (bottom of `resolver.rs`, near existing `MediaKind::Video` tests):

```rust
#[test]
fn openai_video_execution_adapter_is_available() {
    assert!(execution_adapter_is_available_for_kind(
        MediaKind::Video,
        MediaExecutionKind::OpenAiVideo
    ));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p puffer-core openai_video_execution_adapter_is_available`
Expected: FAIL — returns `false`.

- [ ] **Step 3: Allow the pairing + adapter_id mapping**

In `execution_adapter_is_available_for_kind`:

```rust
fn execution_adapter_is_available_for_kind(kind: MediaKind, adapter: MediaExecutionKind) -> bool {
    matches!(
        (kind, adapter),
        (MediaKind::Image, MediaExecutionKind::ImagesJson)
            | (MediaKind::Image, MediaExecutionKind::ChatImageOutput)
            | (MediaKind::Image, MediaExecutionKind::MinimaxImage)
            | (MediaKind::Video, MediaExecutionKind::ReplicateVideo)
            | (MediaKind::Video, MediaExecutionKind::OpenAiVideo)
    )
}
```

In the `adapter_id` fn (returns the wire string per `MediaExecutionKind`), add:

```rust
        MediaExecutionKind::OpenAiVideo => "openai_video",
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p puffer-core openai_video_execution_adapter_is_available`
Expected: PASS

- [ ] **Step 5: Add `resolve_video_execution_descriptor`**

Read `resolve_image_execution_descriptor` fully first, then add this sibling next to it (only delta: `media.image` → `media.video`, label `image` → `video`). If the image fn has extra handling, copy ITS structure and apply only the `.image`→`.video` delta.

```rust
/// Resolves the provider and execution descriptor for a validated exact video selection.
pub(crate) fn resolve_video_execution_descriptor<'a>(
    registry: &'a ProviderRegistry,
    provider_id: &str,
    model_id: &str,
    adapter: &str,
) -> Result<(&'a ProviderDescriptor, MediaExecutionDescriptor)> {
    let unavailable =
        || format!("selected video model unavailable: {provider_id}/{model_id} via {adapter}");
    let provider = registry.provider(provider_id).with_context(unavailable)?;
    let video = provider
        .media
        .as_ref()
        .and_then(|media| media.video.as_ref())
        .with_context(unavailable)?;
    let model = video
        .models
        .iter()
        .find(|model| model.id == model_id)
        .with_context(unavailable)?;
    let execution = model
        .execution
        .clone()
        .or_else(|| video.execution.clone())
        .with_context(unavailable)?;
    Ok((provider, execution))
}
```

- [ ] **Step 6: Run build to verify it compiles**

Run: `cargo build -p puffer-core`
Expected: success (an unused-function warning for `resolve_video_execution_descriptor` is acceptable until Task 7 uses it).

- [ ] **Step 7: Commit**

```bash
git add crates/puffer-core/runtime/media/resolver.rs
git commit -m "feat(media): allow openai_video execution and add video execution descriptor resolver"
```

---

## Task 3: Request body + param mapping (top-level vs metadata)

**Files:**
- Create: `crates/puffer-core/runtime/media/openai_video.rs`
- Modify: `crates/puffer-core/runtime/media/mod.rs` (register module so tests compile)

- [ ] **Step 1: Register the module**

In `crates/puffer-core/runtime/media/mod.rs`, next to `pub(crate) mod replicate_video;`, add:

```rust
pub(crate) mod openai_video;
```

- [ ] **Step 2: Write the failing tests**

Create `crates/puffer-core/runtime/media/openai_video.rs`:

```rust
use super::capabilities::MediaCapabilityParameter;
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// One OpenAI-compatible video generation request (`POST /v1/video/generations`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAiVideoRequest {
    pub(crate) model: String,
    pub(crate) prompt: String,
    /// Ordered (request_field, value) pairs. A `request_field` of `metadata.<k>`
    /// is nested under the body's `metadata` object; otherwise it is top-level.
    pub(crate) params: Vec<(String, String)>,
}

const METADATA_PREFIX: &str = "metadata.";

impl OpenAiVideoRequest {
    fn validate(&self) -> Result<()> {
        if self.model.trim().is_empty() {
            bail!("video model is required");
        }
        if self.prompt.trim().is_empty() {
            bail!("video prompt is required");
        }
        Ok(())
    }

    /// Builds the `POST /v1/video/generations` request body.
    pub(crate) fn request_body(&self) -> Value {
        let mut body = Map::new();
        body.insert("model".to_string(), json!(self.model.trim()));
        body.insert("prompt".to_string(), json!(self.prompt.trim()));
        body.insert("n".to_string(), json!(1));

        let mut metadata = Map::new();
        for (field, value) in &self.params {
            let field = field.trim();
            if let Some(key) = field.strip_prefix(METADATA_PREFIX) {
                metadata.insert(key.to_string(), json!(value.trim()));
            } else {
                body.insert(field.to_string(), json!(value.trim()));
            }
        }
        if !metadata.is_empty() {
            body.insert("metadata".to_string(), Value::Object(metadata));
        }
        Value::Object(body)
    }
}

/// Maps a validated selection's parameters into an OpenAI-video request.
///
/// Emits params in capability order using each parameter's `request_field`
/// (only parameters that declare one). The selected value (already defaulted
/// by the caller) is used, falling back to the parameter default.
pub(crate) fn openai_video_request_from_parameters(
    model_id: String,
    prompt: String,
    capability_parameters: &[MediaCapabilityParameter],
    selected: &BTreeMap<String, String>,
) -> Result<OpenAiVideoRequest> {
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
    let request = OpenAiVideoRequest {
        model: model_id,
        prompt,
        params,
    };
    request.validate()?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameter(name: &str, request_field: &str, default: &str) -> MediaCapabilityParameter {
        MediaCapabilityParameter {
            name: name.to_string(),
            label: name.to_string(),
            values: vec![default.to_string()],
            default: default.to_string(),
            request_field: Some(request_field.to_string()),
        }
    }

    #[test]
    fn splits_top_level_and_metadata_params() {
        let params = vec![
            parameter("duration", "seconds", "5"),
            parameter("resolution", "metadata.resolution", "720p"),
            parameter("ratio", "metadata.ratio", "16:9"),
        ];
        let mut selected = BTreeMap::new();
        selected.insert("resolution".to_string(), "1080p".to_string());

        let request = openai_video_request_from_parameters(
            "m".to_string(),
            "a cat".to_string(),
            &params,
            &selected,
        )
        .expect("request");

        let body = request.request_body();
        assert_eq!(body["model"], json!("m"));
        assert_eq!(body["prompt"], json!("a cat"));
        assert_eq!(body["n"], json!(1));
        assert_eq!(body["seconds"], json!("5"));
        assert_eq!(body["metadata"]["resolution"], json!("1080p"));
        assert_eq!(body["metadata"]["ratio"], json!("16:9"));
    }

    #[test]
    fn omits_metadata_when_no_metadata_params() {
        let params = vec![parameter("duration", "seconds", "5")];
        let request = openai_video_request_from_parameters(
            "m".to_string(),
            "a cat".to_string(),
            &params,
            &BTreeMap::new(),
        )
        .expect("request");
        let body = request.request_body();
        assert!(body.get("metadata").is_none());
    }

    #[test]
    fn rejects_empty_prompt() {
        let error = openai_video_request_from_parameters(
            "m".to_string(),
            "   ".to_string(),
            &[],
            &BTreeMap::new(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("prompt is required"));
    }
}
```

> `MediaCapabilityParameter` lives in `runtime/media/capabilities.rs` (`pub(crate)`), with `request_field: Option<String>`. The import `use super::capabilities::MediaCapabilityParameter;` is correct from `openai_video.rs`.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p puffer-core openai_video::tests`
Expected: compiles and PASSES. If the `MediaCapabilityParameter` field set differs, align the test helper to the real struct.

- [ ] **Step 4: Commit**

```bash
git add crates/puffer-core/runtime/media/openai_video.rs crates/puffer-core/runtime/media/mod.rs
git commit -m "feat(media): add openai_video request body and param mapping"
```

---

## Task 4: Transport + task parsing + status normalization

**Files:**
- Modify: `crates/puffer-core/runtime/media/openai_video.rs`

- [ ] **Step 1: Add production code**

Append above `#[cfg(test)] mod tests`:

```rust
use super::MediaJobStatus;
use anyhow::Context;
use reqwest::blocking::Client;

/// Abstracts OpenAI-compatible video HTTP operations for production and tests.
pub(crate) trait OpenAiVideoTransport {
    /// Submits a video task and returns its JSON response.
    fn submit_task(&self, url: &str, api_token: &str, body: &Value) -> Result<Value>;

    /// Polls a video task URL and returns its JSON response.
    fn poll_task(&self, url: &str, api_token: &str) -> Result<Value>;

    /// Downloads the rendered video bytes from a validated remote URL.
    fn download_bytes(&self, url: &str) -> Result<Vec<u8>>;
}

/// Reqwest-backed transport used by the runtime adapter.
#[derive(Debug, Clone, Default)]
pub(crate) struct ReqwestOpenAiVideoTransport {
    client: Client,
}

impl OpenAiVideoTransport for ReqwestOpenAiVideoTransport {
    fn submit_task(&self, url: &str, api_token: &str, body: &Value) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .bearer_auth(api_token)
            .json(body)
            .send()
            .with_context(|| format!("submit video task {url}"))?;
        openai_video_json_response(response, "submit video task")
    }

    fn poll_task(&self, url: &str, api_token: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .bearer_auth(api_token)
            .send()
            .with_context(|| format!("poll video task {url}"))?;
        openai_video_json_response(response, "poll video task")
    }

    fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        // Reuse the shared https/loopback-enforcing downloader (image-path parity).
        super::http_support::download_image_url(&self.client, url, "video output")
    }
}

fn openai_video_json_response(response: reqwest::blocking::Response, label: &str) -> Result<Value> {
    let status = response.status();
    let text = response
        .text()
        .with_context(|| format!("read {label} response body"))?;
    if !status.is_success() {
        bail!("{label} failed with status {}: {text}", status.as_u16());
    }
    serde_json::from_str(&text).with_context(|| format!("parse {label} response JSON"))
}

/// Normalized view of an OpenAI-compatible video task response.
///
/// Envelope confirmed from New API `dto/openai_video.go`: `id`, `status`
/// (`queued|in_progress|completed|failed`), the video URL at `metadata.url`,
/// and `error.{message,code}`.
#[derive(Debug, Clone)]
pub(crate) struct OpenAiVideoTask {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) video_url: Option<String>,
    pub(crate) error: Option<String>,
}

impl OpenAiVideoTask {
    pub(crate) fn from_value(value: Value) -> Result<Self> {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .context("video task response missing `id`")?
            .to_string();
        let status = value
            .get("status")
            .and_then(Value::as_str)
            .context("video task response missing `status`")?
            .to_string();
        let video_url = value
            .get("metadata")
            .and_then(|metadata| metadata.get("url"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let error = value.get("error").and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        Ok(Self {
            id,
            status,
            video_url,
            error,
        })
    }

    /// Maps the OpenAI-video status string onto a media job status.
    pub(crate) fn media_status(&self) -> Result<MediaJobStatus> {
        match self.status.trim().to_ascii_lowercase().as_str() {
            "queued" | "pending" => Ok(MediaJobStatus::Queued),
            "in_progress" | "running" | "processing" => Ok(MediaJobStatus::Running),
            "completed" | "succeeded" | "success" => Ok(MediaJobStatus::Succeeded),
            "failed" | "error" | "expired" => Ok(MediaJobStatus::Failed),
            "cancelled" | "canceled" => Ok(MediaJobStatus::Canceled),
            other => bail!("unknown video task status `{other}`"),
        }
    }
}
```

- [ ] **Step 2: Add the tests**

Inside `mod tests`:

```rust
    #[test]
    fn parses_completed_task_with_metadata_url() {
        let value = json!({
            "id": "vid-1",
            "status": "completed",
            "metadata": { "url": "https://cdn.example.com/v.mp4" }
        });
        let task = OpenAiVideoTask::from_value(value).expect("task");
        assert_eq!(task.id, "vid-1");
        assert_eq!(task.media_status().unwrap(), MediaJobStatus::Succeeded);
        assert_eq!(task.video_url.as_deref(), Some("https://cdn.example.com/v.mp4"));
    }

    #[test]
    fn parses_failed_task_error_message() {
        let value = json!({
            "id": "vid-2",
            "status": "failed",
            "error": { "code": "x", "message": "content blocked" }
        });
        let task = OpenAiVideoTask::from_value(value).expect("task");
        assert_eq!(task.media_status().unwrap(), MediaJobStatus::Failed);
        assert_eq!(task.error.as_deref(), Some("content blocked"));
    }

    #[test]
    fn rejects_unknown_status() {
        let value = json!({ "id": "v", "status": "weird" });
        let task = OpenAiVideoTask::from_value(value).expect("task");
        assert!(task
            .media_status()
            .unwrap_err()
            .to_string()
            .contains("unknown video task status"));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p puffer-core openai_video::tests`
Expected: PASS (6 tests). If `MediaJobStatus` variant names differ, align the arms to the real enum (from `replicate_video.rs`: `Queued | Running | Succeeded | Failed | Canceled`).

- [ ] **Step 4: Commit**

```bash
git add crates/puffer-core/runtime/media/openai_video.rs
git commit -m "feat(media): add openai_video transport and task status parsing"
```

---

## Task 5: Adapter (submit + poll lifecycle)

**Files:**
- Modify: `crates/puffer-core/runtime/media/openai_video.rs`

This mirrors `replicate_video.rs`'s adapter lifecycle (submit → bounded poll → complete), simplified (no cancel). Read `replicate_video.rs` lines 177–415 before writing; reuse its `poll_until_terminal` loop shape.

- [ ] **Step 1: Add production code**

```rust
use super::{MediaArtifact, MediaGenerationService, MediaJob, MediaKind};
use std::time::Duration;
use uuid::Uuid;

const VIDEO_MIME_TYPE: &str = "video/mp4";
const OPENAI_VIDEO_ADAPTER: &str = "openai_video";

/// Bounded backoff while polling video tasks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OpenAiVideoPollingConfig {
    pub(crate) max_attempts: usize,
    pub(crate) delay: Duration,
}

impl Default for OpenAiVideoPollingConfig {
    fn default() -> Self {
        // Video renders take minutes; poll every 3s up to ~10 minutes.
        Self { max_attempts: 200, delay: Duration::from_millis(3_000) }
    }
}

/// Submits and polls OpenAI-compatible video tasks into media jobs.
pub(crate) struct OpenAiVideoAdapter<T = ReqwestOpenAiVideoTransport> {
    api_token: String,
    submit_url: String,
    provider_id: String,
    transport: T,
}

impl OpenAiVideoAdapter<ReqwestOpenAiVideoTransport> {
    /// Creates a production adapter. `submit_url` is the absolute task-creation
    /// URL built by the caller via `provider_execution_url`.
    pub(crate) fn new(
        api_token: impl Into<String>,
        submit_url: impl Into<String>,
        provider_id: impl Into<String>,
    ) -> Result<Self> {
        let api_token = api_token.into().trim().to_string();
        if api_token.is_empty() {
            bail!("video API token is required");
        }
        Ok(Self {
            api_token,
            submit_url: submit_url.into().trim_end_matches('/').to_string(),
            provider_id: provider_id.into(),
            transport: ReqwestOpenAiVideoTransport::default(),
        })
    }
}

impl<T> OpenAiVideoAdapter<T>
where
    T: OpenAiVideoTransport,
{
    #[cfg(test)]
    pub(crate) fn with_transport(
        api_token: impl Into<String>,
        submit_url: impl Into<String>,
        provider_id: impl Into<String>,
        transport: T,
    ) -> Self {
        Self {
            api_token: api_token.into().trim().to_string(),
            submit_url: submit_url.into().trim_end_matches('/').to_string(),
            provider_id: provider_id.into(),
            transport,
        }
    }

    /// Submits a task and persists the queued job (task id in `provider_job_id`).
    pub(crate) fn submit(
        &self,
        service: &MediaGenerationService,
        request: OpenAiVideoRequest,
        now_ms: u64,
    ) -> Result<MediaJob> {
        let response =
            self.transport
                .submit_task(&self.submit_url, &self.api_token, &request.request_body())?;
        let task = OpenAiVideoTask::from_value(response)?;
        let mut job = MediaJob::new(
            Uuid::new_v4().to_string(),
            MediaKind::Video,
            &self.provider_id,
            request.model.trim(),
            request.prompt.trim(),
            1,
            now_ms,
        );
        job.adapter = Some(OPENAI_VIDEO_ADAPTER.to_string());
        job.provider_job_id = Some(task.id.clone());
        self.apply_task(service, job, task, now_ms)
    }

    fn poll_url(&self, job: &MediaJob) -> Result<String> {
        let id = job
            .provider_job_id
            .as_ref()
            .context("video job is missing a task id")?;
        Ok(format!("{}/{id}", self.submit_url))
    }

    /// Polls a non-terminal job once and persists the resulting state.
    pub(crate) fn poll(
        &self,
        service: &MediaGenerationService,
        job: MediaJob,
        now_ms: u64,
    ) -> Result<MediaJob> {
        if job.status.is_terminal() {
            return Ok(job);
        }
        let url = self.poll_url(&job)?;
        let response = self.transport.poll_task(&url, &self.api_token)?;
        let task = OpenAiVideoTask::from_value(response)?;
        self.apply_task(service, job, task, now_ms)
    }

    /// Polls until the job reaches a terminal status.
    pub(crate) fn poll_until_terminal(
        &self,
        service: &MediaGenerationService,
        mut job: MediaJob,
        config: OpenAiVideoPollingConfig,
        mut sleep: impl FnMut(Duration),
        mut now_ms: impl FnMut() -> u64,
    ) -> Result<MediaJob> {
        for attempt in 0..config.max_attempts {
            job = self.poll(service, job, now_ms())?;
            if job.status.is_terminal() {
                return Ok(job);
            }
            if attempt + 1 < config.max_attempts {
                sleep(config.delay);
            }
        }
        bail!(
            "video job `{}` did not reach a terminal status after {} polls",
            job.id,
            config.max_attempts
        )
    }

    fn apply_task(
        &self,
        service: &MediaGenerationService,
        mut job: MediaJob,
        task: OpenAiVideoTask,
        now_ms: u64,
    ) -> Result<MediaJob> {
        match task.media_status()? {
            MediaJobStatus::Queued | MediaJobStatus::Running => {
                job.transition(task.media_status()?, now_ms)?;
                service.save_job(&job)?;
                Ok(job)
            }
            MediaJobStatus::Succeeded => self.complete_succeeded(service, job, &task, now_ms),
            MediaJobStatus::Failed => {
                job.error = task.error.clone().or(Some("video task failed".to_string()));
                job.transition(MediaJobStatus::Failed, now_ms)?;
                service.save_job(&job)?;
                Ok(job)
            }
            MediaJobStatus::Canceled => {
                job.transition(MediaJobStatus::Canceled, now_ms)?;
                service.save_job(&job)?;
                Ok(job)
            }
        }
    }

    fn complete_succeeded(
        &self,
        service: &MediaGenerationService,
        mut job: MediaJob,
        task: &OpenAiVideoTask,
        now_ms: u64,
    ) -> Result<MediaJob> {
        if !job.artifact_ids.is_empty() {
            job.transition(MediaJobStatus::Succeeded, now_ms)?;
            service.save_job(&job)?;
            return Ok(job);
        }
        let url = task
            .video_url
            .clone()
            .context("completed video task is missing `metadata.url`")?;
        let bytes = match self.transport.download_bytes(&url) {
            Ok(bytes) => bytes,
            Err(error) => {
                job.error = Some(format!("{error:#}"));
                job.transition(MediaJobStatus::Failed, now_ms)?;
                service.save_job(&job)?;
                return Err(error);
            }
        };
        let artifact_id = Uuid::new_v4().to_string();
        let path = service.write_artifact_bytes(
            &artifact_id,
            &format!("openai-video-{artifact_id}.mp4"),
            &bytes,
        )?;
        let artifact = MediaArtifact {
            id: artifact_id.clone(),
            job_id: job.id.clone(),
            kind: MediaKind::Video,
            path,
            mime_type: VIDEO_MIME_TYPE.to_string(),
            byte_count: bytes.len() as u64,
            metadata: json!({
                "provider": self.provider_id,
                "taskId": task.id,
                "remoteStatus": task.status,
            }),
            created_at_ms: now_ms,
        };
        service.save_artifact(&artifact)?;
        job.attach_artifact(artifact_id, now_ms);
        job.error = None;
        job.transition(MediaJobStatus::Succeeded, now_ms)?;
        service.save_job(&job)?;
        Ok(job)
    }
}
```

> Cross-check every `MediaJob`/`MediaArtifact`/`MediaGenerationService` method and field against `replicate_video.rs` (`MediaJob::new` arg order/types, `job.transition`, `job.attach_artifact`, `job.artifact_ids`, `job.provider_job_id`, `service.save_job`, `service.save_artifact`, `service.write_artifact_bytes`, `MediaArtifact { .. }`). Use the exact shapes it uses; if `MediaJob::new` takes the provider as a different type, match it.

- [ ] **Step 2: Add the happy-path test**

Inside `mod tests`:

```rust
    use super::super::MediaGenerationService;
    use std::cell::RefCell;

    struct ScriptedTransport {
        submit: Value,
        polls: RefCell<Vec<Value>>,
    }

    impl OpenAiVideoTransport for ScriptedTransport {
        fn submit_task(&self, _url: &str, _token: &str, _body: &Value) -> Result<Value> {
            Ok(self.submit.clone())
        }
        fn poll_task(&self, _url: &str, _token: &str) -> Result<Value> {
            Ok(self.polls.borrow_mut().remove(0))
        }
        fn download_bytes(&self, _url: &str) -> Result<Vec<u8>> {
            Ok(b"MP4BYTES".to_vec())
        }
    }

    #[test]
    fn submit_then_poll_downloads_video_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let service = MediaGenerationService::new(dir.path());
        let transport = ScriptedTransport {
            submit: json!({ "id": "vid-9", "status": "queued" }),
            polls: RefCell::new(vec![
                json!({ "id": "vid-9", "status": "in_progress" }),
                json!({ "id": "vid-9", "status": "completed", "metadata": { "url": "https://cdn.example.com/v.mp4" } }),
            ]),
        };
        let adapter = OpenAiVideoAdapter::with_transport(
            "token",
            "https://relaydance.com/v1/video/generations",
            "relaydance",
            transport,
        );

        let request = OpenAiVideoRequest { model: "m".into(), prompt: "a cat".into(), params: vec![] };
        let job = adapter.submit(&service, request, 1).expect("submit");
        let job = adapter
            .poll_until_terminal(
                &service,
                job,
                OpenAiVideoPollingConfig { max_attempts: 5, delay: Duration::from_millis(0) },
                |_| {},
                || 2,
            )
            .expect("poll");

        assert_eq!(job.status, MediaJobStatus::Succeeded);
        assert_eq!(job.artifact_ids.len(), 1);
    }
```

> Download is faked via the transport (no real HTTP server). `tempfile` is already a `puffer-core` dev-dependency. Confirm `MediaGenerationService::new` signature and `MediaJob`/`MediaArtifact` field names against `replicate_video.rs` before running.

- [ ] **Step 3: Run tests**

Run: `cargo test -p puffer-core openai_video::tests`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/puffer-core/runtime/media/openai_video.rs
git commit -m "feat(media): add openai_video adapter submit/poll lifecycle"
```

---

## Task 6: Add `relaydance.yaml` provider with `media.video`

**Files:**
- Create: `resources/providers/relaydance.yaml`

- [ ] **Step 1: Create the provider resource**

Use the model id confirmed in Task 0. Providers are auto-discovered from `resources/providers/`, so no code registers this file.

```yaml
id: relaydance
display_name: Relaydance
base_url: https://relaydance.com
default_api: openai-completions
auth_modes:
  - api_key
discovery:
  path: /v1/models
  response: open_ai_models
  api: openai-completions
  context_window: 128000
  max_output_tokens: 16384
  supports_reasoning: false
media:
  video:
    discovery:
      adapter: static
    execution:
      adapter: openai_video
      path: /v1/video/generations
    models:
      - id: doubao-seedance-2-0-720p
        display_name: Seedance 2.0 (720p)
        operations:
          - generate
        parameters:
          - name: duration
            label: Duration
            values: ["5", "10"]
            default: "5"
            request_field: seconds
          - name: resolution
            label: Resolution
            values: ["720p", "1080p"]
            default: "720p"
            request_field: metadata.resolution
          - name: ratio
            label: Aspect ratio
            values: ["16:9", "9:16", "1:1"]
            default: "16:9"
            request_field: metadata.ratio
```

- [ ] **Step 2: Verify the resource parses and loads**

Run: `cargo test -p puffer-provider-registry`
Expected: PASS (resource load/parse tests still green; the new `media.video` section deserializes with `adapter: openai_video`). If a test enumerates expected provider files/count, update it to include `relaydance`.

- [ ] **Step 3: Commit**

```bash
git add resources/providers/relaydance.yaml
git commit -m "feat(media): add Relaydance gateway provider with Seedance video capability"
```

---

## Task 7: Wire the `openai_video` arm in the media runtime

**Files:**
- Modify: `crates/puffer-core/media_runtime.rs` (`generate_exact_video_from_media_request`, ~line 675)

- [ ] **Step 1: Add the match arm**

In `generate_exact_video_from_media_request`, the `match request.adapter.as_str()` currently has `"replicate_video"` and a catch-all `bail!`. Add an `"openai_video"` arm before the catch-all. Reuse the bindings the surrounding code already computed (`capability` from `validate_media_generate_selection`, `parameters` from the selected-with-defaults map). Mirror the `replicate_video` arm's structure:

```rust
        "openai_video" => {
            let (provider, execution) = resolve_video_execution_descriptor(
                registry,
                &request.provider_id,
                &request.model_id,
                &request.adapter,
            )?;
            let api_key = bearer_token(provider, auth_store, CredentialAliasMode::Strict)?
                .context("Relaydance API key is required")?;
            let submit_url = provider_execution_url(provider, &execution, "video task")?;
            let service = MediaGenerationService::new(workspace_root);
            let adapter =
                OpenAiVideoAdapter::new(api_key, submit_url.to_string(), request.provider_id.clone())?;
            let job = adapter.submit(
                &service,
                openai_video_request_from_parameters(
                    request.model_id.clone(),
                    request.prompt.clone(),
                    &capability.parameters,
                    &parameters,
                )?,
                now_ms(),
            )?;
            let job = adapter.poll_until_terminal(
                &service,
                job,
                OpenAiVideoPollingConfig::default(),
                std::thread::sleep,
                now_ms,
            )?;
            let artifacts = load_media_job_artifacts(&service, &job)?;
            Ok(exact_media_generation_result(job, artifacts))
        }
```

Add imports at the top of `media_runtime.rs`:

```rust
use crate::runtime::media::http_support::provider_execution_url;
use crate::runtime::media::resolver::resolve_video_execution_descriptor;
use crate::runtime::media::openai_video::{
    openai_video_request_from_parameters, OpenAiVideoAdapter, OpenAiVideoPollingConfig,
};
```

> Match the existing import style. `bearer_token`, `CredentialAliasMode`, `MediaGenerationService`, `load_media_job_artifacts`, `exact_media_generation_result`, `now_ms`, `workspace_root` are already used by the `replicate_video` arm — reuse exactly those. If the replicate arm wraps provider errors with `provider_error_secrets`/`redact_secrets`, mirror that wrapping here. Confirm the `capability`/`parameters` binding names at the match site; if named `_capability`, use the real name.

- [ ] **Step 2: Build**

Run: `cargo build -p puffer-core`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/puffer-core/media_runtime.rs
git commit -m "feat(media): route openai_video adapter in media runtime"
```

---

## Task 8: Integration test — daemon discovers Relaydance video capability

**Files:**
- Modify: `crates/puffer-cli/src/daemon.rs` (tests module — reuse the existing `daemon_state_with_replicate_video_capability` / `write_replicate_video_resource_override` harness pattern)

- [ ] **Step 1: Write the failing test**

Mirror the replicate video test helpers, changing: provider id → `relaydance`, base_url → `https://relaydance.com`, `adapter: openai_video`, `path: /v1/video/generations`, a Seedance model id, and an auth key set for `relaydance`. Then:

```rust
    #[test]
    fn daemon_list_media_capabilities_returns_relaydance_video_capability() {
        let (_home_guard, _temp, state) = daemon_state_with_relaydance_video_capability();

        let response =
            handle_list_media_capabilities(&state, &json!({"kind": "video"})).expect("response");
        let capabilities = response["capabilities"].as_array().expect("capabilities");

        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0]["adapter"], "openai_video");
        assert_eq!(capabilities[0]["provider_id"], "relaydance");
    }
```

Add helpers `daemon_state_with_relaydance_video_capability` and `write_relaydance_video_resource_override` by copying the replicate equivalents and applying the deltas above (provider id, base_url, adapter, path, model id, and one parameter block matching Task 6's YAML).

- [ ] **Step 2: Run test to verify it fails then passes**

Run: `cargo test -p puffer-cli daemon_list_media_capabilities_returns_relaydance_video_capability`
Expected: FAIL first if the override/helper is incomplete; PASS once the override matches the new YAML schema and Tasks 1/2 are in place.

- [ ] **Step 3: Commit**

```bash
git add crates/puffer-cli/src/daemon.rs
git commit -m "test(media): daemon discovers Relaydance video capability"
```

---

## Task 9: Full verification

- [ ] **Step 1: Workspace build + tests**

Run: `cargo build --workspace`
Expected: success.

Run: `cargo test -p puffer-core -p puffer-provider-registry -p puffer-cli`
Expected: all PASS.

- [ ] **Step 2: Desktop UI smoke (manual)**

With a `relaydance` API key configured, open a session → Add content → "Video generation settings". Confirm the modal lists provider **Relaydance** / model **Seedance 2.0 (720p)** with Duration/Resolution/Aspect ratio selectors (no "No video capabilities available."). Save, then run `/video <prompt>` and confirm a task is submitted and an MP4 artifact attaches on completion.

- [ ] **Step 3: Commit any test-only fixups**

```bash
git add -A
git commit -m "test(media): finalize Relaydance video verification"
```

---

## Self-Review Notes

- **Spec coverage:** provider YAML (Task 6) ✓; generic `openai_video` adapter reusing `http_support` + replicate lifecycle (Tasks 3–5) ✓; four wiring points — enum (T1), availability + adapter_id + `resolve_video_execution_descriptor` (T2), `mod.rs` (T3), match arm (T7) ✓; dotted `request_field` top-level/metadata split (T3) ✓; OpenAI-video envelope parsing — `metadata.url`, statuses `queued|in_progress|completed|failed`, `error.message` (T4) ✓; error/redaction reuse via `download_image_url` and (if replicate does) `provider_error_secrets`/`redact_secrets` in the arm (T5/T7) ✓; testing (T3–T5, T8) ✓; verification gate (T0) ✓; count==1 via existing `validate_video_count` (unchanged) ✓.
- **Non-goals honored:** no generic multi-gateway engine; no `content[]`/`content_role`/image-input plumbing (v2); `replicate_video` untouched; image path untouched.
- **Type consistency:** `OpenAiVideoRequest`, `openai_video_request_from_parameters`, `OpenAiVideoTransport`, `ReqwestOpenAiVideoTransport`, `OpenAiVideoTask`, `OpenAiVideoAdapter`, `OpenAiVideoPollingConfig`, wire string `"openai_video"` used identically across Tasks 1/3/4/5/7. Provider id `relaydance` consistent across Tasks 6/7/8.
- **Verified gaps closed:** image-input plumbing absent → first/last frame deferred to v2 (spec "Out of scope"); providers auto-discovered from `resources/providers/` → no central list edit needed; OpenAI-video poll envelope grounded in New API `dto/openai_video.go`.
```