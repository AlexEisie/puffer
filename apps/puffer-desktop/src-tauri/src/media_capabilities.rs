use crate::dtos::MediaCapabilityInfoDto;
use puffer_core::{list_exact_media_capabilities, MediaCapabilityView};
use puffer_provider_registry::{AuthStore, ProviderRegistry};

/// Lists descriptor-backed exact media capabilities for the desktop backend.
pub(crate) fn list(
    providers: &ProviderRegistry,
    auth_store: &AuthStore,
    kind: Option<&str>,
) -> Vec<MediaCapabilityInfoDto> {
    list_exact_media_capabilities(providers, auth_store, kind)
        .into_iter()
        .map(media_capability_info_dto)
        .collect()
}

fn media_capability_info_dto(capability: MediaCapabilityView) -> MediaCapabilityInfoDto {
    MediaCapabilityInfoDto {
        provider_id: capability.provider_id,
        model_id: capability.model_id,
        kind: capability.kind,
        operations: capability.operations,
        supports_async: capability.supports_async,
        supports_streaming: capability.supports_streaming,
        parameter_values: capability.parameter_values,
        status: capability.status,
        source: capability.source,
        reason: capability.reason,
        checked_at_ms: capability.checked_at_ms,
    }
}
