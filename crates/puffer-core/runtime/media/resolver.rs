use super::capabilities::{MediaCapability, MediaKind};
use anyhow::{bail, Result};
use puffer_provider_registry::{
    canonical_provider_id, AuthStore, MediaExecutionKind, MediaModelDescriptor, MediaOperation,
    ProviderDescriptor, ProviderRegistry,
};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Carries cached dynamic media discovery records into capability resolution.
#[derive(Debug, Clone, Default)]
pub(crate) struct MediaDiscoveryCache {
    pub(crate) image_models: Vec<CachedImageMediaModel>,
}

/// Carries one cached exact image model discovered for a provider.
#[derive(Debug, Clone)]
pub(crate) struct CachedImageMediaModel {
    pub(crate) provider_id: String,
    pub(crate) model: MediaModelDescriptor,
}

/// Describes a saved exact image generation selection to validate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImageGenerationSelection<'a> {
    pub(crate) provider_id: &'a str,
    pub(crate) model_id: &'a str,
    pub(crate) size: &'a str,
    pub(crate) quality: &'a str,
    pub(crate) output_format: &'a str,
}

/// Resolves selectable exact media capabilities from provider descriptors.
pub(crate) fn resolve_media_capabilities(
    registry: &ProviderRegistry,
    auth_store: &AuthStore,
    kind: MediaKind,
    operation: MediaOperation,
    checked_at_ms: u64,
    discovery_cache: &MediaDiscoveryCache,
) -> Vec<MediaCapability> {
    match kind {
        MediaKind::Image => resolve_image_capabilities(
            registry,
            auth_store,
            operation,
            checked_at_ms,
            discovery_cache,
        ),
        MediaKind::Video => Vec::new(),
    }
}

/// Validates a saved exact image generation selection against current capabilities.
pub(crate) fn validate_image_generate_selection(
    registry: &ProviderRegistry,
    auth_store: &AuthStore,
    selection: &ImageGenerationSelection<'_>,
    checked_at_ms: u64,
    discovery_cache: &MediaDiscoveryCache,
) -> Result<MediaCapability> {
    let capability = resolve_media_capabilities(
        registry,
        auth_store,
        MediaKind::Image,
        MediaOperation::Generate,
        checked_at_ms,
        discovery_cache,
    )
    .into_iter()
    .find(|capability| {
        capability.provider_id == selection.provider_id && capability.model_id == selection.model_id
    });

    let Some(capability) = capability else {
        bail!(
            "selected image model unavailable: {}/{}",
            selection.provider_id,
            selection.model_id
        );
    };

    validate_parameter_value(&capability.parameter_values, "size", selection.size)?;
    validate_parameter_value(&capability.parameter_values, "quality", selection.quality)?;
    validate_parameter_value(
        &capability.parameter_values,
        "output_format",
        selection.output_format,
    )?;

    Ok(capability)
}

fn resolve_image_capabilities(
    registry: &ProviderRegistry,
    auth_store: &AuthStore,
    operation: MediaOperation,
    checked_at_ms: u64,
    discovery_cache: &MediaDiscoveryCache,
) -> Vec<MediaCapability> {
    let mut capabilities = Vec::new();
    for provider in registry.providers() {
        if !provider_is_connected(provider, auth_store) {
            continue;
        }
        let Some(image) = provider
            .media
            .as_ref()
            .and_then(|media| media.image.as_ref())
        else {
            continue;
        };
        let Some(execution) = image.execution.as_ref() else {
            continue;
        };
        if !execution_adapter_is_available(execution.adapter) {
            continue;
        }

        let mut emitted_model_ids = HashSet::new();
        for model in image.models.iter().chain(
            discovery_cache
                .image_models
                .iter()
                .filter(|cached| cached.provider_id == provider.id)
                .map(|cached| &cached.model),
        ) {
            if !emitted_model_ids.insert(model.id.clone()) {
                continue;
            }
            if !image_model_is_available(model, operation) {
                continue;
            }
            capabilities.push(MediaCapability {
                provider_id: provider.id.clone(),
                model_id: model.id.clone(),
                kind: MediaKind::Image,
                operations: vec![operation_wire_name(operation).to_string()],
                supports_async: false,
                supports_streaming: false,
                parameter_values: image_parameter_values(model),
                status: "available".to_string(),
                source: adapter_source(execution.adapter).to_string(),
                reason: None,
                checked_at_ms,
            });
        }
    }
    capabilities
}

