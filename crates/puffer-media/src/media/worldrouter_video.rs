use super::video_jobs::{
    complete_video_job, map_video_task_status, persist_failed_video_job,
    poll_video_until_terminal, video_poll_url, CompletedVideoTask, VideoPollingConfig,
};
use super::{MediaGenerationService, MediaJob, MediaJobStatus, MediaKind};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Map, Number, Value};
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

/// Adapter identifier for WorldRouter Seedance video generation.
pub(crate) const WORLDROUTER_VIDEO_ADAPTER: &str = "worldrouter_video";

/// One WorldRouter Seedance video generation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorldRouterVideoRequest {
    pub(crate) model: String,
    pub(crate) prompt: String,
    pub(crate) image_references: Vec<String>,
    pub(crate) params: BTreeMap<String, String>,
}

impl WorldRouterVideoRequest {
    /// Builds the WorldRouter Seedance task request body.
    pub(crate) fn request_body(
        &self,
        asset_group_id: Option<&str>,
        asset_urls: &[String],
    ) -> Result<Value> {
        self.validate()?;
        if self.image_references.len() != asset_urls.len() {
            bail!(
                "WorldRouter image reference count {} does not match uploaded asset count {}",
                self.image_references.len(),
                asset_urls.len()
            );
        }
        if !asset_urls.is_empty() && asset_group_id.unwrap_or("").trim().is_empty() {
            bail!("WorldRouter image-to-video requires an asset group id");
        }

        let mut body = Map::new();
        body.insert("model".to_string(), json!(self.model.trim()));
        if let Some(group) = asset_group_id.map(str::trim).filter(|v| !v.is_empty()) {
            body.insert("asset_group_id".to_string(), json!(group));
        }

        let mut content = vec![json!({
            "type": "text",
            "text": self.prompt.trim()
        })];
        for url in asset_urls {
            content.push(json!({
                "type": "image_url",
                "role": "reference_image",
                "image_url": { "url": url.trim() }
            }));
        }
        body.insert("content".to_string(), Value::Array(content));

        for (field, value) in &self.params {
            body.insert(
                field.trim().to_string(),
                worldrouter_request_value(field, value)?,
            );
        }
        Ok(Value::Object(body))
    }

    fn validate(&self) -> Result<()> {
        if self.model.trim().is_empty() {
            bail!("WorldRouter video model is required");
        }
        if self.prompt.trim().is_empty() {
            bail!("WorldRouter video prompt is required");
        }
        for (index, reference) in self.image_references.iter().enumerate() {
            validate_image_reference(reference, index)?;
        }
        for (field, value) in &self.params {
            let _ = worldrouter_request_value(field, value)?;
        }
        Ok(())
    }
}

fn worldrouter_request_value(field: &str, value: &str) -> Result<Value> {
    let value = value.trim();
    if field == "duration" {
        if let Ok(number) = value.parse::<i64>() {
            return Ok(Value::Number(Number::from(number)));
        }
        let number = value
            .parse::<f64>()
            .with_context(|| format!("WorldRouter video parameter {field} must be numeric"))?;
        return Number::from_f64(number)
            .map(Value::Number)
            .with_context(|| format!("WorldRouter video parameter {field} must be finite"));
    }
    Ok(json!(value))
}

fn validate_image_reference(reference: &str, index: usize) -> Result<()> {
    let reference = reference.trim();
    let url = reqwest::Url::parse(reference)
        .with_context(|| format!("WorldRouter image reference {index} must be an https:// URL"))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        bail!("WorldRouter image reference {index} must be an https:// URL");
    }
    Ok(())
}

/// Parsed response from the WorldRouter asset-group creation endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorldRouterAssetGroup {
    pub(crate) id: String,
}

impl WorldRouterAssetGroup {
    /// Parses a WorldRouter asset-group response.
    pub(crate) fn from_value(value: Value) -> Result<Self> {
        Ok(Self {
            id: string_field(&value, &["id"]).ok_or_else(|| {
                anyhow!(
                    "create asset group response missing id: {}",
                    response_shape_summary(&value)
                )
            })?,
        })
    }
}

/// Parsed response from the WorldRouter asset upload endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorldRouterAsset {
    pub(crate) url: String,
}

