use super::artifacts::MediaArtifact;
use super::jobs::{MediaJob, MediaJobStatus};
use super::resolver::{
    validate_image_generate_selection, ImageGenerationSelection, MediaDiscoveryCache,
};
use super::{MediaGenerationService, MediaKind};
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use puffer_provider_registry::{
    canonical_provider_id, AuthStore, MediaExecutionKind, ProviderDescriptor, ProviderRegistry,
    StoredCredential,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const DEFAULT_IMAGE_REQUEST_TIMEOUT_MS: u64 = 300_000;

/// Request shape for OpenAI image generation after media settings resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenAIImageRequest {
    model: String,
    prompt: String,
    size: String,
    quality: String,
    output_format: String,
}

impl OpenAIImageRequest {
    fn new(
        model: impl Into<String>,
        prompt: impl Into<String>,
        size: impl Into<String>,
        quality: impl Into<String>,
        output_format: impl Into<String>,
    ) -> Self {
        Self {
            model: model.into(),
            prompt: prompt.into(),
            size: size.into(),
            quality: quality.into(),
            output_format: output_format.into(),
        }
    }

    fn to_body(&self) -> Value {
        json!({
            "model": self.model,
            "prompt": self.prompt,
            "size": self.size,
            "quality": self.quality,
            "output_format": self.output_format,
            "n": 1
        })
    }
}

/// Carries an exact OpenAI Images-compatible generation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAIImagesGenerationRequest {
    pub(crate) provider_id: String,
    pub(crate) model_id: String,
    pub(crate) prompt: String,
    pub(crate) size: String,
    pub(crate) quality: String,
    pub(crate) output_format: String,
}

/// Carries persisted media records created by the OpenAI Images adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAIImagesGenerationResult {
    pub(crate) job: MediaJob,
    pub(crate) artifact: MediaArtifact,
}

/// Executes descriptor-driven OpenAI Images-compatible generation.
#[derive(Debug, Clone)]
pub(crate) struct OpenAIImagesAdapter {
    client: Client,
}

