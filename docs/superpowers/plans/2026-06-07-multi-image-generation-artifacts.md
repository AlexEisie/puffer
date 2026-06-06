# Multi Image Generation Artifacts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support one image generation request producing multiple persisted image artifacts under one media job.

**Architecture:** Keep `artifactId` single-file scoped and use `jobId` as the grouping identity. Add an explicit typed `count` request field, return `artifacts[]`, and synthesize one generated-media attachment per artifact without adding a gallery, database, or artifact container.

**Tech Stack:** Rust (`puffer-core`, `puffer-cli`, Tauri backend), Svelte/TypeScript (`apps/puffer-desktop`), Cargo tests, npm frontend tests.

---

## Recheck Outcome

The reviewed design was tightened before planning:

- `artifactId` remains one generated media file.
- `jobId` is the only grouping identity.
- `count` is a typed request field, not a global media setting or string-valued provider parameter.
- `MediaJob` stores `requested_count`; produced count is derived from `artifact_ids.len()`.
- Fully failed generation saves a failed job sidecar and returns a tool/RPC error.
- Partial success returns a normal succeeded result with fewer artifacts.
- Adapter modules keep persistence local to the existing `MediaGenerationService`
  flow. Do not introduce a generic batch framework or artifact container.
- No parallel execution, gallery, object storage, database, retry UI, or generic batch system is included.

## File Structure

- Modify `crates/puffer-core/runtime/media/jobs.rs`
  - Add `requested_count`.
  - Add a derived `produced_count()` helper.

- Do not modify `crates/puffer-core/runtime/media/artifacts.rs`
  - Keep artifact identity unchanged.
  - No collection fields are added.

- Modify `crates/puffer-core/runtime/media/images_json.rs`
  - Accept `count`.
  - Serialize provider-native `n` as a JSON number.
  - Parse all returned `data[]` image items up to count.
  - Persist multiple artifacts under one job.

- Modify `crates/puffer-core/runtime/media/chat_image_output.rs`
  - Accept `count`.
  - Collect image outputs in stable order instead of returning the first image.
  - Persist multiple artifacts under one job.

- Modify `crates/puffer-core/runtime/media/minimax_image.rs`
  - Accept `count`.
  - Execute repeated single-image calls serially under one job.

- Modify `crates/puffer-core/media_runtime.rs`
  - Add `count` to `ExactImageGenerationRequest`.
  - Replace single-artifact `ExactImageGenerationResult` with `artifacts[]`.
  - Add public generated artifact result struct.
  - Preserve preview reads by `sessionId + artifactId`.

- Modify `crates/puffer-core/lib.rs`
  - Re-export the new public generated artifact result type.

- Modify `crates/puffer-core/runtime/claude_tools/workflow/image_generation.rs`
  - Add `count` to tool input.
  - Default missing count to `1`.
  - Emit `artifacts[]` in the tool output.

- Modify `resources/tools/image_generation.yaml`
  - Add `count` to the schema and describe the `1..4` bound.

- Modify `crates/puffer-cli/src/daemon.rs`
  - Add `count` to `GenerateMediaParams`.
  - Replace `GenerateMediaResult.artifactId/path` with `artifacts[]`.

- Modify `crates/puffer-cli/src/desktop_api_types.rs`
  - Add `jobId` and `index` to generated-media attachment source.

- Modify `crates/puffer-cli/src/desktop_api.rs`
  - Parse `artifacts[]` from `ImageGeneration` outputs.
  - Synthesize multiple generated image attachments from one tool output.

- Modify `apps/puffer-desktop/src-tauri/src/dtos.rs`
  - Add `jobId` and `index` to generated-media attachment source DTOs.

- Modify `apps/puffer-desktop/src-tauri/src/session_data.rs`
  - Parse `artifacts[]` and create one generated attachment per artifact.

- Modify `apps/puffer-desktop/src-tauri/src/backend.rs`
  - Add `count` and return `artifacts[]` for direct backend generated media.

- Modify `apps/puffer-desktop/src/lib/types.ts`
  - Add `count` to media generation input.
  - Replace generation result single artifact with `artifacts[]`.
  - Add `jobId` and `index` to generated-media source.

- Modify `apps/puffer-desktop/src/lib/api/desktop.ts`
  - Normalize new result and attachment source shapes.

- Modify `apps/puffer-desktop/src/App.svelte`
  - Live `/image` preview creates one assistant item with all generated attachments.

- Modify `apps/puffer-desktop/tests/support/fakeDaemon.ts`
  - Return `artifacts[]`.
  - Store generated previews keyed by artifact id.

- Modify `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
  - Cover one generated result with two generated attachments.

## Task 1: Media Job Count Metadata

**Files:**
- Modify: `crates/puffer-core/runtime/media/jobs.rs`
- Modify: `crates/puffer-core/runtime/media/mod.rs`
- Modify: `crates/puffer-core/runtime/media/images_json.rs`
- Modify: `crates/puffer-core/runtime/media/chat_image_output.rs`
- Modify: `crates/puffer-core/runtime/media/minimax_image.rs`
- Modify: `crates/puffer-core/runtime/media/replicate_video.rs`

- [ ] **Step 1: Write the failing job metadata test**

Add this test module to `crates/puffer-core/runtime/media/jobs.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_job_tracks_requested_and_produced_counts() {
        let mut job = MediaJob::new(
            "job-1",
            MediaKind::Image,
            "openai",
            "gpt-image-1",
            "draw two images",
            2,
            10,
        );

        assert_eq!(job.requested_count, 2);
        assert_eq!(job.produced_count(), 0);

        job.attach_artifact("artifact-1", 11);
        job.attach_artifact("artifact-2", 12);
        job.attach_artifact("artifact-2", 13);

        assert_eq!(job.artifact_ids, vec!["artifact-1", "artifact-2"]);
        assert_eq!(job.produced_count(), 2);
    }
}
```

- [ ] **Step 2: Run the failing job metadata test**

Run:

```bash
cargo test -p puffer-core media_job_tracks_requested_and_produced_counts -- --nocapture
```

Expected: FAIL because `MediaJob::new` does not accept `requested_count`, and `produced_count` does not exist.

- [ ] **Step 3: Add requested count to `MediaJob`**

Update `MediaJob` and constructor in `crates/puffer-core/runtime/media/jobs.rs`:

```rust
pub(crate) struct MediaJob {
    pub(crate) id: String,
    pub(crate) kind: MediaKind,
    pub(crate) provider_id: String,
    pub(crate) model_id: String,
    pub(crate) prompt: String,
    pub(crate) status: MediaJobStatus,
    pub(crate) provider_job_id: Option<String>,
    pub(crate) remote_status: Option<String>,
    pub(crate) remote_get_url: Option<String>,
    pub(crate) remote_cancel_url: Option<String>,
    pub(crate) artifact_ids: Vec<String>,
    pub(crate) requested_count: u8,
    pub(crate) error: Option<String>,
    pub(crate) created_at_ms: u64,
    pub(crate) updated_at_ms: u64,
}
```

```rust
pub(crate) fn new(
    id: impl Into<String>,
    kind: MediaKind,
    provider_id: impl Into<String>,
    model_id: impl Into<String>,
    prompt: impl Into<String>,
    requested_count: u8,
    now_ms: u64,
) -> Self {
    Self {
        id: id.into(),
        kind,
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        prompt: prompt.into(),
        status: MediaJobStatus::Queued,
        provider_job_id: None,
        remote_status: None,
        remote_get_url: None,
        remote_cancel_url: None,
        artifact_ids: Vec::new(),
        requested_count,
        error: None,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
    }
}
```

Add the derived helper:

```rust
/// Returns the number of unique artifacts attached to this job.
pub(crate) fn produced_count(&self) -> usize {
    self.artifact_ids.len()
}
```

- [ ] **Step 4: Update existing `MediaJob::new` call sites**

Pass `1` for existing non-multi call sites first:

```rust
let mut job = MediaJob::new(
    job_id.clone(),
    MediaKind::Image,
    request.provider_id.clone(),
    request.model_id.clone(),
    request.prompt.clone(),
    1,
    created_at_ms,
);
```

Use the same `1` for video call sites in `crates/puffer-core/runtime/media/replicate_video.rs`.

- [ ] **Step 5: Verify job metadata**

Run:

```bash
cargo test -p puffer-core media_job_tracks_requested_and_produced_counts -- --nocapture
cargo test -p puffer-core media_jobs_and_artifacts_roundtrip_through_json_sidecars -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit job metadata**