impl WorldRouterAsset {
    /// Parses a WorldRouter asset upload response.
    pub(crate) fn from_value(value: Value) -> Result<Self> {
        let url = string_field(&value, &["url"]).ok_or_else(|| {
            anyhow!(
                "upload asset response missing asset URL: {}",
                response_shape_summary(&value)
            )
        })?;
        if !url.starts_with("asset://") {
            bail!("upload asset response URL must start with asset://");
        }
        Ok(Self { url })
    }
}

/// Parsed response from a WorldRouter task-submission response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorldRouterSubmitTask {
    pub(crate) id: String,
    pub(crate) request_id: Option<String>,
}

impl WorldRouterSubmitTask {
    /// Parses a WorldRouter task-submission response.
    pub(crate) fn from_value(value: Value) -> Result<Self> {
        Ok(Self {
            id: string_field(&value, &["id"]).ok_or_else(|| {
                anyhow!(
                    "submit response missing task id: {}",
                    response_shape_summary(&value)
                )
            })?,
            request_id: string_field(&value, &["requestId", "request_id"]),
        })
    }
}

/// Parsed response from a WorldRouter task-poll response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorldRouterVideoTask {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) video_url: Option<String>,
    pub(crate) error: Option<String>,
}

impl WorldRouterVideoTask {
    /// Parses a WorldRouter task-poll response.
    pub(crate) fn from_value(value: Value) -> Result<Self> {
        Ok(Self {
            id: string_field(&value, &["id"]).ok_or_else(|| {
                anyhow!(
                    "poll response missing task id: {}",
                    response_shape_summary(&value)
                )
            })?,
            status: string_field(&value, &["status"]).ok_or_else(|| {
                anyhow!(
                    "poll response missing status: {}",
                    response_shape_summary(&value)
                )
            })?,
            video_url: value
                .get("content")
                .and_then(|content| content.get("video_url"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            error: worldrouter_error_message(&value),
        })
    }

    /// Maps the WorldRouter status string onto a media job status.
    pub(crate) fn media_status(&self) -> MediaJobStatus {
        map_video_task_status(&self.status)
    }
}

fn worldrouter_error_message(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .or_else(|| value.get("failure_reason"))
        .or_else(|| value.get("fail_reason"))
        .or_else(|| value.get("reason"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_string)
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

fn response_shape_summary(value: &Value) -> String {
    let keys = value
        .as_object()
        .map(|object| {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys.join(",")
        })
        .unwrap_or_else(|| "non-object".to_string());
    format!("keys=[{keys}]")
}

/// Abstracts WorldRouter video HTTP operations for production and tests.
pub(crate) trait WorldRouterVideoTransport {
    /// Creates an asset group for one image-to-video generation request.
    fn create_asset_group(&self, url: &str, api_token: &str, body: &Value) -> Result<Value>;

    /// Uploads one public reference image URL into an existing asset group.
    fn upload_asset(&self, url: &str, api_token: &str, body: &Value) -> Result<Value>;

    /// Submits a WorldRouter Seedance video task and returns its JSON response.
    fn submit_task(&self, url: &str, api_token: &str, body: &Value) -> Result<Value>;

    /// Polls a WorldRouter Seedance video task and returns its JSON response.
    fn poll_task(&self, url: &str, api_token: &str) -> Result<Value>;

    /// Downloads rendered video bytes from a validated remote URL.
    fn download_bytes(&self, url: &str) -> Result<Vec<u8>>;
}

/// Reqwest-backed WorldRouter video transport used by the runtime adapter.
#[derive(Debug, Clone, Default)]
pub(crate) struct ReqwestWorldRouterVideoTransport {
    client: Client,
}

impl WorldRouterVideoTransport for ReqwestWorldRouterVideoTransport {
    fn create_asset_group(&self, url: &str, api_token: &str, body: &Value) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .bearer_auth(api_token)
            .json(body)
            .send()
            .with_context(|| format!("create WorldRouter asset group {url}"))?;
        json_response(response, "create WorldRouter asset group")
    }

    fn upload_asset(&self, url: &str, api_token: &str, body: &Value) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .bearer_auth(api_token)
            .json(body)
            .send()
            .with_context(|| format!("upload WorldRouter asset {url}"))?;
        json_response(response, "upload WorldRouter asset")
    }

    fn submit_task(&self, url: &str, api_token: &str, body: &Value) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .bearer_auth(api_token)
            .json(body)
            .send()
            .with_context(|| format!("submit WorldRouter video task {url}"))?;
        json_response(response, "submit WorldRouter video task")
    }

    fn poll_task(&self, url: &str, api_token: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .bearer_auth(api_token)
            .send()
            .with_context(|| format!("poll WorldRouter video task {url}"))?;
        json_response(response, "poll WorldRouter video task")
    }

    fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        super::http_support::download_image_url(&self.client, url, "video output")
    }
}

