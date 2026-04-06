use crate::model::{
    ModelDescriptor, ProviderDescriptor, ProviderSource, ProviderSourceKind, RegisteredProvider,
};
use indexmap::IndexMap;

/// Stores all providers and models known to the application.
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    providers: IndexMap<String, RegisteredProvider>,
}

impl ProviderRegistry {
    /// Creates an empty provider registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces a provider descriptor using the builtin source kind.
    pub fn register(&mut self, provider: ProviderDescriptor) {
        self.register_with_source(
            provider,
            ProviderSource {
                kind: ProviderSourceKind::Builtin,
                path: None,
            },
        );
    }

    /// Registers or replaces a provider descriptor with explicit provenance.
    pub fn register_with_source(&mut self, provider: ProviderDescriptor, source: ProviderSource) {
        self.providers.insert(
            provider.id.clone(),
            RegisteredProvider {
                descriptor: provider,
                source,
            },
        );
    }

    /// Registers a sequence of providers into the registry.
    pub fn register_many(&mut self, providers: impl IntoIterator<Item = ProviderDescriptor>) {
        for provider in providers {
            self.register(provider);
        }
    }

    /// Returns an iterator over all registered provider descriptors in insertion order.
    pub fn providers(&self) -> impl Iterator<Item = &ProviderDescriptor> {
        self.providers.values().map(|provider| &provider.descriptor)
    }

    /// Returns an iterator over all registered providers including provenance.
    pub fn provider_entries(&self) -> impl Iterator<Item = &RegisteredProvider> {
        self.providers.values()
    }

    /// Looks up a provider descriptor by id.
    pub fn provider(&self, id: &str) -> Option<&ProviderDescriptor> {
        self.providers.get(id).map(|provider| &provider.descriptor)
    }

    /// Looks up a registered provider entry by id.
    pub fn provider_entry(&self, id: &str) -> Option<&RegisteredProvider> {
        self.providers.get(id)
    }

    /// Returns an iterator over all known models across all providers.
    pub fn models(&self) -> impl Iterator<Item = &ModelDescriptor> {
        self.providers
            .values()
            .flat_map(|provider| provider.descriptor.models.iter())
    }

    /// Resolves a model from a `provider/model` selector string.
    pub fn resolve_model(&self, value: &str) -> Option<&ModelDescriptor> {
        let (provider_id, model_id) = value.split_once('/')?;
        self.provider(provider_id)?
            .models
            .iter()
            .find(|model| model.id == model_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthMode;

    fn provider_descriptor() -> ProviderDescriptor {
        ProviderDescriptor {
            id: "anthropic".to_string(),
            display_name: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            default_api: "anthropic-messages".to_string(),
            auth_modes: vec![AuthMode::ApiKey, AuthMode::OAuth],
            headers: IndexMap::new(),
            models: vec![ModelDescriptor {
                id: "claude-sonnet-4-5".to_string(),
                display_name: "Claude Sonnet 4.5".to_string(),
                provider: "anthropic".to_string(),
                api: "anthropic-messages".to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_reasoning: true,
            }],
        }
    }

    #[test]
    fn registry_tracks_provider_sources() {
        let mut registry = ProviderRegistry::new();
        registry.register_with_source(
            provider_descriptor(),
            ProviderSource {
                kind: ProviderSourceKind::ResourcePack,
                path: Some("resources/providers/anthropic.yaml".to_string()),
            },
        );

        let entry = registry
            .provider_entry("anthropic")
            .expect("provider entry");
        assert_eq!(entry.source.kind, ProviderSourceKind::ResourcePack);
        assert_eq!(
            registry
                .resolve_model("anthropic/claude-sonnet-4-5")
                .expect("model")
                .display_name,
            "Claude Sonnet 4.5"
        );
    }
}