fn provider_is_connected(provider: &ProviderDescriptor, auth_store: &AuthStore) -> bool {
    if provider.auth_modes.is_empty() {
        return true;
    }
    if auth_store.has_auth(&provider.id) {
        return true;
    }
    let canonical = canonical_provider_id(&provider.id);
    if canonical != provider.id && auth_store.has_auth(&canonical) {
        return true;
    }
    provider.id == "openai" && auth_store.has_auth("codex")
}

fn execution_adapter_is_available(adapter: MediaExecutionKind) -> bool {
    matches!(adapter, MediaExecutionKind::OpenAiImages)
}

fn image_model_is_available(model: &MediaModelDescriptor, operation: MediaOperation) -> bool {
    let id = model.id.trim();
    !id.is_empty()
        && !id.eq_ignore_ascii_case("auto")
        && !has_wildcard_or_regex_marker(id)
        && model.operations.contains(&operation)
}

fn image_parameter_values(model: &MediaModelDescriptor) -> Value {
    json!({
        "size": &model.parameters.size,
        "quality": &model.parameters.quality,
        "output_format": &model.parameters.output_format,
    })
}

fn validate_parameter_value(parameters: &Value, name: &str, value: &str) -> Result<()> {
    let Some(values) = parameters.get(name).and_then(Value::as_array) else {
        return Ok(());
    };
    if values.is_empty() {
        return Ok(());
    }
    let supported = values
        .iter()
        .filter_map(Value::as_str)
        .any(|candidate| candidate == value);
    if supported {
        Ok(())
    } else {
        bail!("image generation parameter unsupported: {name}={value}")
    }
}

fn operation_wire_name(operation: MediaOperation) -> &'static str {
    match operation {
        MediaOperation::Generate => "generate",
    }
}

fn adapter_source(adapter: MediaExecutionKind) -> &'static str {
    match adapter {
        MediaExecutionKind::OpenAiImages => "adapter:openai_images",
    }
}