```bash
git add crates/puffer-core/runtime/media/jobs.rs crates/puffer-core/runtime/media/images_json.rs crates/puffer-core/runtime/media/chat_image_output.rs crates/puffer-core/runtime/media/minimax_image.rs crates/puffer-core/runtime/media/replicate_video.rs crates/puffer-core/runtime/media/mod.rs
git commit -m "feat(media): track requested image count on jobs"
```

## Task 2: Core Multi-Artifact Result Contract

**Files:**
- Modify: `crates/puffer-core/media_runtime.rs`
- Modify: `crates/puffer-core/lib.rs`
- Modify: `crates/puffer-core/runtime/media/images_json.rs`
- Modify: `crates/puffer-core/runtime/media/chat_image_output.rs`
- Modify: `crates/puffer-core/runtime/media/minimax_image.rs`

- [ ] **Step 1: Write failing core result tests**

Add this test to `crates/puffer-core/media_runtime.rs` inside the existing test module:

```rust
#[test]
fn exact_image_generation_rejects_invalid_count() {
    assert!(validate_image_count(1).is_ok());
    assert!(validate_image_count(4).is_ok());
    assert_eq!(
        validate_image_count(0).unwrap_err().to_string(),
        "image generation count must be between 1 and 4"
    );
    assert_eq!(
        validate_image_count(5).unwrap_err().to_string(),
        "image generation count must be between 1 and 4"
    );
}
```

Add this test near existing exact image tests:

```rust
#[test]
fn exact_generation_result_returns_artifacts_in_order() {
    let job = MediaJob {
        id: "job-1".to_string(),
        kind: MediaKind::Image,
        provider_id: "openai".to_string(),
        model_id: "gpt-image-1".to_string(),
        prompt: "draw".to_string(),
        status: MediaJobStatus::Succeeded,
        provider_job_id: None,
        remote_status: None,
        remote_get_url: None,
        remote_cancel_url: None,
        artifact_ids: vec!["artifact-1".to_string(), "artifact-2".to_string()],
        requested_count: 2,
        error: None,
        created_at_ms: 1,
        updated_at_ms: 2,
    };
    let artifacts = vec![
        MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: MediaKind::Image,
            path: PathBuf::from("/tmp/image-1.png"),
            mime_type: "image/png".to_string(),
            byte_count: 10,
            metadata: serde_json::json!({"index": 0}),
            created_at_ms: 1,
        },
        MediaArtifact {
            id: "artifact-2".to_string(),
            job_id: "job-1".to_string(),
            kind: MediaKind::Image,
            path: PathBuf::from("/tmp/image-2.png"),
            mime_type: "image/png".to_string(),
            byte_count: 11,
            metadata: serde_json::json!({"index": 1}),
            created_at_ms: 1,
        },
    ];

    let result = exact_generation_result(job, artifacts);

    assert_eq!(result.job_id, "job-1");
    assert_eq!(result.requested_count, 2);
    assert_eq!(result.artifacts.len(), 2);
    assert_eq!(result.artifacts[0].artifact_id, "artifact-1");
    assert_eq!(result.artifacts[0].index, 0);
    assert_eq!(result.artifacts[1].artifact_id, "artifact-2");
    assert_eq!(result.artifacts[1].index, 1);
}
```

- [ ] **Step 2: Run failing core result tests**

Run:

```bash
cargo test -p puffer-core exact_image_generation_rejects_invalid_count -- --nocapture
cargo test -p puffer-core exact_generation_result_returns_artifacts_in_order -- --nocapture
```

Expected: FAIL because `validate_image_count`, `requested_count`, and multi-artifact result fields do not exist.

- [ ] **Step 3: Add core request and result types**

Update `crates/puffer-core/media_runtime.rs`:

```rust
/// Carries an exact image generation request from UI or tool configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactImageGenerationRequest {
    pub provider_id: String,
    pub model_id: String,
    pub adapter: String,
    pub prompt: String,
    pub parameters: BTreeMap<String, String>,
    pub count: u8,
}
```

```rust
/// Carries one persisted generated image artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactGeneratedArtifact {
    pub artifact_id: String,
    pub index: usize,
    pub path: PathBuf,
    pub mime_type: String,
    pub byte_count: u64,
}
```

```rust
/// Carries the persisted job and artifacts produced by exact image generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactImageGenerationResult {
    pub job_id: String,
    pub requested_count: u8,
    pub artifacts: Vec<ExactGeneratedArtifact>,
    pub provider_id: String,
    pub model_id: String,
    pub status: String,
}
```

Add count validation:

```rust
fn validate_image_count(count: u8) -> Result<u8> {
    if (1..=4).contains(&count) {
        Ok(count)
    } else {
        bail!("image generation count must be between 1 and 4")
    }
}
```

Update `generate_exact_image_with_cache` to validate count before resolving
capabilities or creating a media service:

```rust
let count = validate_image_count(request.count)?;
request.count = count;
```

Update `exact_generation_result`:

```rust
fn exact_generation_result(
    job: MediaJob,
    artifacts: Vec<MediaArtifact>,
) -> ExactImageGenerationResult {
    let artifacts = artifacts
        .into_iter()
        .enumerate()
        .map(|(index, artifact)| ExactGeneratedArtifact {
            artifact_id: artifact.id,
            index,
            path: artifact.path,
            mime_type: artifact.mime_type,
            byte_count: artifact.byte_count,
        })
        .collect();
    ExactImageGenerationResult {
        job_id: job.id,
        requested_count: job.requested_count,
        artifacts,
        provider_id: job.provider_id,
        model_id: job.model_id,
        status: media_job_status_name(job.status).to_string(),
    }
}
```

- [ ] **Step 4: Update adapter result structs to vectors**

Change each image adapter result to carry `artifacts: Vec<MediaArtifact>`:

```rust
pub(crate) struct ImagesJsonGenerationResult {
    pub(crate) job: MediaJob,
    pub(crate) artifacts: Vec<MediaArtifact>,
}
```

Use the same shape for `ChatImageOutputGenerationResult` and `MinimaxImageGenerationResult`.

- [ ] **Step 5: Re-export the generated artifact result**

Update `crates/puffer-core/lib.rs` so callers can name the public artifact
type returned by `ExactImageGenerationResult`:

```rust
pub use media_runtime::{
    discover_exact_media_capabilities, generate_exact_image_with_cache,
    generated_media_attachment_metadata, list_exact_media_capabilities_with_cache,
    read_generated_media_preview_by_artifact, resolved_exact_image_parameters_with_cache,
    ExactGeneratedArtifact, ExactImageGenerationRequest, ExactImageGenerationResult,
    ExactMediaDiscoveryCache, GeneratedMediaAttachmentMetadata, GeneratedMediaPreviewResult,
    MediaCapabilityView, MEDIA_DISCOVERY_TTL_MS,
};
```

- [ ] **Step 6: Verify core result tests**

Run:

```bash
cargo test -p puffer-core exact_image_generation_rejects_invalid_count -- --nocapture
cargo test -p puffer-core exact_generation_result_returns_artifacts_in_order -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit core result contract**

```bash
git add crates/puffer-core/media_runtime.rs crates/puffer-core/lib.rs crates/puffer-core/runtime/media/images_json.rs crates/puffer-core/runtime/media/chat_image_output.rs crates/puffer-core/runtime/media/minimax_image.rs
git commit -m "feat(media): return image generation artifacts"
```

## Task 3: OpenAI Images JSON Multi-Output Adapter

**Files:**
- Modify: `crates/puffer-core/runtime/media/images_json.rs`

- [ ] **Step 1: Write the failing adapter test**

Add this test to `crates/puffer-core/runtime/media/images_json.rs`:

```rust
#[test]
fn images_json_persists_multiple_response_images_under_one_job() {
    let (base_url, server) = spawn_image_server_with_body(
        r#"{"data":[{"b64_json":"aW1hZ2UtMQ=="},{"b64_json":"aW1hZ2UtMg=="}]}"#,
    );
    let registry = registry_with_provider(base_url);
    let mut auth_store = AuthStore::default();
    auth_store.set_api_key("exact-provider", "sk-test");
    let service_dir = tempfile::tempdir().unwrap();
    let service = MediaGenerationService::new(service_dir.path());
    let request = ImagesJsonGenerationRequest {
        provider_id: "exact-provider".to_string(),
        model_id: "exact-image-model".to_string(),
        adapter: "images_json".to_string(),
        prompt: "draw two images".to_string(),
        parameters: BTreeMap::from([
            ("size".to_string(), "1024x1024".to_string()),
            ("quality".to_string(), "auto".to_string()),
            ("output_format".to_string(), "png".to_string()),
        ]),
        count: 2,
    };

    let result = ImagesJsonAdapter::new()
        .unwrap()
        .execute(&registry, &auth_store, &service, request)
        .unwrap();

    let request_text = server.join().unwrap();
    assert!(request_text.contains("\"n\":2"));
    assert_eq!(result.job.requested_count, 2);
    assert_eq!(result.job.artifact_ids.len(), 2);
    assert_eq!(result.artifacts.len(), 2);
    assert_ne!(result.artifacts[0].id, result.artifacts[1].id);
    assert_eq!(std::fs::read(&result.artifacts[0].path).unwrap(), b"image-1");
    assert_eq!(std::fs::read(&result.artifacts[1].path).unwrap(), b"image-2");
    assert_eq!(result.artifacts[0].metadata["index"], 0);
    assert_eq!(result.artifacts[1].metadata["index"], 1);
}
```

Replace the current single-response HTTP helper with this request-capturing
helper:

```rust
fn spawn_image_server_with_body(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 4096];
        let n = stream.read(&mut buffer).unwrap();
        let request = String::from_utf8_lossy(&buffer[..n]).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
        request
    });
    (format!("http://{addr}"), handle)
}
```

- [ ] **Step 2: Run the failing adapter test**

Run:

```bash
cargo test -p puffer-core images_json_persists_multiple_response_images_under_one_job -- --nocapture
```

Expected: FAIL because the adapter only returns one artifact and does not write typed `n`.

- [ ] **Step 3: Add `count` to the request and typed body**

Remove `n` from the string-parameter allowlist:

```rust
const IMAGES_JSON_ALLOWED_REQUEST_FIELDS: &[&str] = &[
    "model",
    "prompt",
    "size",
    "quality",
    "output_format",
    "response_format",
    "aspect_ratio",
    "resolution",
];
```

Update `ImagesJsonGenerationRequest`:

```rust
pub(crate) struct ImagesJsonGenerationRequest {
    pub(crate) provider_id: String,
    pub(crate) model_id: String,
    pub(crate) adapter: String,
    pub(crate) prompt: String,
    pub(crate) parameters: BTreeMap<String, String>,
    pub(crate) count: u8,
}
```

Update `ImagesJsonRequest`:

```rust
struct ImagesJsonRequest {
    model: String,
    prompt: String,
    parameters: BTreeMap<String, String>,
    count: u8,
}
```

Update `ImagesJsonRequest::new` to accept `count: u8` and set the field. Rename
`request_image` to `request_images`, return `Result<Vec<ImageOutput>>`, and
build the body with:

```rust
let body = ImagesJsonRequest::new(
    &request.model_id,
    &request.prompt,
    parameters,
    request.count,
)
.to_body();
```

```rust
fn to_body(&self) -> Value {
    let mut body = Map::new();
    body.insert("model".to_string(), json!(self.model));
    body.insert("prompt".to_string(), json!(self.prompt));
    for (name, value) in &self.parameters {
        if name == "n" {
            continue;
        }
        body.insert(name.clone(), json!(value));
    }
    if self.count > 1 {
        body.insert("n".to_string(), json!(self.count));
    }
    Value::Object(body)
}
```

- [ ] **Step 4: Parse all image outputs**

Replace the single-output parser with:

```rust
fn image_outputs_from_response(client: &Client, value: &Value, count: u8) -> Result<Vec<ImageOutput>> {
    let Some(items) = value.get("data").and_then(Value::as_array) else {
        bail!("image generation response did not contain an image");
    };
    let mut outputs = Vec::new();
    for item in items.iter().take(count as usize) {
        outputs.push(image_output_from_item(client, item)?);
    }
    if outputs.is_empty() {
        bail!("image generation response did not contain an image");
    }
    Ok(outputs)
}
```

Move the existing `b64_json` and `url` logic into:

```rust
fn image_output_from_item(client: &Client, item: &Value) -> Result<ImageOutput> {
    let revised_prompt = item
        .get("revised_prompt")
        .or_else(|| item.get("revisedPrompt"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    if let Some(encoded) = item.get("b64_json").and_then(Value::as_str) {
        let bytes = BASE64_STANDARD
            .decode(encoded.trim())
            .context("decode image b64_json")?;
        return Ok(ImageOutput {
            bytes,
            revised_prompt,
            remote_source_url: None,
        });
    }
    if let Some(url) = item.get("url").and_then(Value::as_str) {
        let bytes = download_image_url(client, url, "image response")?;
        return Ok(ImageOutput {
            bytes,
            revised_prompt,
            remote_source_url: Some(url.to_string()),
        });
    }
    bail!("image generation response did not contain an image")
}
```

- [ ] **Step 5: Persist each output under one job**

In `ImagesJsonAdapter::execute`, create the job with the request count:

```rust
let mut job = MediaJob::new(
    job_id.clone(),
    MediaKind::Image,
    request.provider_id.clone(),
    request.model_id.clone(),
    request.prompt.clone(),
    request.count,
    created_at_ms,
);
```

Loop over outputs:

```rust
let outputs = self.request_images(provider, auth_store, &request, request_parameters.clone(), &execution)?;
let mut artifacts = Vec::new();
for (index, output) in outputs.into_iter().enumerate() {
    let artifact_id = Uuid::new_v4().to_string();
    let output_format = resolved_output_format(&request_parameters, &output.bytes);
    let filename = format!("image.{}", extension_for_output_format(&output_format));
    let artifact_path =
        service.write_image_artifact_bytes(&artifact_id, &filename, &output.bytes)?;
    let artifact = MediaArtifact {
        id: artifact_id.clone(),
        job_id: job_id.clone(),
        kind: MediaKind::Image,
        path: artifact_path.clone(),
        mime_type: mime_type_for_output_format(&output_format).to_string(),
        byte_count: output.bytes.len() as u64,
        metadata: artifact_metadata(
            &request,
            &request_parameters,
            &artifact_path,
            &output,
            index,
            created_at_ms,
        ),
        created_at_ms,
    };
    service.save_artifact(&artifact)?;
    job.attach_artifact(artifact_id, now_ms());
    artifacts.push(artifact);
}
```

Fail only when there are no outputs:

```rust
if artifacts.is_empty() {
    job.error = Some("image generation produced no images".to_string());
    job.transition(MediaJobStatus::Failed, now_ms())?;
    service.save_job(&job)?;
    bail!("image generation produced no images");
}
```

- [ ] **Step 6: Verify the OpenAI adapter test**

Run:

```bash
cargo test -p puffer-core images_json_persists_multiple_response_images_under_one_job -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit the OpenAI adapter**

```bash
git add crates/puffer-core/runtime/media/images_json.rs
git commit -m "feat(media): persist multiple images_json artifacts"
```

## Task 4: Chat Image Output And MiniMax Count Handling

**Files:**
- Modify: `crates/puffer-core/runtime/media/chat_image_output.rs`
- Modify: `crates/puffer-core/runtime/media/minimax_image.rs`

- [ ] **Step 1: Write failing chat image-output test**

Add this test to `crates/puffer-core/runtime/media/chat_image_output.rs`:

```rust
#[test]
fn chat_image_output_collects_multiple_images() {
    let value = serde_json::json!({
        "choices": [{
            "message": {
                "images": [
                    {"b64_json": "aW1hZ2UtMQ=="},
                    {"b64_json": "aW1hZ2UtMg=="}
                ]
            }
        }]
    });
    let client = Client::new();

    let outputs = chat_outputs_from_response(&client, &value, 2).unwrap();

    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].bytes, b"image-1");
    assert_eq!(outputs[1].bytes, b"image-2");
}
```

- [ ] **Step 2: Run the failing chat parser test**

Run:

```bash
cargo test -p puffer-core chat_image_output_collects_multiple_images -- --nocapture
```

Expected: FAIL because the parser returns only the first image.

- [ ] **Step 3: Change chat parser to collect outputs**

Add a vector parser:

```rust
fn chat_outputs_from_response(
    client: &Client,
    value: &Value,
    count: u8,
) -> Result<Vec<ChatImageOutput>> {
    let mut outputs = Vec::new();
    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
        for choice in choices {
            if let Some(message) = choice.get("message") {
                collect_chat_outputs_from_message(client, message, count, &mut outputs)?;
                if outputs.len() >= count as usize {
                    return Ok(outputs);
                }
            }
        }
    }
    if let Some(images) = value.get("images") {
        collect_chat_outputs_from_image_array(client, images, count, &mut outputs)?;
    }
    if outputs.is_empty() {
        bail!("chat image-output response did not contain an image");
    }
    Ok(outputs)
}
```

Change `request_image` to return `Result<Vec<ChatImageOutput>>` and call
`chat_outputs_from_response(&self.client, &value, request.count)`.

Use this collector for arrays:

```rust
fn collect_chat_outputs_from_image_array(
    client: &Client,
    value: &Value,
    count: u8,
    outputs: &mut Vec<ChatImageOutput>,
) -> Result<()> {
    let Some(images) = value.as_array() else {
        return Ok(());
    };
    for image in images {
        if outputs.len() >= count as usize {
            return Ok(());
        }
        if let Some(output) = chat_output_from_image_value(client, image) {
            outputs.push(output?);
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Persist multiple chat outputs with serial fallback**

Update `ChatImageOutputGenerationRequest`:

```rust
pub(crate) struct ChatImageOutputGenerationRequest {
    pub(crate) provider_id: String,
    pub(crate) model_id: String,
    pub(crate) adapter: String,
    pub(crate) prompt: String,
    pub(crate) parameters: BTreeMap<String, String>,
    pub(crate) count: u8,
}
```

In `ChatImageOutputAdapter::execute_with_discovery_cache`, create the job with
`request.count`, then repeat single chat image-output calls serially until the
requested count is reached or every attempt has failed:

```rust
let mut outputs = Vec::new();
let mut last_error = None;
for _ in 0..request.count {
    if outputs.len() >= request.count as usize {
        break;
    }
    match self.request_image(provider, auth_store, &execution, &request) {
        Ok(mut response_outputs) => {
            outputs.append(&mut response_outputs);
            outputs.truncate(request.count as usize);
        }
        Err(error) => {
            last_error = Some(error);
            if outputs.is_empty() {
                continue;
            }
            break;
        }
    }
}
if outputs.is_empty() {
    let error = last_error
        .map(|error| format!("{error:#}"))
        .unwrap_or_else(|| "chat image-output produced no images".to_string());
    job.error = Some(error.clone());
    job.transition(MediaJobStatus::Failed, now_ms())?;
    service.save_job(&job)?;
    bail!(error);
}
```

Persist each output under the same job:

```rust
let mut artifacts = Vec::new();
for (index, output) in outputs.into_iter().enumerate() {
    let artifact_id = Uuid::new_v4().to_string();
    let filename = "image.png";
    let artifact_path =
        service.write_image_artifact_bytes(&artifact_id, filename, &output.bytes)?;
    let artifact = MediaArtifact {
        id: artifact_id.clone(),
        job_id: job_id.clone(),
        kind: MediaKind::Image,
        path: artifact_path.clone(),
        mime_type: "image/png".to_string(),
        byte_count: output.bytes.len() as u64,
        metadata: artifact_metadata(&request, &artifact_path, &output, index, created_at_ms),
        created_at_ms,
    };
    service.save_artifact(&artifact)?;
    job.attach_artifact(artifact_id, now_ms());
    artifacts.push(artifact);
}
job.transition(MediaJobStatus::Succeeded, now_ms())?;
service.save_job(&job)?;
```

Use this metadata shape:

```rust
metadata: artifact_metadata(&request, &artifact_path, &output, index, created_at_ms),
```

- [ ] **Step 5: Add MiniMax serial count fallback**

Update `MinimaxImageGenerationRequest`:

```rust
pub(crate) struct MinimaxImageGenerationRequest {
    pub(crate) provider_id: String,
    pub(crate) model_id: String,
    pub(crate) adapter: String,
    pub(crate) prompt: String,
    pub(crate) parameters: BTreeMap<String, String>,
    pub(crate) count: u8,
}
```

In `MinimaxImageAdapter::execute`, call `request_image` serially:

```rust
let mut outputs = Vec::new();
let mut last_error = None;
for _ in 0..request.count {
    match self.request_image(provider, auth_store, &execution, &request, selected_parameters.clone()) {
        Ok(output) => outputs.push(output),
        Err(error) => last_error = Some(error),
    }
}
if outputs.is_empty() {
    let error = last_error
        .map(|error| format!("{error:#}"))
        .unwrap_or_else(|| "MiniMax image generation produced no images".to_string());
    job.error = Some(error.clone());
    job.transition(MediaJobStatus::Failed, now_ms())?;
    service.save_job(&job)?;
    bail!(error);
}
```

Persist all outputs with the same per-output loop pattern used by other image
adapters.

- [ ] **Step 6: Verify chat and MiniMax tests**

Run:

```bash
cargo test -p puffer-core chat_image_output_collects_multiple_images -- --nocapture
cargo test -p puffer-core minimax -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit adapter count handling**

```bash
git add crates/puffer-core/runtime/media/chat_image_output.rs crates/puffer-core/runtime/media/minimax_image.rs
git commit -m "feat(media): support multi-image adapter outputs"
```

## Task 5: Agent Tool Output And Tool Schema

**Files:**
- Modify: `crates/puffer-core/runtime/claude_tools/workflow/image_generation.rs`
- Modify: `resources/tools/image_generation.yaml`

- [ ] **Step 1: Write failing ImageGeneration output test**

Update or add this test in `crates/puffer-core/runtime/claude_tools/workflow/image_generation/tests/image_generation_tool_tests.rs`:

```rust
#[test]
fn image_generation_output_includes_artifacts_array() {
    let output = image_generation_output(&ImageGenerationResult {
        job_id: "job-1".to_string(),
        requested_count: 2,
        artifacts: vec![
            ImageGenerationArtifactResult {
                artifact_id: "artifact-1".to_string(),
                index: 0,
                path: PathBuf::from("/tmp/image-1.png"),
                mime_type: "image/png".to_string(),
                byte_count: 10,
            },
            ImageGenerationArtifactResult {
                artifact_id: "artifact-2".to_string(),
                index: 1,
                path: PathBuf::from("/tmp/image-2.png"),
                mime_type: "image/png".to_string(),
                byte_count: 11,
            },
        ],
        provider: "openai".to_string(),
        model: "gpt-image-1".to_string(),
        status: "succeeded".to_string(),
        parameters: BTreeMap::new(),
        purpose: None,
        retry_from_error: false,
    })
    .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

    assert_eq!(parsed["jobId"], "job-1");
    assert_eq!(parsed["requestedCount"], 2);
    assert!(parsed.get("artifactId").is_none());
    assert!(parsed.get("path").is_none());
    assert_eq!(parsed["artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["artifacts"][0]["artifactId"], "artifact-1");
    assert_eq!(parsed["artifacts"][1]["index"], 1);
}
```

- [ ] **Step 2: Run the failing ImageGeneration output test**

Run:

```bash
cargo test -p puffer-core image_generation_output_includes_artifacts_array -- --nocapture
```

Expected: FAIL because the tool output still emits `artifactId` and `path`.

- [ ] **Step 3: Add `count` to tool input**

Update `ImageGenerationInput`:

```rust
struct ImageGenerationInput {
    prompt: String,
    #[serde(default)]
    prompt_reference: Option<String>,
    #[serde(default)]
    aspect: Option<String>,
    #[serde(default = "default_image_count")]
    count: u8,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    retry_from_error: Option<Value>,
}
```

Add:

```rust
fn default_image_count() -> u8 {
    1
}
```

- [ ] **Step 4: Pass count into exact generation**

When building `ExactImageGenerationRequest`, include:

```rust
count: request.count,
```

- [ ] **Step 5: Emit `artifacts[]`**

Replace the single result struct with:

```rust
struct ImageGenerationArtifactResult {
    artifact_id: String,
    index: usize,
    path: PathBuf,
    mime_type: String,
    byte_count: u64,
}
```

```rust
struct ImageGenerationResult {
    job_id: String,
    requested_count: u8,
    artifacts: Vec<ImageGenerationArtifactResult>,
    provider: String,
    model: String,
    status: String,
    parameters: BTreeMap<String, String>,
    purpose: Option<String>,
    retry_from_error: bool,
}
```

Update output JSON:

```rust
Ok(serde_json::to_string_pretty(&json!({
    "jobId": result.job_id,
    "requestedCount": result.requested_count,
    "artifacts": result.artifacts.iter().map(|artifact| json!({
        "artifactId": artifact.artifact_id,
        "index": artifact.index,
        "path": artifact.path,
        "mimeType": artifact.mime_type,
        "size": artifact.byte_count
    })).collect::<Vec<_>>(),
    "provider": result.provider,
    "model": result.model,
    "status": result.status,
    "parameters": result.parameters,
    "purpose": result.purpose,
    "retryFromError": result.retry_from_error
}))?)
```

- [ ] **Step 6: Update tool schema**

Add `count` to `resources/tools/image_generation.yaml` input schema:

```yaml
count:
  type: integer
  description: Number of images to generate in one request. Must be between 1 and 4.
  minimum: 1
  maximum: 4
```

- [ ] **Step 7: Verify ImageGeneration tests**

Run:

```bash
cargo test -p puffer-core image_generation_output_includes_artifacts_array -- --nocapture
cargo test -p puffer-core image_generation -- --nocapture
```

Expected: PASS.

- [ ] **Step 8: Commit tool output**

```bash
git add crates/puffer-core/runtime/claude_tools/workflow/image_generation.rs crates/puffer-core/runtime/claude_tools/workflow/image_generation/tests/image_generation_tool_tests.rs resources/tools/image_generation.yaml
git commit -m "feat(media): emit image generation artifacts array"
```

## Task 6: Daemon And Desktop DTO Contracts

**Files:**
- Modify: `crates/puffer-cli/src/daemon.rs`
- Modify: `crates/puffer-cli/src/desktop_api_types.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/backend.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/dtos.rs`

- [ ] **Step 1: Write failing daemon serialization test**

Add this test to `crates/puffer-cli/src/daemon.rs` tests:

```rust
#[test]
fn generate_media_result_serializes_artifacts_array() {
    let result = GenerateMediaResult {
        job_id: "job-1".to_string(),
        requested_count: 2,
        artifacts: vec![
            GenerateMediaArtifactResult {
                artifact_id: "artifact-1".to_string(),
                index: 0,
                path: "/tmp/image-1.png".to_string(),
                mime_type: "image/png".to_string(),
                size: 10,
            },
            GenerateMediaArtifactResult {
                artifact_id: "artifact-2".to_string(),
                index: 1,
                path: "/tmp/image-2.png".to_string(),
                mime_type: "image/png".to_string(),
                size: 11,
            },
        ],
        kind: "image".to_string(),
        provider_id: "openai".to_string(),
        model_id: "gpt-image-1".to_string(),
        status: "succeeded".to_string(),
        prompt: "draw".to_string(),
    };
    let value = serde_json::to_value(result).unwrap();

    assert!(value.get("artifactId").is_none());
    assert!(value.get("path").is_none());
    assert_eq!(value["requestedCount"], 2);
    assert_eq!(value["artifacts"].as_array().unwrap().len(), 2);
}
```

- [ ] **Step 2: Write failing Tauri backend serialization test**

Add the same serialization contract test to `apps/puffer-desktop/src-tauri/src/backend.rs` tests:

```rust
#[test]
fn generate_media_result_serializes_artifacts_array() {
    let result = GenerateMediaResult {
        job_id: "job-1".to_string(),
        requested_count: 2,
        artifacts: vec![
            GenerateMediaArtifactResult {
                artifact_id: "artifact-1".to_string(),
                index: 0,
                path: "/tmp/image-1.png".to_string(),
                mime_type: "image/png".to_string(),
                size: 10,
            },
            GenerateMediaArtifactResult {
                artifact_id: "artifact-2".to_string(),
                index: 1,
                path: "/tmp/image-2.png".to_string(),
                mime_type: "image/png".to_string(),
                size: 11,
            },
        ],
        kind: "image".to_string(),
        provider_id: "openai".to_string(),
        model_id: "gpt-image-1".to_string(),
        status: "succeeded".to_string(),
        prompt: "draw".to_string(),
    };
    let value = serde_json::to_value(result).unwrap();

    assert!(value.get("artifactId").is_none());
    assert!(value.get("path").is_none());
    assert_eq!(value["requestedCount"], 2);
    assert_eq!(value["artifacts"].as_array().unwrap().len(), 2);
}
```

- [ ] **Step 3: Run the failing serialization tests**

Run:

```bash
cargo test -p puffer-cli generate_media_result_serializes_artifacts_array -- --nocapture
cargo test --manifest-path apps/puffer-desktop/src-tauri/Cargo.toml generate_media_result_serializes_artifacts_array -- --nocapture
```

Expected: FAIL because daemon and Tauri backend results still have single artifact fields.

- [ ] **Step 4: Update daemon and Tauri request/result DTOs**

Update `GenerateMediaParams` in both `crates/puffer-cli/src/daemon.rs` and
`apps/puffer-desktop/src-tauri/src/backend.rs`:

```rust
struct GenerateMediaParams {
    kind: String,
    prompt: String,
    #[serde(default = "default_generate_media_count")]
    count: u8,
}
```

Add:

```rust
fn default_generate_media_count() -> u8 {
    1
}
```

Replace result structs in both files:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateMediaArtifactResult {
    artifact_id: String,
    index: usize,
    path: String,
    mime_type: String,
    size: u64,
}
```

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateMediaResult {
    job_id: String,
    requested_count: u8,
    artifacts: Vec<GenerateMediaArtifactResult>,
    kind: String,
    provider_id: String,
    model_id: String,
    status: String,
    prompt: String,
}
```

- [ ] **Step 5: Pass count through daemon and Tauri generation**

In `crates/puffer-cli/src/daemon.rs`, keep `prompt` validation in
`handle_generate_media_job` and pass the typed count into image generation:

```rust
let prompt = input.prompt.trim().to_string();
let count = input.count;
let result = match input.kind.as_str() {
    "image" => generate_image_media_job(state, prompt, count)?,
    other => bail!("unsupported media kind `{other}`"),
};
```

Update the image helper signature:

```rust
fn generate_image_media_job(state: &DaemonState, prompt: String, count: u8) -> Result<Value> {
```

In `apps/puffer-desktop/src-tauri/src/backend.rs`, pass `count` through the
same way:

```rust
let prompt = input.prompt.trim().to_string();
let count = input.count;
match input.kind.as_str() {
    "image" => self.generate_image_media_job(prompt, count),
    "video" => self.generate_video_media_job(prompt),
    other => bail!("unsupported media kind `{other}`"),
}
```

Update the Tauri image helper signature:

```rust
fn generate_image_media_job(&self, prompt: String, count: u8) -> Result<GenerateMediaResult> {
```

When building `ExactImageGenerationRequest` in both paths, include:

```rust
count,
```

Build the result in both paths:

```rust
let artifacts = generation
    .artifacts
    .into_iter()
    .map(|artifact| GenerateMediaArtifactResult {
        artifact_id: artifact.artifact_id,
        index: artifact.index,
        path: artifact.path.display().to_string(),
        mime_type: artifact.mime_type,
        size: artifact.byte_count,
    })
    .collect();
let result = GenerateMediaResult {
    job_id: generation.job_id,
    requested_count: generation.requested_count,
    artifacts,
    kind: "image".to_string(),
    provider_id: generation.provider_id,
    model_id: generation.model_id,
    status: generation.status,
    prompt,
};
```

- [ ] **Step 6: Add generated-media source grouping fields**

Update generated-media source in `crates/puffer-cli/src/desktop_api_types.rs` and `apps/puffer-desktop/src-tauri/src/dtos.rs`:

```rust
GeneratedMedia {
    #[serde(rename = "jobId")]
    job_id: String,
    #[serde(rename = "artifactId")]
    artifact_id: String,
    index: usize,
}
```

- [ ] **Step 7: Verify daemon and Tauri DTO tests**

Run:

```bash
cargo test -p puffer-cli generate_media_result_serializes_artifacts_array -- --nocapture
cargo test -p puffer-cli generated_media -- --nocapture
cargo test --manifest-path apps/puffer-desktop/src-tauri/Cargo.toml generate_media_result_serializes_artifacts_array -- --nocapture
cargo test --manifest-path apps/puffer-desktop/src-tauri/Cargo.toml generated_media -- --nocapture
```

Expected: PASS.

- [ ] **Step 8: Commit desktop DTO contracts**

```bash
git add crates/puffer-cli/src/daemon.rs crates/puffer-cli/src/desktop_api_types.rs apps/puffer-desktop/src-tauri/src/backend.rs apps/puffer-desktop/src-tauri/src/dtos.rs
git commit -m "feat(desktop): return generated media artifacts"
```

## Task 7: Timeline Synthesis From `artifacts[]`

**Files:**
- Modify: `crates/puffer-cli/src/desktop_api.rs`
- Modify: `apps/puffer-desktop/src-tauri/src/session_data.rs`

- [ ] **Step 1: Write failing CLI timeline test**

Add or replace a generated media timeline test in `crates/puffer-cli/src/desktop_api.rs`:

```rust
#[test]
fn timeline_synthesizes_multiple_generated_attachments_from_one_tool_output() {
    let workspace = tempfile::tempdir().unwrap();
    write_generated_image_artifact(&workspace, "artifact-1", "image.png", b"\x89PNG\r\n\x1a\n");
    write_generated_image_artifact(&workspace, "artifact-2", "image.png", b"\x89PNG\r\n\x1a\n");
    let record = session_record_with_events(
        workspace.path(),
        vec![TranscriptEvent::ToolInvocation {
            tool_id: "ImageGeneration".to_string(),
            input: serde_json::json!({"prompt": "draw", "count": 2}),
            output: serde_json::json!({
                "jobId": "job-1",
                "requestedCount": 2,
                "status": "succeeded",
                "artifacts": [
                    {"artifactId": "artifact-1", "index": 0, "path": "/ignored-1.png", "mimeType": "image/png", "size": 8},
                    {"artifactId": "artifact-2", "index": 1, "path": "/ignored-2.png", "mimeType": "image/png", "size": 8}
                ]
            })
            .to_string(),
            success: true,
        }],
    );

    let items = timeline_items_from_record(&record);
    let assistant = items
        .iter()
        .find_map(|item| match item {
            TimelineItemDto::AssistantMessage(message) => Some(message),
            _ => None,
        })
        .expect("assistant generated media item");
    let attachments = assistant.attachments.as_ref().expect("attachments");

    assert_eq!(attachments.len(), 2);
    assert_eq!(attachments[0].id, "generated-image:artifact-1");
    assert_eq!(attachments[1].id, "generated-image:artifact-2");
    assert!(matches!(
        attachments[0].source,
        ChatAttachmentSourceDto::GeneratedMedia { ref job_id, ref artifact_id, index }
            if job_id == "job-1" && artifact_id == "artifact-1" && index == 0
    ));
}
```

- [ ] **Step 2: Write failing Tauri timeline test**

Update `tauri_timeline_attaches_generated_image_to_assistant_message` in
`apps/puffer-desktop/src-tauri/src/session_data.rs` to seed two artifacts and
assert two generated attachments:

```rust
#[test]
fn tauri_timeline_attaches_generated_images_to_assistant_message() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    write_generated_image_artifact(&workspace, "artifact-1", "image.jpeg", b"\xff\xd8\xff\xd9");
    write_generated_image_artifact(&workspace, "artifact-2", "image.jpeg", b"\xff\xd8\xff\xd9");
    let record = session_record_with_events(
        workspace,
        vec![
            TranscriptEvent::ToolInvocation {
                call_id: "call-img".to_string(),
                tool_id: "ImageGeneration".to_string(),
                input: serde_json::json!({"prompt": "draw", "count": 2}).to_string(),
                output: serde_json::json!({
                    "jobId": "job-1",
                    "requestedCount": 2,
                    "status": "succeeded",
                    "artifacts": [
                        {"artifactId": "artifact-1", "index": 0, "path": "/ignored-1.png", "mimeType": "image/png", "size": 8},
                        {"artifactId": "artifact-2", "index": 1, "path": "/ignored-2.png", "mimeType": "image/png", "size": 8}
                    ]
                })
                .to_string(),
                success: true,
                actor: None,
                subject: None,
                metadata: None,
            },
            TranscriptEvent::AssistantMessage {
                text: "Done".to_string(),
                actor: None,
            },
        ],
    );
    let store = SessionStore::from_paths(&ConfigPaths::discover(temp.path())).unwrap();

    let items = timeline_items(&store, &record);

    let Some(TimelineItemDto::AssistantMessage { attachments, .. }) = items
        .iter()
        .find(|item| matches!(item, TimelineItemDto::AssistantMessage { .. }))
    else {
        panic!("assistant message exists");
    };
    assert_eq!(attachments.len(), 2);
    assert!(matches!(
        attachments[0].source,
        ChatAttachmentSourceDto::GeneratedMedia { ref job_id, ref artifact_id, index }
            if job_id == "job-1" && artifact_id == "artifact-1" && index == 0
    ));
    assert!(matches!(
        attachments[1].source,
        ChatAttachmentSourceDto::GeneratedMedia { ref job_id, ref artifact_id, index }
            if job_id == "job-1" && artifact_id == "artifact-2" && index == 1
    ));
}
```

- [ ] **Step 3: Run the failing timeline tests**

Run:

```bash
cargo test -p puffer-cli timeline_synthesizes_multiple_generated_attachments_from_one_tool_output -- --nocapture
cargo test --manifest-path apps/puffer-desktop/src-tauri/Cargo.toml tauri_timeline_attaches_generated_images_to_assistant_message -- --nocapture
```

Expected: FAIL because timeline synthesis parses only `artifactId`.

- [ ] **Step 4: Parse artifacts array in CLI desktop API**

Replace the single generated attachment helper with:

```rust
fn generated_image_attachments(cwd: &Path, output: &str) -> Vec<ChatAttachmentDto> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
        return Vec::new();
    };
    let job_id = value
        .get("jobId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    if job_id.is_empty() {
        return Vec::new();
    }
    value
        .get("artifacts")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|artifact| generated_image_attachment(cwd, job_id, artifact))
        .collect()
}
```

Add:

```rust
fn generated_image_attachment(
    cwd: &Path,
    job_id: &str,
    artifact: &serde_json::Value,
) -> Option<ChatAttachmentDto> {
    let artifact_id = artifact.get("artifactId")?.as_str()?.trim();
    if artifact_id.is_empty() {
        return None;
    }
    let index = artifact
        .get("index")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let metadata = generated_media_attachment_metadata(cwd, artifact_id)?;
    let extension = generated_image_extension(&metadata.mime_type).to_string();
    Some(ChatAttachmentDto {
        id: format!("generated-image:{artifact_id}"),
        name: "Generated image".to_string(),
        mime_type: metadata.mime_type,
        size: metadata.byte_count,
        extension,
        kind: "image".to_string(),
        state: metadata.state,
        source: ChatAttachmentSourceDto::GeneratedMedia {
            job_id: job_id.to_string(),
            artifact_id: artifact_id.to_string(),
            index,
        },
    })
}
```

- [ ] **Step 5: Buffer all attachments from one tool output**

In the `TranscriptEvent::ToolInvocation` branch, replace pushing one optional
attachment with:

```rust
if *success && tool_id == "ImageGeneration" {
    pending_generated_attachments.extend(generated_image_attachments(&record.metadata.cwd, output));
}
```

- [ ] **Step 6: Parse artifacts array in Tauri session data**

In `apps/puffer-desktop/src-tauri/src/session_data.rs`, replace the single
`generated_image_attachment` helper with the same two-helper shape used by
`crates/puffer-cli/src/desktop_api.rs`. The only type names that differ are
the local imports from `crate::dtos`.

Use this source construction in the Tauri helper:

```rust
source: ChatAttachmentSourceDto::GeneratedMedia {
    job_id: job_id.to_string(),
    artifact_id: artifact_id.to_string(),
    index,
},
```

- [ ] **Step 7: Verify timeline tests**

Run:

```bash
cargo test -p puffer-cli timeline_synthesizes_multiple_generated_attachments_from_one_tool_output -- --nocapture
cargo test -p puffer-cli generated_media -- --nocapture
cargo test --manifest-path apps/puffer-desktop/src-tauri/Cargo.toml tauri_timeline_attaches_generated_images_to_assistant_message -- --nocapture
cargo test --manifest-path apps/puffer-desktop/src-tauri/Cargo.toml generated_media -- --nocapture
```

Expected: PASS.

- [ ] **Step 8: Commit timeline synthesis**

```bash
git add crates/puffer-cli/src/desktop_api.rs apps/puffer-desktop/src-tauri/src/session_data.rs
git commit -m "feat(desktop): synthesize multi-image attachments"
```

## Task 8: Frontend Types And Live Preview

**Files:**
- Modify: `apps/puffer-desktop/src/lib/types.ts`
- Modify: `apps/puffer-desktop/src/lib/api/desktop.ts`
- Modify: `apps/puffer-desktop/src/App.svelte`
- Modify: `apps/puffer-desktop/tests/support/fakeDaemon.ts`

- [ ] **Step 1: Write failing frontend type test**

Update `apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts` with:

```ts
it("keeps generated media grouping fields", () => {
  const generatedAttachment: MessageAttachment = {
    id: "generated-image:artifact-1",
    name: "Generated image",
    mimeType: "image/png",
    size: 8,
    extension: "PNG",
    kind: "image",
    state: "available",
    source: {
      kind: "generated_media",
      jobId: "job-1",
      artifactId: "artifact-1",
      index: 0
    }
  };

  expect(generatedAttachment.source.kind).toBe("generated_media");
  if (generatedAttachment.source.kind === "generated_media") {
    expect(generatedAttachment.source.jobId).toBe("job-1");
    expect(generatedAttachment.source.index).toBe(0);
  }
});
```

- [ ] **Step 2: Run the failing frontend type test**

Run:

```bash
cd apps/puffer-desktop
npm test -- desktop.attachment-types.test.ts --run
```

Expected: FAIL because generated media source lacks `jobId` and `index`.

- [ ] **Step 3: Update frontend types**

In `apps/puffer-desktop/src/lib/types.ts`:

```ts
export type AttachmentPreviewSource =
  | { kind: "user_upload" }
  | { kind: "generated_media"; jobId: string; artifactId: string; index: number };
```

Update generation input/result types:

```ts
export type GenerateMediaInput = {
  kind: MediaKind;
  prompt: string;
  count?: number;
};

export type GeneratedMediaArtifactResult = {
  artifactId: string;
  index: number;
  path: string;
  mimeType: string;
  size: number;
};

export type GenerateMediaResult = {
  jobId: string;
  requestedCount: number;
  artifacts: GeneratedMediaArtifactResult[];
  kind: MediaKind;
  providerId: string;
  modelId: string;
  status: string;
  prompt: string;
};
```

- [ ] **Step 4: Normalize backend source and result shapes**

In `apps/puffer-desktop/src/lib/api/desktop.ts`, update backend types to match
the new shape:

```ts
type BackendChatAttachmentSource =
  | { kind: "user_upload" }
  | { kind: "generated_media"; jobId: string; artifactId: string; index: number };
```

Ensure generated-media preview still reads only artifact id:

```ts
if (attachment.source.kind === "generated_media") {
  return readGeneratedMediaPreview(sessionId, attachment.source.artifactId);
}
```

- [ ] **Step 5: Render live generated images as one assistant item**

In `apps/puffer-desktop/src/App.svelte`, replace single artifact live handling with:

```ts
async function generatedImageAttachment(
  sessionId: string,
  jobId: string,
  artifact: GeneratedMediaArtifactResult
): Promise<MessageAttachment> {
  const preview = await readGeneratedMediaPreview(sessionId, artifact.artifactId).catch(() => ({
    state: "missing" as const
  }));
  if (preview.state !== "available") {
    return {
      id: generatedImageAttachmentId(artifact.artifactId),
      name: "Generated image",
      mimeType: artifact.mimeType || "image/*",
      size: artifact.size || 0,
      extension: generatedImageExtension(artifact.mimeType || "image/*"),
      kind: "image",
      state: "missing",
      source: { kind: "generated_media", jobId, artifactId: artifact.artifactId, index: artifact.index },
      previewUrl: null
    };
  }
  const bytes = new Uint8Array(preview.bytes);
  return {
    id: generatedImageAttachmentId(artifact.artifactId),
    name: "Generated image",
    mimeType: preview.mimeType,
    size: bytes.byteLength,
    extension: generatedImageExtension(preview.mimeType),
    kind: "image",
    state: "available",
    source: { kind: "generated_media", jobId, artifactId: artifact.artifactId, index: artifact.index },
    previewUrl: URL.createObjectURL(new Blob([bytes], { type: preview.mimeType }))
  };
}
```

Update `appendGeneratedImagePreview`:

```ts
async function appendGeneratedImagePreview(
  sessionId: string,
  result: GenerateMediaResult
): Promise<void> {
  const artifacts = result.artifacts ?? [];
  if (artifacts.length === 0) return;
  const attachments = await Promise.all(
    artifacts.map((artifact) => generatedImageAttachment(sessionId, result.jobId, artifact))
  );
  if (selectedSession?.id !== sessionId) {
    revokeAttachmentPreviews(attachments);
    return;
  }
  appendLive({
    id: `${GENERATED_IMAGE_PREVIEW_ID_PREFIX}${result.jobId}`,
    kind: "assistant",
    title: "Assistant",
    summary: artifacts.length === 1 ? "Generated image" : `Generated ${artifacts.length} images`,
    body: "",
    meta: [],
    status: result.status,
    attachments
  });
}
```

- [ ] **Step 6: Update fake daemon generated result**

In `apps/puffer-desktop/tests/support/fakeDaemon.ts`, return:

```ts
return {
  jobId,
  requestedCount: artifacts.length,
  artifacts,
  kind: "image",
  providerId,
  modelId,
  status: "succeeded",
  prompt
};
```

- [ ] **Step 7: Verify frontend tests**

Run:

```bash
cd apps/puffer-desktop
npm test -- desktop.attachment-types.test.ts --run
```

Expected: PASS.

- [ ] **Step 8: Commit frontend type and live preview changes**

```bash
git add apps/puffer-desktop/src/lib/types.ts apps/puffer-desktop/src/lib/api/desktop.ts apps/puffer-desktop/src/App.svelte apps/puffer-desktop/tests/support/fakeDaemon.ts apps/puffer-desktop/src/lib/api/desktop.attachment-types.test.ts
git commit -m "feat(desktop): render multi-image generation results"
```

## Task 9: End-To-End Regression Coverage

**Files:**
- Modify: `apps/puffer-desktop/tests/chat-session-ui.spec.ts`
- Modify: `apps/puffer-desktop/tests/support/fakeDaemon.ts`

- [ ] **Step 1: Add failing persisted multi-image UI test**

Add a test case in `apps/puffer-desktop/tests/chat-session-ui.spec.ts`:

```ts
test("shows two generated image attachments from one image generation result", async ({ page }) => {
  const sessionId = "session-generated-two";
  await fakeDaemon.seedSession({
    id: sessionId,
    title: "Generated images",
    cwd: "/workspace/generated",
    timeline: [
      {
        id: "assistant-generated-two",
        kind: "assistant",
        title: "Assistant",
        summary: "Generated 2 images",
        body: "",
        meta: [],
        status: "succeeded",
        attachments: [
          generatedAttachment("job-1", "artifact-1", 0),
          generatedAttachment("job-1", "artifact-2", 1)
        ]
      }
    ]
  });
  fakeDaemon.setGeneratedMediaPreview(sessionId, "artifact-1", {
    state: "available",
    mimeType: "image/png",
    bytes: pngBytes()
  });
  fakeDaemon.setGeneratedMediaPreview(sessionId, "artifact-2", {
    state: "available",
    mimeType: "image/png",
    bytes: pngBytes()
  });

  await page.goto("/");
  await openSession(page, sessionId);

  await expect(page.getByRole("img", { name: /Generated image/i })).toHaveCount(2);
});
```

Add this local helper in the test file:

```ts
function generatedAttachment(jobId: string, artifactId: string, index: number): MessageAttachment {
  return {
    id: `generated-image:${artifactId}`,
    name: "Generated image",
    mimeType: "image/png",
    size: 8,
    extension: "PNG",
    kind: "image",
    state: "available",
    source: { kind: "generated_media", jobId, artifactId, index }
  };
}
```

- [ ] **Step 2: Run the failing persisted UI test**

Run:

```bash
cd apps/puffer-desktop
npm test -- chat-session-ui.spec.ts --run
```

Expected: FAIL before the frontend and fake daemon changes from Task 8 are complete; PASS after Task 8.

- [ ] **Step 3: Verify full targeted suite**

Run:

```bash
cargo test -p puffer-core image_generation -- --nocapture
cargo test -p puffer-core media -- --nocapture
cargo test -p puffer-cli generated_media -- --nocapture
cd apps/puffer-desktop
npm test -- desktop.attachment-types.test.ts chat-session-ui.spec.ts --run
```

Expected: PASS.

- [ ] **Step 4: Commit regression coverage**

```bash
git add apps/puffer-desktop/tests/chat-session-ui.spec.ts apps/puffer-desktop/tests/support/fakeDaemon.ts
git commit -m "test(desktop): cover multi-image generated attachments"
```

## Task 10: Final Verification

**Files:**
- No source edits unless a prior targeted test exposed a defect.

- [ ] **Step 1: Run Rust workspace tests**

Run:

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 2: Run desktop frontend tests**

Run:

```bash
cd apps/puffer-desktop
npm test -- --run
```

Expected: PASS.

- [ ] **Step 3: Review git diff**

Run:

```bash
git status --short
git log --oneline -10
```

Expected: working tree is clean after the final task commit, and the last commits are the task commits from this plan.
