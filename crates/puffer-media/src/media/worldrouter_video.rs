use super::MediaJobStatus;
use super::video_jobs::map_video_task_status;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Number, Value};
use std::collections::BTreeMap;

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

#[cfg(test)]
mod tests {
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
