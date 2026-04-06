use crate::auth::OpenAIAuth;
use serde::{Deserialize, Serialize};

/// A minimal OpenAI Responses API request payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAIResponsesRequest {
    pub model: String,
    pub input: String,
}

/// Runtime request configuration for the OpenAI provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAIRequestConfig {
    pub base_url: String,
    pub version: String,
    pub auth: OpenAIAuth,
}

/// An ordered HTTP request representation for tests and execution adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltOpenAIRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Builds a minimal OpenAI Responses API request with ordered headers.
pub fn build_responses_request(
    config: &OpenAIRequestConfig,
    request: &OpenAIResponsesRequest,
) -> anyhow::Result<BuiltOpenAIRequest> {
    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        (
            "User-Agent".to_string(),
            format!("puffer-code/{}", config.version),
        ),
    ];
    match &config.auth {
        OpenAIAuth::ApiKey(key) | OpenAIAuth::OAuthBearer(key) => {
            headers.push(("Authorization".to_string(), format!("Bearer {key}")));
        }
    }
    Ok(BuiltOpenAIRequest {
        method: "POST",
        url: format!("{}/v1/responses", config.base_url.trim_end_matches('/')),
        headers,
        body: serde_json::to_string(request)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_uses_bearer_auth() {
        let request = build_responses_request(
            &OpenAIRequestConfig {
                base_url: "https://api.openai.com".to_string(),
                version: "0.1.0".to_string(),
                auth: OpenAIAuth::ApiKey("sk-test".to_string()),
            },
            &OpenAIResponsesRequest {
                model: "gpt-5".to_string(),
                input: "hello".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            request.headers[2],
            ("Authorization".to_string(), "Bearer sk-test".to_string())
        );
    }
}
