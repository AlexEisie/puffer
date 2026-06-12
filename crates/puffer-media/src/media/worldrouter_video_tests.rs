use super::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug, PartialEq)]
struct RecordedRequest {
    operation: &'static str,
    url: String,
    body: Option<Value>,
}

#[derive(Clone)]
struct ScriptedTransport {
    asset_group: Value,
    assets: Rc<RefCell<Vec<Value>>>,
    submit: Value,
    polls: Rc<RefCell<Vec<Value>>>,
    downloads: Rc<RefCell<Vec<Vec<u8>>>>,
    requests: Rc<RefCell<Vec<RecordedRequest>>>,
}

impl ScriptedTransport {
    fn record(&self, operation: &'static str, url: &str, body: Option<&Value>) {
        self.requests.borrow_mut().push(RecordedRequest {
            operation,
            url: url.to_string(),
            body: body.cloned(),
        });
    }
}

impl WorldRouterVideoTransport for ScriptedTransport {
    fn create_asset_group(&self, url: &str, _api_token: &str, body: &Value) -> Result<Value> {
        self.record("asset_group", url, Some(body));
        Ok(self.asset_group.clone())
    }

    fn upload_asset(&self, url: &str, _api_token: &str, body: &Value) -> Result<Value> {
        self.record("asset_upload", url, Some(body));
        pop_json(&self.assets, "asset")
    }

    fn submit_task(&self, url: &str, _api_token: &str, body: &Value) -> Result<Value> {
        self.record("submit", url, Some(body));
        Ok(self.submit.clone())
    }

    fn poll_task(&self, url: &str, _api_token: &str) -> Result<Value> {
        self.record("poll", url, None);
        pop_json(&self.polls, "poll")
    }

    fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.record("download", url, None);
        pop_bytes(&self.downloads)
    }
}

fn pop_json(queue: &Rc<RefCell<Vec<Value>>>, label: &str) -> Result<Value> {
    let mut queue = queue.borrow_mut();
    if queue.is_empty() {
        bail!("missing scripted {label} response");
    }
    Ok(queue.remove(0))
}

fn pop_bytes(queue: &Rc<RefCell<Vec<Vec<u8>>>>) -> Result<Vec<u8>> {
    let mut queue = queue.borrow_mut();
    if queue.is_empty() {
        bail!("missing scripted download response");
    }
    Ok(queue.remove(0))
}

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn test_adapter(transport: ScriptedTransport) -> WorldRouterVideoAdapter<ScriptedTransport> {
    WorldRouterVideoAdapter::with_transport(
        "token",
        "https://inference-api.worldrouter.ai/api/v3/contents/generations/tasks",
        "worldrouter",
        transport,
    )
}

#[test]
fn submit_uploads_assets_before_creating_video_task() {
    let temp = tempfile::tempdir().unwrap();
    let service = MediaGenerationService::new(temp.path());
    let requests = Rc::new(RefCell::new(Vec::new()));
    let adapter = test_adapter(ScriptedTransport {
        asset_group: json!({"id": "group-1"}),
        assets: Rc::new(RefCell::new(vec![json!({"url": "asset://asset-1"})])),
        submit: json!({"id": "task-123", "requestId": "req-123"}),
        polls: Rc::new(RefCell::new(vec![json!({
            "id": "task-123",
            "status": "succeeded",
            "content": { "video_url": "https://media.example.com/out.mp4" }
        })])),
        downloads: Rc::new(RefCell::new(vec![b"mp4-bytes".to_vec()])),
        requests: requests.clone(),
    });
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
    assert_eq!(
        *requests.borrow(),
        vec![
            RecordedRequest {
                operation: "asset_group",
                url: "https://inference-api.worldrouter.ai/v1/asset-groups".to_string(),
                body: Some(json!({
                    "name": "puffer-seedance-video",
                    "description": "reference assets for one Puffer Seedance video generation"
                })),
            },
            RecordedRequest {
                operation: "asset_upload",
                url: "https://inference-api.worldrouter.ai/v1/asset-groups/group-1/assets"
                    .to_string(),
                body: Some(json!({
                    "name": "reference-image-1",
                    "description": "Puffer Seedance reference image 1",
                    "type": "image",
                    "url": "https://example.com/ref.png"
                })),
            },
            RecordedRequest {
                operation: "submit",
                url: "https://inference-api.worldrouter.ai/api/v3/contents/generations/tasks"
                    .to_string(),
                body: Some(json!({
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
                    "resolution": "480p",
                    "duration": 5
                })),
            },
            RecordedRequest {
                operation: "poll",
                url: "https://inference-api.worldrouter.ai/api/v3/contents/generations/tasks/task-123"
                    .to_string(),
                body: None,
            },
            RecordedRequest {
                operation: "download",
                url: "https://media.example.com/out.mp4".to_string(),
                body: None,
            },
        ]
    );
}