fn json_response(response: reqwest::blocking::Response, label: &str) -> Result<Value> {
    let status = response.status();
    let text = response
        .text()
        .with_context(|| format!("read {label} response body"))?;
    if !status.is_success() {
        bail!("{label} failed with status {}: {text}", status.as_u16());
    }
    serde_json::from_str(&text).with_context(|| format!("parse {label} response JSON"))
}

fn asset_groups_url(submit_url: &str) -> Result<String> {
    let mut url =
        reqwest::Url::parse(submit_url).context("WorldRouter submit URL must be absolute")?;
    url.set_path("/v1/asset-groups");
    url.set_query(None);
    Ok(url.to_string())
}

fn asset_upload_url(submit_url: &str, group_id: &str) -> Result<String> {
    let mut url =
        reqwest::Url::parse(submit_url).context("WorldRouter submit URL must be absolute")?;
    url.set_path(&format!("/v1/asset-groups/{}/assets", group_id.trim()));
    url.set_query(None);
    Ok(url.to_string())
}

fn asset_group_body() -> Value {
    json!({
        "name": "puffer-seedance-video",
        "description": "reference assets for one Puffer Seedance video generation"
    })
}

fn asset_upload_body(index: usize, source_url: &str) -> Value {
    json!({
        "name": format!("reference-image-{}", index + 1),
        "description": format!("Puffer Seedance reference image {}", index + 1),
        "type": "image",
        "url": source_url.trim()
    })
}

/// Shared polling configuration for WorldRouter video tasks.
pub(crate) type WorldRouterVideoPollingConfig = VideoPollingConfig;

/// Submits and polls WorldRouter Seedance video tasks into media jobs.
pub(crate) struct WorldRouterVideoAdapter<T = ReqwestWorldRouterVideoTransport> {
    api_token: String,
    submit_url: String,
    provider_id: String,
    transport: T,
}

impl WorldRouterVideoAdapter<ReqwestWorldRouterVideoTransport> {
    /// Creates a production WorldRouter video adapter.
    pub(crate) fn new(
        api_token: impl Into<String>,
        submit_url: impl Into<String>,
        provider_id: impl Into<String>,
    ) -> Result<Self> {
        let api_token = api_token.into().trim().to_string();
        if api_token.is_empty() {
            bail!("WorldRouter video API token is required");
        }
        Ok(Self {
            api_token,
            submit_url: submit_url.into().trim_end_matches('/').to_string(),
            provider_id: provider_id.into(),
            transport: ReqwestWorldRouterVideoTransport::default(),
        })
    }
}