fn has_wildcard_or_regex_marker(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '*' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '^' | '$' | '\\'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::media::MediaKind;
    use puffer_provider_registry::{
        AuthMode, AuthStore, ImageMediaDescriptor, MediaExecutionDescriptor, MediaExecutionKind,
        MediaImageParameters, MediaModelDescriptor, MediaOperation, ModelDescriptor,
        ProviderDescriptor, ProviderMediaDescriptor, ProviderRegistry,
    };
    use serde_json::json;

    fn registry_with(providers: Vec<ProviderDescriptor>) -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        registry.register_many(providers);
        registry
    }

    fn provider(
        id: &str,
        auth_modes: Vec<AuthMode>,
        media: Option<ProviderMediaDescriptor>,
    ) -> ProviderDescriptor {
        ProviderDescriptor {
            id: id.to_string(),
            display_name: id.to_string(),
            base_url: format!("https://{id}.example"),
            default_api: "openai-responses".to_string(),
            auth_modes,
            headers: Default::default(),
            query_params: Default::default(),
            chat_completions_path: None,
            discovery: None,
            media,
            models: Vec::<ModelDescriptor>::new(),
        }
    }

    fn image_media(model_id: &str) -> ProviderMediaDescriptor {
        ProviderMediaDescriptor {
            image: Some(ImageMediaDescriptor {
                discovery: None,
                execution: Some(MediaExecutionDescriptor {
                    adapter: MediaExecutionKind::OpenAiImages,
                    path: "/v1/images/generations".to_string(),
                }),
                models: vec![MediaModelDescriptor {
                    id: model_id.to_string(),
                    display_name: Some(model_id.to_string()),
                    operations: vec![MediaOperation::Generate],
                    parameters: MediaImageParameters::new(
                        vec!["1024x1024".to_string()],
                        vec!["auto".to_string(), "high".to_string()],
                        vec!["png".to_string()],
                    ),
                }],
            }),
        }
    }

    fn auth_for(provider_id: &str) -> AuthStore {
        let mut auth = AuthStore::default();
        auth.set_api_key(provider_id, "sk-test");
        auth
    }

    #[test]
    fn connected_exact_image_descriptor_appears() {
        let registry = registry_with(vec![provider(
            "openai",
            vec![AuthMode::ApiKey],
            Some(image_media("gpt-image-1")),
        )]);
        let capabilities = resolve_media_capabilities(
            &registry,
            &auth_for("openai"),
            MediaKind::Image,
            MediaOperation::Generate,
            42,
            &MediaDiscoveryCache::default(),
        );

        assert_eq!(capabilities.len(), 1);
        assert_eq!(capabilities[0].provider_id, "openai");
        assert_eq!(capabilities[0].model_id, "gpt-image-1");
        assert_eq!(capabilities[0].status, "available");
        assert_eq!(
            capabilities[0].parameter_values["size"],
            json!(["1024x1024"])
        );
    }

    #[test]
    fn providers_without_image_media_or_auth_are_hidden() {
        let registry = registry_with(vec![
            provider("connected-text", vec![AuthMode::ApiKey], None),
            provider(
                "missing-auth",
                vec![AuthMode::ApiKey],
                Some(image_media("gpt-image-1")),
            ),
            provider("auth-free-text", Vec::new(), None),
        ]);
        let capabilities = resolve_media_capabilities(
            &registry,
            &auth_for("connected-text"),
            MediaKind::Image,
            MediaOperation::Generate,
            42,
            &MediaDiscoveryCache::default(),
        );

        assert!(capabilities.is_empty());
    }

    #[test]
    fn auto_models_and_missing_execution_are_hidden() {
        let mut missing_execution = image_media("gpt-image-1");
        missing_execution.image.as_mut().unwrap().execution = None;
        let registry = registry_with(vec![
            provider(
                "auto-provider",
                vec![AuthMode::ApiKey],
                Some(image_media("auto")),
            ),
            provider(
                "no-execution",
                vec![AuthMode::ApiKey],
                Some(missing_execution),
            ),
        ]);
        let mut auth = AuthStore::default();
        auth.set_api_key("auto-provider", "sk-test");
        auth.set_api_key("no-execution", "sk-test");

        let capabilities = resolve_media_capabilities(
            &registry,
            &auth,
            MediaKind::Image,
            MediaOperation::Generate,
            42,
            &MediaDiscoveryCache::default(),
        );

        assert!(capabilities.is_empty());
    }

    #[test]
    fn saved_stale_provider_model_is_rejected() {
        let registry = registry_with(vec![provider(
            "openai",
            vec![AuthMode::ApiKey],
            Some(image_media("gpt-image-1")),
        )]);

        let error = validate_image_generate_selection(
            &registry,
            &auth_for("openai"),
            &ImageGenerationSelection {
                provider_id: "openai",
                model_id: "stale-image",
                size: "1024x1024",
                quality: "auto",
                output_format: "png",
            },
            42,
            &MediaDiscoveryCache::default(),
        )
        .expect_err("stale model should fail");

        assert_eq!(
            error.to_string(),
            "selected image model unavailable: openai/stale-image"
        );
    }

    #[test]
    fn unsupported_parameter_value_is_rejected() {
        let registry = registry_with(vec![provider(
            "openai",
            vec![AuthMode::ApiKey],
            Some(image_media("gpt-image-1")),
        )]);

        let error = validate_image_generate_selection(
            &registry,
            &auth_for("openai"),
            &ImageGenerationSelection {
                provider_id: "openai",
                model_id: "gpt-image-1",
                size: "2048x2048",
                quality: "auto",
                output_format: "png",
            },
            42,
            &MediaDiscoveryCache::default(),
        )
        .expect_err("unsupported size should fail");

        assert_eq!(
            error.to_string(),
            "image generation parameter unsupported: size=2048x2048"
        );
    }
}
