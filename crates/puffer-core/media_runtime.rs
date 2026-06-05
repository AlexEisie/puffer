use crate::runtime::media::openai_image::{OpenAIImagesAdapter, OpenAIImagesGenerationRequest};
use crate::runtime::media::resolver::{resolve_media_capabilities, MediaDiscoveryCache};
use crate::runtime::media::{MediaGenerationService, MediaJobStatus, MediaKind};
use anyhow::Result;
use puffer_provider_registry::{AuthStore, MediaOperation, ProviderRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Describes one exact media capability suitable for client display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCapabilityView {
    pub provider_id: String,
    pub model_id: String,
    pub kind: String,
    pub operations: Vec<String>,
    pub supports_async: bool,
    pub supports_streaming: bool,
    pub parameter_values: Value,
    pub status: String,
    pub source: String,
    pub reason: Option<String>,
    pub checked_at_ms: u64,
}

/// Carries an exact image generation request from UI or tool configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactImageGenerationRequest {
    pub provider_id: String,
    pub model_id: String,
    pub prompt: String,
    pub size: String,
    pub quality: String,
    pub output_format: String,
}

/// Carries the persisted job and artifact produced by exact image generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExactImageGenerationResult {
    pub job_id: String,
    pub artifact_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: String,
    pub path: PathBuf,
}

/// Lists exact media capabilities from provider media descriptors.
pub fn list_exact_media_capabilities(
    registry: &ProviderRegistry,
    auth_store: &AuthStore,
    kind_filter: Option<&str>,
) -> Vec<MediaCapabilityView> {
    let checked_at_ms = now_ms();
    let discovery_cache = MediaDiscoveryCache::default();
    let mut capabilities = Vec::new();
    if kind_filter_matches(kind_filter, "image") {
        capabilities.extend(resolve_media_capabilities(
            registry,
            auth_store,
            MediaKind::Image,
            MediaOperation::Generate,
            checked_at_ms,
            &discovery_cache,
        ));
    }
    if kind_filter_matches(kind_filter, "video") {
        capabilities.extend(resolve_media_capabilities(
            registry,
            auth_store,
            MediaKind::Video,
            MediaOperation::Generate,
            checked_at_ms,
            &discovery_cache,
        ));
    }
    capabilities
        .into_iter()
        .map(MediaCapabilityView::from)
        .collect()
}

/// Generates one exact image and persists its media job and artifact sidecars.
pub fn generate_exact_image(
    registry: &ProviderRegistry,
    auth_store: &AuthStore,
    workspace_root: &Path,
    request: ExactImageGenerationRequest,
) -> Result<ExactImageGenerationResult> {
    let result = OpenAIImagesAdapter::new()?.execute(
        registry,
        auth_store,
        &MediaGenerationService::new(workspace_root),
        OpenAIImagesGenerationRequest {
            provider_id: request.provider_id,
            model_id: request.model_id,
            prompt: request.prompt,
            size: request.size,
            quality: request.quality,
            output_format: request.output_format,
        },
    )?;
    Ok(ExactImageGenerationResult {
        job_id: result.job.id,
        artifact_id: result.artifact.id,
        provider_id: result.job.provider_id,
        model_id: result.job.model_id,
        status: media_job_status_name(result.job.status).to_string(),
        path: result.artifact.path,
    })
}

impl From<crate::runtime::media::capabilities::MediaCapability> for MediaCapabilityView {
    fn from(capability: crate::runtime::media::capabilities::MediaCapability) -> Self {
        Self {
            provider_id: capability.provider_id,
            model_id: capability.model_id,
            kind: media_kind_name(capability.kind).to_string(),
            operations: capability.operations,
            supports_async: capability.supports_async,
            supports_streaming: capability.supports_streaming,
            parameter_values: client_parameter_values(capability.parameter_values),
            status: capability.status,
            source: capability.source,
            reason: capability.reason,
            checked_at_ms: capability.checked_at_ms,
        }
    }
}

fn kind_filter_matches(kind_filter: Option<&str>, kind: &str) -> bool {
    kind_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none_or(|value| value == kind)
}

fn client_parameter_values(mut value: Value) -> Value {
    let Some(object) = value.as_object_mut() else {
        return value;
    };
    if let Some(output_format) = object.remove("output_format") {
        object.insert("outputFormat".to_string(), output_format);
    }
    value
}

fn media_kind_name(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "image",
        MediaKind::Video => "video",
    }
}

fn media_job_status_name(status: MediaJobStatus) -> &'static str {
    match status {
        MediaJobStatus::Queued => "queued",
        MediaJobStatus::Running => "running",
        MediaJobStatus::Succeeded => "succeeded",
        MediaJobStatus::Failed => "failed",
        MediaJobStatus::Canceled => "canceled",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
