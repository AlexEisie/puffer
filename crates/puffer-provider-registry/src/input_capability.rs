use crate::model::{Modality, ModelDiscoveryConfig, ProviderDescriptor};
use serde_json::Value;

const INPUT_MODALITY_PATHS: &[&str] = &[
    "/input_modalities",
    "/modalities/input",
    "/architecture/input_modalities",
];

const OUTPUT_MODALITY_PATHS: &[&str] = &[
    "/output_modalities",
    "/modalities/output",
    "/architecture/output_modalities",
];

/// Infers input modalities for one discovered model.
pub(crate) fn infer_input_modalities(
    _provider: &ProviderDescriptor,
    _discovery: &ModelDiscoveryConfig,
    item: &Value,
    model_id: &str,
) -> Vec<Modality> {
    if is_non_chat_media_model(model_id, item) {
        return text_only();
    }
    if item_declares_image_input(item) {
        return text_image();
    }
    if family_accepts_image_input(model_id) {
        return text_image();
    }
    text_only()
}

fn text_only() -> Vec<Modality> {
    vec![Modality::Text]
}

fn text_image() -> Vec<Modality> {
    vec![Modality::Text, Modality::Image]
}

fn item_declares_image_input(item: &Value) -> bool {
    INPUT_MODALITY_PATHS
        .iter()
        .any(|path| modality_path_contains(item, path, "image"))
}

fn item_declares_media_output(item: &Value) -> bool {
    OUTPUT_MODALITY_PATHS.iter().any(|path| {
        modality_path_contains(item, path, "image") || modality_path_contains(item, path, "video")
    })
}

fn modality_path_contains(item: &Value, path: &str, needle: &str) -> bool {
    item.pointer(path)
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|value| value.eq_ignore_ascii_case(needle))
            })
        })
}

fn is_non_chat_media_model(model_id: &str, item: &Value) -> bool {
    item_declares_media_output(item) || contains_non_chat_marker(&normalized_model_id(model_id))
}

fn contains_non_chat_marker(model_id: &str) -> bool {
    [
        "image",
        "imagen",
        "flux",
        "seedream",
        "dall-e",
        "dalle",
        "video",
        "veo",
        "seedance",
        "audio",
        "realtime",
        "embedding",
        "embeddings",
        "rerank",
        "moderation",
        "transcription",
        "whisper",
        "tts",
    ]
    .iter()
    .any(|marker| model_id.contains(marker))
}

fn family_accepts_image_input(model_id: &str) -> bool {
    let normalized = normalized_model_id(model_id);
    let family = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    is_claude_chat_family(family)
        || is_gemini_chat_family(family)
        || is_qwen_vision_family(family)
}

fn normalized_model_id(model_id: &str) -> String {
    model_id.trim().to_ascii_lowercase()
}

fn is_claude_chat_family(model_id: &str) -> bool {
    model_id.starts_with("claude-opus-")
        || model_id.starts_with("claude-sonnet-")
        || model_id.starts_with("claude-haiku-")
}

fn is_gemini_chat_family(model_id: &str) -> bool {
    model_id.starts_with("gemini-")
}

fn is_qwen_vision_family(model_id: &str) -> bool {
    model_id.contains("qwen") && contains_qwen_vision_marker(model_id)
}

fn contains_qwen_vision_marker(model_id: &str) -> bool {
    model_id.contains("vision")
        || model_id.contains("-vl")
        || model_id.contains("_vl")
        || model_id.contains(".vl")
        || model_id.contains("vl-")
        || model_id.contains("vl_")
        || model_id.ends_with("-vl")
        || model_id.ends_with("_vl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthMode;
    use crate::model::ModelDiscoveryFormat;
    use indexmap::IndexMap;
    use serde_json::json;

    fn provider() -> ProviderDescriptor {
        ProviderDescriptor {
            id: "worldrouter".to_string(),
            display_name: "WorldRouter".to_string(),
            base_url: "https://inference-api.worldrouter.ai/v1".to_string(),
            default_api: "openai-completions".to_string(),
            auth_modes: vec![AuthMode::ApiKey],
            headers: IndexMap::new(),
            query_params: IndexMap::new(),
            discovery: Some(discovery()),
            media: None,
            models: Vec::new(),
            chat_completions_path: Some("/chat/completions".to_string()),
        }
    }

    fn discovery() -> ModelDiscoveryConfig {
        ModelDiscoveryConfig {
            path: "/models".to_string(),
            response: ModelDiscoveryFormat::OpenAiModels,
            api: "openai-completions".to_string(),
            context_window: 200_000,
            max_output_tokens: 16_384,
            supports_reasoning: true,
            items_field: "data".to_string(),
            id_field: "id".to_string(),
            display_name_field: None,
            headers: IndexMap::new(),
        }
    }

    fn infer(model_id: &str, item: Value) -> Vec<Modality> {
        infer_input_modalities(&provider(), &discovery(), &item, model_id)
    }

    #[test]
    fn claude_family_accepts_image_input() {
        assert_eq!(
            infer("anthropic/claude-opus-4-8", json!({})),
            vec![Modality::Text, Modality::Image]
        );
    }

    #[test]
    fn plain_unknown_model_stays_text_only() {
        assert_eq!(infer("deepseek-chat", json!({})), vec![Modality::Text]);
    }

    #[test]
    fn explicit_input_modalities_accept_image_input() {
        assert_eq!(
            infer(
                "provider/vision-chat",
                json!({
                    "architecture": {
                        "input_modalities": ["text", "image"],
                        "output_modalities": ["text"]
                    }
                })
            ),
            vec![Modality::Text, Modality::Image]
        );
    }

    #[test]
    fn output_image_models_stay_text_only_for_chat_runtime() {
        assert_eq!(
            infer(
                "google/gemini-2.5-flash-image-preview",
                json!({
                    "architecture": {
                        "input_modalities": ["text", "image"],
                        "output_modalities": ["image"]
                    }
                })
            ),
            vec![Modality::Text]
        );
    }

    #[test]
    fn qwen_requires_vision_marker_for_image_input() {
        assert_eq!(
            infer("qwen/qwen2.5-vl-72b-instruct", json!({})),
            vec![Modality::Text, Modality::Image]
        );
        assert_eq!(infer("qwen/qwen3-coder", json!({})), vec![Modality::Text]);
    }

    #[test]
    fn broad_openai_names_do_not_infer_image_input() {
        assert_eq!(infer("openai/gpt-5.4-nano", json!({})), vec![Modality::Text]);
    }
}