impl OpenAIImagesAdapter {
    /// Creates an adapter with a default blocking HTTP client.
    pub(crate) fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(DEFAULT_IMAGE_REQUEST_TIMEOUT_MS))
            .build()
            .context("build image generation HTTP client")?;
        Ok(Self { client })
    }

    /// Executes an exact image generation request and persists job/artifact sidecars.
    pub(crate) fn execute(
        &self,
        registry: &ProviderRegistry,
        auth_store: &AuthStore,
        service: &MediaGenerationService,
        request: OpenAIImagesGenerationRequest,
    ) -> Result<OpenAIImagesGenerationResult> {
        validate_image_generate_selection(
            registry,
            auth_store,
            &ImageGenerationSelection {
                provider_id: &request.provider_id,
                model_id: &request.model_id,
                size: &request.size,
                quality: &request.quality,
                output_format: &request.output_format,
            },
            now_ms(),
            &MediaDiscoveryCache::default(),
        )?;

        let provider = registry.provider(&request.provider_id).with_context(|| {
            format!(
                "selected image model unavailable: {}/{}",
                request.provider_id, request.model_id
            )
        })?;
        let execution = provider
            .media
            .as_ref()
            .and_then(|media| media.image.as_ref())
            .and_then(|image| image.execution.as_ref())
            .with_context(|| {
                format!(
                    "selected image model unavailable: {}/{}",
                    request.provider_id, request.model_id
                )
            })?;
        if !matches!(execution.adapter, MediaExecutionKind::OpenAiImages) {
            bail!(
                "image media adapter unavailable for {:?}",
                execution.adapter
            );
        }

        let job_id = Uuid::new_v4().to_string();
        let artifact_id = Uuid::new_v4().to_string();
        let created_at_ms = now_ms();
        let mut job = MediaJob::new(
            job_id.clone(),
            MediaKind::Image,
            request.provider_id.clone(),
            request.model_id.clone(),
            request.prompt.clone(),
            created_at_ms,
        );
        service.save_job(&job)?;
        job.transition(MediaJobStatus::Running, now_ms())?;
        service.save_job(&job)?;

        let output = match self.request_image(provider, auth_store, &request, &execution.path) {
            Ok(output) => output,
            Err(error) => {
                job.error = Some(format!("{error:#}"));
                job.transition(MediaJobStatus::Failed, now_ms())?;
                service.save_job(&job)?;
                return Err(error);
            }
        };

        let filename = format!(
            "image.{}",
            extension_for_output_format(&request.output_format)
        );
        let artifact_path = service.write_artifact_bytes(&artifact_id, &filename, &output.bytes)?;
        let artifact = MediaArtifact {
            id: artifact_id.clone(),
            job_id: job_id.clone(),
            kind: MediaKind::Image,
            path: artifact_path.clone(),
            mime_type: mime_type_for_output_format(&request.output_format).to_string(),
            byte_count: output.bytes.len() as u64,
            metadata: artifact_metadata(&request, &artifact_path, &output, created_at_ms),
            created_at_ms,
        };
        service.save_artifact(&artifact)?;
        job.attach_artifact(artifact_id, now_ms());
        job.transition(MediaJobStatus::Succeeded, now_ms())?;
        service.save_job(&job)?;

        Ok(OpenAIImagesGenerationResult { job, artifact })
    }

    fn request_image(
        &self,
        provider: &ProviderDescriptor,
        auth_store: &AuthStore,
        request: &OpenAIImagesGenerationRequest,
        execution_path: &str,
    ) -> Result<ImageOutput> {
        let url = provider_execution_url(provider, execution_path)?;
        let secrets = provider_error_secrets(provider, auth_store);
        let mut http = self.client.post(url).json(
            &OpenAIImageRequest::new(
                &request.model_id,
                &request.prompt,
                &request.size,
                &request.quality,
                &request.output_format,
            )
            .to_body(),
        );
        for (name, value) in &provider.headers {
            http = http.header(name.as_str(), value.as_str());
        }
        if let Some(token) = bearer_token(provider, auth_store)? {
            http = http.bearer_auth(token);
        }
        let response = http
            .send()
            .map_err(|error| anyhow!("{}", redact_secrets(&error.to_string(), &secrets)))
            .context("send image generation request")?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|error| anyhow!("{}", redact_secrets(&error.to_string(), &secrets)))
            .context("read image generation response")?;
        if !status.is_success() {
            bail!(
                "image generation failed with status {}: {}",
                status.as_u16(),
                redact_secrets(&body, &secrets)
            );
        }
        let value: Value =
            serde_json::from_str(&body).context("parse image generation response")?;
        image_output_from_response(&self.client, &value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageOutput {
    bytes: Vec<u8>,
    revised_prompt: Option<String>,
    remote_source_url: Option<String>,
}

fn image_output_from_response(client: &Client, value: &Value) -> Result<ImageOutput> {
    let Some(first) = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    else {
        bail!("image generation response did not contain an image");
    };
    let revised_prompt = first
        .get("revised_prompt")
        .or_else(|| first.get("revisedPrompt"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    if let Some(encoded) = first.get("b64_json").and_then(Value::as_str) {
        let bytes = BASE64_STANDARD
            .decode(encoded.trim())
            .context("decode image b64_json")?;
        return Ok(ImageOutput {
            bytes,
            revised_prompt,
            remote_source_url: None,
        });
    }
    if let Some(url) = first.get("url").and_then(Value::as_str) {
        let bytes = download_image_url(client, url)?;
        return Ok(ImageOutput {
            bytes,
            revised_prompt,
            remote_source_url: Some(url.to_string()),
        });
    }
    bail!("image generation response did not contain an image")
}

fn download_image_url(client: &Client, url: &str) -> Result<Vec<u8>> {
    let parsed = reqwest::Url::parse(url).context("image response URL must be absolute")?;
    match parsed.scheme() {
        "https" => {}
        "http" if url_host_is_loopback(&parsed) => {}
        other => bail!("unsupported image response URL scheme `{other}`"),
    }
    let response = client
        .get(parsed)
        .send()
        .context("download generated image")?;
    let status = response.status();
    if !status.is_success() {
        bail!(
            "download generated image failed with status {}",
            status.as_u16()
        );
    }
    Ok(response
        .bytes()
        .context("read generated image bytes")?
        .to_vec())
}

fn url_host_is_loopback(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn provider_execution_url(
    provider: &ProviderDescriptor,
    execution_path: &str,
) -> Result<reqwest::Url> {
    let base = format!("{}/", provider.base_url.trim_end_matches('/'));
    let path = execution_path.trim_start_matches('/');
    let mut url = reqwest::Url::parse(&base)
        .and_then(|base| base.join(path))
        .with_context(|| {
            format!(
                "build image generation URL from {} and {}",
                provider.base_url, execution_path
            )
        })?;
    if !provider.query_params.is_empty() {
        let mut query = url.query_pairs_mut();
        for (key, value) in &provider.query_params {
            query.append_pair(key, value);
        }
    }
    Ok(url)
}

fn bearer_token(provider: &ProviderDescriptor, auth_store: &AuthStore) -> Result<Option<String>> {
    if provider.auth_modes.is_empty() {
        return Ok(None);
    }
    let Some(credential) = provider_credential(provider, auth_store) else {
        bail!(
            "missing credentials configured for provider {}",
            provider.id
        );
    };
    match credential {
        StoredCredential::ApiKey { key } => non_empty_token(key, &provider.id).map(Some),
        StoredCredential::OAuth(credential) => {
            non_empty_token(&credential.access_token, &provider.id).map(Some)
        }
    }
}

fn provider_credential<'a>(
    provider: &ProviderDescriptor,
    auth_store: &'a AuthStore,
) -> Option<&'a StoredCredential> {
    let canonical = canonical_provider_id(&provider.id);
    auth_store
        .get(&provider.id)
        .or_else(|| {
            (canonical != provider.id.as_str())
                .then(|| auth_store.get(&canonical))
                .flatten()
        })
        .or_else(|| {
            (canonical == "openai")
                .then(|| auth_store.get("codex"))
                .flatten()
        })
}

fn non_empty_token(value: &str, provider_id: &str) -> Result<String> {
    let token = value.trim();
    if token.is_empty() {
        bail!("empty credentials configured for provider {provider_id}");
    }
    Ok(token.to_string())
}

fn artifact_metadata(
    request: &OpenAIImagesGenerationRequest,
    path: &std::path::Path,
    output: &ImageOutput,
    created_at_ms: u64,
) -> Value {
    let mut metadata = json!({
        "providerId": request.provider_id,
        "modelId": request.model_id,
        "adapter": "openai_images",
        "prompt": request.prompt,
        "size": request.size,
        "quality": request.quality,
        "outputFormat": request.output_format,
        "mimeType": mime_type_for_output_format(&request.output_format),
        "localPath": path,
        "byteCount": output.bytes.len() as u64,
        "createdAtMs": created_at_ms,
    });
    if let Some(revised_prompt) = &output.revised_prompt {
        metadata["revisedPrompt"] = json!(revised_prompt);
    }
    if let Some(remote_source_url) = &output.remote_source_url {
        metadata["remoteSourceUrl"] = json!(remote_source_url);
    }
    metadata
}

fn provider_error_secrets(provider: &ProviderDescriptor, auth_store: &AuthStore) -> Vec<String> {
    let mut secrets = Vec::new();
    if let Some(credential) = provider_credential(provider, auth_store) {
        match credential {
            StoredCredential::ApiKey { key } => secrets.push(key.clone()),
            StoredCredential::OAuth(credential) => {
                secrets.push(credential.access_token.clone());
                secrets.push(credential.refresh_token.clone());
            }
        }
    }
    secrets.extend(provider.headers.values().cloned());
    secrets.extend(provider.query_params.values().cloned());
    secrets
        .into_iter()
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty())
        .collect()
}

fn redact_secrets(text: &str, secrets: &[String]) -> String {
    secrets.iter().fold(text.to_string(), |redacted, secret| {
        redacted.replace(secret, "[redacted]")
    })
}

fn mime_type_for_output_format(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

fn extension_for_output_format(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "jpeg",
        "webp" => "webp",
        _ => "png",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::media::MediaGenerationService;
    use indexmap::IndexMap;
    use puffer_provider_registry::{
        AuthMode, AuthStore, ImageMediaDescriptor, MediaExecutionDescriptor, MediaExecutionKind,
        MediaImageParameters, MediaModelDescriptor, MediaOperation, ModelDescriptor,
        ProviderDescriptor, ProviderMediaDescriptor, ProviderRegistry,
    };
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use tempfile::tempdir;

    fn registry_with_provider(base_url: String) -> ProviderRegistry {
        registry_with_provider_id("exact-provider", base_url)
    }

    fn registry_with_provider_id(provider_id: &str, base_url: String) -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        registry.register(ProviderDescriptor {
            id: provider_id.to_string(),
            display_name: "Exact Provider".to_string(),
            base_url,
            default_api: "openai-responses".to_string(),
            auth_modes: vec![AuthMode::ApiKey],
            headers: IndexMap::from([("x-provider-header".to_string(), "present".to_string())]),
            query_params: IndexMap::from([("api-version".to_string(), "2026-06-05".to_string())]),
            chat_completions_path: None,
            discovery: None,
            media: Some(ProviderMediaDescriptor {
                image: Some(ImageMediaDescriptor {
                    discovery: None,
                    execution: Some(MediaExecutionDescriptor {
                        adapter: MediaExecutionKind::OpenAiImages,
                        path: "/custom/images".to_string(),
                    }),
                    models: vec![MediaModelDescriptor {
                        id: "exact-image-model".to_string(),
                        display_name: Some("Exact Image Model".to_string()),
                        operations: vec![MediaOperation::Generate],
                        parameters: MediaImageParameters::new(
                            vec!["1024x1024".to_string()],
                            vec!["auto".to_string()],
                            vec!["png".to_string()],
                        ),
                    }],
                }),
            }),
            models: Vec::<ModelDescriptor>::new(),
        });
        registry
    }

    fn auth_store() -> AuthStore {
        let mut auth = AuthStore::default();
        auth.set_api_key("exact-provider", "sk-secret");
        auth
    }

    fn codex_auth_store() -> AuthStore {
        let mut auth = AuthStore::default();
        auth.set_api_key("codex", "sk-codex-secret");
        auth
    }

    fn request() -> OpenAIImagesGenerationRequest {
        OpenAIImagesGenerationRequest {
            provider_id: "exact-provider".to_string(),
            model_id: "exact-image-model".to_string(),
            prompt: "draw a precise icon".to_string(),
            size: "1024x1024".to_string(),
            quality: "auto".to_string(),
            output_format: "png".to_string(),
        }
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        let mut buffer = [0_u8; 8192];
        let size = stream.read(&mut buffer).expect("read request");
        String::from_utf8_lossy(&buffer[..size]).to_string()
    }

    #[test]
    fn request_body_uses_selected_model_and_descriptor_path() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let request_text = read_http_request(&mut stream);
            let body = json!({
                "data": [{"b64_json": "aW1hZ2UtYnl0ZXM="}]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("response");
            request_text
        });
        let registry = registry_with_provider(format!("http://{address}"));
        let service_dir = tempdir().expect("tempdir");

        let result = OpenAIImagesAdapter::new()
            .expect("adapter")
            .execute(
                &registry,
                &auth_store(),
                &MediaGenerationService::new(service_dir.path()),
                request(),
            )
            .expect("generation succeeds");

        let request_text = server.join().expect("server");
        assert!(request_text.starts_with("POST /custom/images?api-version=2026-06-05 HTTP/1.1"));
        assert!(request_text.contains("authorization: Bearer sk-secret"));
        assert!(request_text.contains("x-provider-header: present"));
        assert!(request_text.contains("\"model\":\"exact-image-model\""));
        assert_eq!(
            std::fs::read(&result.artifact.path).unwrap(),
            b"image-bytes"
        );
        assert_eq!(result.artifact.metadata["adapter"], "openai_images");
    }

    #[test]
    fn url_response_is_downloaded_before_success() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("generation request");
            let generation_request = read_http_request(&mut stream);
            let body = json!({
                "data": [{
                    "url": format!("http://{address}/generated.png"),
                    "revised_prompt": "draw a more precise icon"
                }]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("generation response");

            let (mut stream, _) = listener.accept().expect("download request");
            let download_request = read_http_request(&mut stream);
            let response = "HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: 12\r\nconnection: close\r\n\r\ndownloaded!!";
            stream
                .write_all(response.as_bytes())
                .expect("download response");
            (generation_request, download_request)
        });
        let registry = registry_with_provider(format!("http://{address}"));
        let service_dir = tempdir().expect("tempdir");

        let result = OpenAIImagesAdapter::new()
            .expect("adapter")
            .execute(
                &registry,
                &auth_store(),
                &MediaGenerationService::new(service_dir.path()),
                request(),
            )
            .expect("generation succeeds");

        let (_, download_request) = server.join().expect("server");
        assert!(download_request.starts_with("GET /generated.png HTTP/1.1"));
        assert_eq!(
            std::fs::read(&result.artifact.path).unwrap(),
            b"downloaded!!"
        );
        assert_eq!(
            result.job.status,
            crate::runtime::media::MediaJobStatus::Succeeded
        );
        assert_eq!(
            result.artifact.metadata["revisedPrompt"],
            "draw a more precise icon"
        );
        assert_eq!(
            result.artifact.metadata["remoteSourceUrl"],
            format!("http://{address}/generated.png")
        );
    }

    #[test]
    fn missing_image_data_returns_stable_error() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let body = json!({"data": [{}]}).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("response");
        });
        let registry = registry_with_provider(format!("http://{address}"));
        let service_dir = tempdir().expect("tempdir");

        let error = OpenAIImagesAdapter::new()
            .expect("adapter")
            .execute(
                &registry,
                &auth_store(),
                &MediaGenerationService::new(service_dir.path()),
                request(),
            )
            .expect_err("missing image data should fail");

        server.join().expect("server");
        assert_eq!(
            error.to_string(),
            "image generation response did not contain an image"
        );
    }

    #[test]
    fn external_http_image_url_is_rejected_before_download() {
        let value = json!({
            "data": [{"url": "http://example.com/generated.png"}]
        });

        let error = image_output_from_response(&Client::new(), &value)
            .expect_err("external http URL should fail before download");

        assert_eq!(
            error.to_string(),
            "unsupported image response URL scheme `http`"
        );
    }

    #[test]
    fn unsupported_parameter_fails_before_http_request() {
        let registry = registry_with_provider("http://127.0.0.1:9".to_string());
        let service_dir = tempdir().expect("tempdir");
        let mut request = request();
        request.size = "2048x2048".to_string();

        let error = OpenAIImagesAdapter::new()
            .expect("adapter")
            .execute(
                &registry,
                &auth_store(),
                &MediaGenerationService::new(service_dir.path()),
                request,
            )
            .expect_err("unsupported parameter should fail");

        assert_eq!(
            error.to_string(),
            "image generation parameter unsupported: size=2048x2048"
        );
    }

    #[test]
    fn openai_provider_uses_codex_credentials_for_generation() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let address = listener.local_addr().expect("address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            let request_text = read_http_request(&mut stream);
            let body = json!({
                "data": [{"b64_json": "aW1hZ2UtYnl0ZXM="}]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("response");
            request_text
        });
        let registry = registry_with_provider_id("openai", format!("http://{address}"));
        let service_dir = tempdir().expect("tempdir");
        let request = OpenAIImagesGenerationRequest {
            provider_id: "openai".to_string(),
            model_id: "exact-image-model".to_string(),
            ..request()
        };

        OpenAIImagesAdapter::new()
            .expect("adapter")
            .execute(
                &registry,
                &codex_auth_store(),
                &MediaGenerationService::new(service_dir.path()),
                request,
            )
            .expect("generation succeeds");

        let request_text = server.join().expect("server");
        assert!(request_text.contains("authorization: Bearer sk-codex-secret"));
    }
}