impl<T> WorldRouterVideoAdapter<T>
where
    T: WorldRouterVideoTransport,
{
    /// Creates a WorldRouter video adapter with injected transport for tests.
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

    /// Submits a WorldRouter task and persists the queued job.
    pub(crate) fn submit(
        &self,
        service: &MediaGenerationService,
        request: WorldRouterVideoRequest,
        selected_parameters: BTreeMap<String, String>,
        now_ms: u64,
    ) -> Result<MediaJob> {
        request.validate().map_err(|error| {
            anyhow!(
                "provider={} adapter={WORLDROUTER_VIDEO_ADAPTER} phase=validate: {error:#}",
                self.provider_id
            )
        })?;
        let (asset_group_id, asset_urls) = self.prepare_assets(&request)?;
        let body = request.request_body(asset_group_id.as_deref(), &asset_urls)?;
        let response = self
            .transport
            .submit_task(&self.submit_url, &self.api_token, &body)
            .with_context(|| {
                format!(
                    "provider={} adapter={WORLDROUTER_VIDEO_ADAPTER} phase=submit",
                    self.provider_id
                )
            })?;
        let task = WorldRouterSubmitTask::from_value(response).with_context(|| {
            format!(
                "provider={} adapter={WORLDROUTER_VIDEO_ADAPTER} phase=submit",
                self.provider_id
            )
        })?;
        let mut job = MediaJob::new(
            Uuid::new_v4().to_string(),
            MediaKind::Video,
            &self.provider_id,
            request.model.trim(),
            request.prompt.trim(),
            1,
            now_ms,
        );
        job.adapter = Some(WORLDROUTER_VIDEO_ADAPTER.to_string());
        job.parameters = selected_parameters;
        job.provider_job_id = Some(task.id.clone());
        service.save_job(&job)?;
        Ok(job)
    }

    fn prepare_assets(
        &self,
        request: &WorldRouterVideoRequest,
    ) -> Result<(Option<String>, Vec<String>)> {
        if request.image_references.is_empty() {
            return Ok((None, Vec::new()));
        }
        let asset_group_phase = format!(
            "provider={} adapter={WORLDROUTER_VIDEO_ADAPTER} phase=asset_group",
            self.provider_id
        );
        let asset_group_url =
            asset_groups_url(&self.submit_url).with_context(|| asset_group_phase.clone())?;
        let group_response = self
            .transport
            .create_asset_group(&asset_group_url, &self.api_token, &asset_group_body())
            .with_context(|| asset_group_phase.clone())?;
        let group =
            WorldRouterAssetGroup::from_value(group_response).with_context(|| asset_group_phase)?;
        let mut asset_urls = Vec::new();
        for (index, reference) in request.image_references.iter().enumerate() {
            let asset_upload_phase = format!(
                "provider={} adapter={WORLDROUTER_VIDEO_ADAPTER} phase=asset_upload image={}",
                self.provider_id,
                index + 1
            );
            let asset_url = asset_upload_url(&self.submit_url, &group.id)
                .with_context(|| asset_upload_phase.clone())?;
            let response = self
                .transport
                .upload_asset(&asset_url, &self.api_token, &asset_upload_body(index, reference))
                .with_context(|| asset_upload_phase.clone())?;
            let asset = WorldRouterAsset::from_value(response)
                .with_context(|| asset_upload_phase.clone())?;
            asset_urls.push(asset.url);
        }
        Ok((Some(group.id), asset_urls))
    }

    fn fetch_task(&self, job: &MediaJob) -> Result<WorldRouterVideoTask> {
        let response = self
            .transport
            .poll_task(&video_poll_url(&self.submit_url, job)?, &self.api_token)?;
        WorldRouterVideoTask::from_value(response)
    }

    /// Polls a non-terminal WorldRouter job once and persists the resulting state.
    pub(crate) fn poll(
        &self,
        service: &MediaGenerationService,
        job: MediaJob,
        now_ms: u64,
    ) -> Result<MediaJob> {
        if job.status.is_terminal() {
            return Ok(job);
        }
        match self.fetch_task(&job) {
            Ok(task) => self.apply_task(service, job, task, now_ms),
            Err(error) => {
                let diagnostic = error.context(format!(
                    "provider={} adapter={WORLDROUTER_VIDEO_ADAPTER} phase=poll task={}",
                    self.provider_id,
                    job.provider_job_id.as_deref().unwrap_or("unknown")
                ));
                super::video_jobs::record_transient_poll_error(service, job, diagnostic, now_ms)
            }
        }
    }

    /// Polls until a WorldRouter job reaches a terminal status.
    pub(crate) fn poll_until_terminal(
        &self,
        service: &MediaGenerationService,
        job: MediaJob,
        config: WorldRouterVideoPollingConfig,
        sleep: impl FnMut(Duration),
        now_ms: impl FnMut() -> u64,
    ) -> Result<MediaJob> {
        poll_video_until_terminal(job, config, sleep, now_ms, |job, now_ms| {
            self.poll(service, job, now_ms)
        })
    }

    fn apply_task(
        &self,
        service: &MediaGenerationService,
        mut job: MediaJob,
        task: WorldRouterVideoTask,
        now_ms: u64,
    ) -> Result<MediaJob> {
        let status = task.media_status();
        job.remote_status = Some(task.status.clone());
        match status {
            MediaJobStatus::Queued | MediaJobStatus::Running => {
                job.transition(status, now_ms)?;
                service.save_job(&job)?;
                Ok(job)
            }
            MediaJobStatus::Succeeded => {
                let task_id = task.id.clone();
                complete_video_job(
                    service,
                    job,
                    CompletedVideoTask {
                        provider_id: &self.provider_id,
                        task_id: &task.id,
                        remote_status: &task.status,
                        video_url: task.video_url.as_deref(),
                        filename_prefix: "worldrouter-video",
                        missing_url_message:
                            "succeeded WorldRouter video task is missing content.video_url",
                    },
                    now_ms,
                    |url| {
                        self.transport.download_bytes(url).with_context(|| {
                            format!(
                                "provider={} adapter={WORLDROUTER_VIDEO_ADAPTER} phase=download task={task_id}",
                                self.provider_id
                            )
                        })
                    },
                )
            }
            MediaJobStatus::Failed => persist_failed_video_job(
                service,
                job,
                task.error
                    .unwrap_or_else(|| "WorldRouter video task failed".to_string()),
                now_ms,
            ),
            MediaJobStatus::Canceled => {
                job.transition(MediaJobStatus::Canceled, now_ms)?;
                service.save_job(&job)?;
                Ok(job)
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::WorldRouterVideoTransport;
    use anyhow::Result;
    use serde_json::Value;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Scripted transport returning canned WorldRouter responses in tests.
    #[derive(Clone)]
    pub(crate) struct ScriptedTransport {
        pub(crate) asset_group: Value,
        pub(crate) assets: Rc<RefCell<Vec<Value>>>,
        pub(crate) submit: Value,
        pub(crate) polls: Rc<RefCell<Vec<Value>>>,
        pub(crate) downloads: Rc<RefCell<Vec<Vec<u8>>>>,
        pub(crate) seen: Rc<RefCell<Vec<String>>>,
    }

    impl WorldRouterVideoTransport for ScriptedTransport {
        fn create_asset_group(
            &self,
            _url: &str,
            _api_token: &str,
            _body: &Value,
        ) -> Result<Value> {
            self.seen.borrow_mut().push("asset-group".to_string());
            Ok(self.asset_group.clone())
        }

        fn upload_asset(&self, _url: &str, _api_token: &str, _body: &Value) -> Result<Value> {
            let index = self
                .seen
                .borrow()
                .iter()
                .filter(|event| event.starts_with("asset-upload"))
                .count();
            self.seen
                .borrow_mut()
                .push(format!("asset-upload:{index}"));
            Ok(self.assets.borrow_mut().remove(0))
        }

        fn submit_task(&self, _url: &str, _api_token: &str, _body: &Value) -> Result<Value> {
            self.seen.borrow_mut().push("submit".to_string());
            Ok(self.submit.clone())
        }

        fn poll_task(&self, _url: &str, _api_token: &str) -> Result<Value> {
            self.seen.borrow_mut().push("poll".to_string());
            Ok(self.polls.borrow_mut().remove(0))
        }

        fn download_bytes(&self, _url: &str) -> Result<Vec<u8>> {
            self.seen.borrow_mut().push("download".to_string());
            Ok(self.downloads.borrow_mut().remove(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::ScriptedTransport;
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn submit_uploads_assets_before_creating_video_task() {
        let temp = tempfile::tempdir().unwrap();
        let service = MediaGenerationService::new(temp.path());
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let transport = ScriptedTransport {
            asset_group: json!({"id": "group-1"}),
            assets: std::rc::Rc::new(std::cell::RefCell::new(vec![json!({
                "url": "asset://asset-1"
            })])),
            submit: json!({"id": "task-123", "requestId": "req-123"}),
            polls: std::rc::Rc::new(std::cell::RefCell::new(vec![json!({
                "id": "task-123",
                "status": "succeeded",
                "content": { "video_url": "https://media.example.com/out.mp4" }
            })])),
            downloads: std::rc::Rc::new(std::cell::RefCell::new(vec![b"mp4-bytes".to_vec()])),
            seen: seen.clone(),
        };
        let adapter = WorldRouterVideoAdapter::with_transport(
            "token",
            "https://inference-api.worldrouter.ai/api/v3/contents/generations/tasks",
            "worldrouter",
            transport,
        );
        let request = WorldRouterVideoRequest {
            model: "seedance-2.0-fast".to_string(),
            prompt: "animate image 1".to_string(),
            image_references: vec!["https://example.com/ref.png".to_string()],
            params: params(&[("resolution", "480p"), ("duration", "5")]),
        };

        let job = adapter
            .submit(
                &service,
                request,
                params(&[("resolution", "480p"), ("duration", "5")]),
                1,
            )
            .expect("submit");
        let job = adapter
            .poll_until_terminal(&service, job, VideoPollingConfig::default(), |_| {}, || 2)
            .expect("poll");

        assert_eq!(job.status, MediaJobStatus::Succeeded);
        assert_eq!(job.artifact_ids.len(), 1);
        let seen = seen.borrow();
        assert!(seen.iter().any(|event| event.contains("asset-group")));
        assert!(seen.iter().any(|event| event.contains("asset-upload:0")));
        assert!(seen.iter().any(|event| event.contains("submit")));
    }

    #[test]
    fn rejects_invalid_image_reference_before_asset_group_request() {
        let temp = tempfile::tempdir().unwrap();
        let service = MediaGenerationService::new(temp.path());
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let transport = ScriptedTransport {
            asset_group: json!({"id": "group-1"}),
            assets: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            submit: json!({"id": "task-123", "requestId": "req-123"}),
            polls: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            downloads: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            seen: seen.clone(),
        };
        let adapter = WorldRouterVideoAdapter::with_transport(
            "token",
            "https://inference-api.worldrouter.ai/api/v3/contents/generations/tasks",
            "worldrouter",
            transport,
        );
        let request = WorldRouterVideoRequest {
            model: "seedance-2.0-fast".to_string(),
            prompt: "animate image 1".to_string(),
            image_references: vec!["file:///tmp/ref.png".to_string()],
            params: params(&[("resolution", "480p"), ("duration", "5")]),
        };

        let error = adapter
            .submit(
                &service,
                request,
                params(&[("resolution", "480p"), ("duration", "5")]),
                1,
            )
            .unwrap_err()
            .to_string();

        assert!(error.contains("phase=validate"), "{error}");
        assert!(error.contains("image reference 0"), "{error}");
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn builds_text_to_video_request_body() {
        let request = WorldRouterVideoRequest {
            model: "seedance-2.0-fast".to_string(),
            prompt: "a robot battle".to_string(),
            image_references: Vec::new(),
            params: params(&[("resolution", "480p"), ("duration", "5")]),
        };

        assert_eq!(
            request.request_body(None, &[]).expect("body"),
            json!({
                "model": "seedance-2.0-fast",
                "content": [
                    { "type": "text", "text": "a robot battle" }
                ],
                "resolution": "480p",
                "duration": 5
            })
        );
    }

    #[test]
    fn builds_image_to_video_request_body_with_asset_references() {
        let request = WorldRouterVideoRequest {
            model: "seedance-2.0-fast".to_string(),
            prompt: "animate image 1".to_string(),
            image_references: vec!["https://example.com/ref.png".to_string()],
            params: params(&[("resolution", "720p"), ("duration", "5")]),
        };

        assert_eq!(
            request
                .request_body(Some("group-1"), &["asset://asset-1".to_string()])
                .expect("body"),
            json!({
                "model": "seedance-2.0-fast",
                "asset_group_id": "group-1",
                "content": [
                    { "type": "text", "text": "animate image 1" },
                    {
                        "type": "image_url",
                        "role": "reference_image",
                        "image_url": { "url": "asset://asset-1" }
                    }
                ],
                "resolution": "720p",
                "duration": 5
            })
        );
    }

    #[test]
    fn rejects_worldrouter_asset_references_without_group_context() {
        let error = validate_image_reference("asset://asset-1", 0)
            .unwrap_err()
            .to_string();
        assert!(error.contains("image reference 0"), "{error}");
        assert!(error.contains("https://"), "{error}");
    }

    #[test]
    fn parses_submit_response_without_status() {
        let task = WorldRouterSubmitTask::from_value(json!({
            "id": "task-123",
            "requestId": "req-123"
        }))
        .expect("submit task");

        assert_eq!(task.id, "task-123");
        assert_eq!(task.request_id.as_deref(), Some("req-123"));
    }

    #[test]
    fn parses_succeeded_poll_response_video_url() {
        let task = WorldRouterVideoTask::from_value(json!({
            "id": "task-123",
            "status": "succeeded",
            "content": { "video_url": "https://media.example.com/out.mp4" }
        }))
        .expect("poll task");

        assert_eq!(task.id, "task-123");
        assert_eq!(task.media_status(), MediaJobStatus::Succeeded);
        assert_eq!(
            task.video_url.as_deref(),
            Some("https://media.example.com/out.mp4")
        );
    }

    #[test]
    fn parses_asset_group_response() {
        let group = WorldRouterAssetGroup::from_value(json!({
            "id": "group-1",
            "requestId": "req-1"
        }))
        .expect("asset group");
        assert_eq!(group.id, "group-1");
    }

    #[test]
    fn parses_asset_upload_response_asset_url() {
        let asset = WorldRouterAsset::from_value(json!({
            "id": "asset-1",
            "url": "asset://asset-1",
            "source_url": "https://example.com/ref.png"
        }))
        .expect("asset");
        assert_eq!(asset.url, "asset://asset-1");
    }
}