#[test]
fn rejects_invalid_image_reference_before_asset_group_request() {
    let temp = tempfile::tempdir().unwrap();
    let service = MediaGenerationService::new(temp.path());
    let requests = Rc::new(RefCell::new(Vec::new()));
    let adapter = test_adapter(ScriptedTransport {
        asset_group: json!({"id": "group-1"}),
        assets: Rc::new(RefCell::new(Vec::new())),
        submit: json!({"id": "task-123", "requestId": "req-123"}),
        polls: Rc::new(RefCell::new(Vec::new())),
        downloads: Rc::new(RefCell::new(Vec::new())),
        requests: requests.clone(),
    });
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
    assert!(requests.borrow().is_empty());
}

#[test]
fn rejects_non_public_https_image_reference_before_asset_group_request() {
    for reference in [
        "https://localhost/ref.png",
        "https://localhost./ref.png",
        "https://asset.localhost/ref.png",
        "https://192.168.1.10/ref.png",
        "https://[::1]/ref.png",
        "https://[::ffff:127.0.0.1]/ref.png",
    ] {
        let request = WorldRouterVideoRequest {
            model: "seedance-2.0-fast".to_string(),
            prompt: "animate image 1".to_string(),
            image_references: vec![reference.to_string()],
            params: params(&[("resolution", "480p"), ("duration", "5")]),
        };

        let error = request.request_body(None, &[]).unwrap_err().to_string();

        assert!(error.contains("image reference 0"), "{reference}: {error}");
        assert!(
            error.contains("public https:// URL"),
            "{reference}: {error}"
        );
    }
}

#[test]
fn request_body_rejects_non_asset_uploaded_url() {
    let request = WorldRouterVideoRequest {
        model: "seedance-2.0-fast".to_string(),
        prompt: "animate image 1".to_string(),
        image_references: vec!["https://example.com/ref.png".to_string()],
        params: params(&[("resolution", "720p"), ("duration", "5")]),
    };

    let error = request
        .request_body(
            Some("group-1"),
            &["https://example.com/ref.png".to_string()],
        )
        .unwrap_err()
        .to_string();

    assert!(error.contains("asset://"), "{error}");
}

#[test]
fn asset_upload_url_encodes_group_id_as_path_segment() {
    assert_eq!(
        asset_upload_url(
            "https://inference-api.worldrouter.ai/api/v3/contents/generations/tasks",
            "group/1"
        )
        .expect("asset upload url"),
        "https://inference-api.worldrouter.ai/v1/asset-groups/group%2F1/assets"
    );
}

#[test]
fn succeeded_poll_without_video_url_marks_job_failed() {
    let temp = tempfile::tempdir().unwrap();
    let service = MediaGenerationService::new(temp.path());
    let adapter = test_adapter(ScriptedTransport {
        asset_group: json!({"id": "group-1"}),
        assets: Rc::new(RefCell::new(Vec::new())),
        submit: json!({"id": "task-123", "requestId": "req-123"}),
        polls: Rc::new(RefCell::new(vec![json!({
            "id": "task-123",
            "status": "succeeded",
            "content": {}
        })])),
        downloads: Rc::new(RefCell::new(Vec::new())),
        requests: Rc::new(RefCell::new(Vec::new())),
    });
    let request = WorldRouterVideoRequest {
        model: "seedance-2.0-fast".to_string(),
        prompt: "a robot battle".to_string(),
        image_references: Vec::new(),
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

    assert_eq!(job.status, MediaJobStatus::Failed);
    assert_eq!(job.error.as_deref(), Some(MISSING_VIDEO_URL_MESSAGE));
    assert!(job.artifact_ids.is_empty());
}

#[test]
fn failed_poll_persists_remote_failure_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let service = MediaGenerationService::new(temp.path());
    let adapter = test_adapter(ScriptedTransport {
        asset_group: json!({"id": "group-1"}),
        assets: Rc::new(RefCell::new(Vec::new())),
        submit: json!({"id": "task-123", "requestId": "req-123"}),
        polls: Rc::new(RefCell::new(vec![json!({
            "id": "task-123",
            "status": "failed",
            "error": {
                "message": "The service encountered an unexpected internal error."
            }
        })])),
        downloads: Rc::new(RefCell::new(Vec::new())),
        requests: Rc::new(RefCell::new(Vec::new())),
    });
    let request = WorldRouterVideoRequest {
        model: "seedance-2.0-fast".to_string(),
        prompt: "a robot battle".to_string(),
        image_references: Vec::new(),
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

    assert_eq!(job.status, MediaJobStatus::Failed);
    assert_eq!(job.provider_job_id.as_deref(), Some("task-123"));
    assert_eq!(job.remote_status.as_deref(), Some("failed"));
    assert_eq!(
        job.error.as_deref(),
        Some("The service encountered an unexpected internal error.")
    );
    assert!(job.artifact_ids.is_empty());
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
