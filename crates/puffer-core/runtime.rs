use crate::AppState;
use anyhow::{anyhow, bail, Context, Result};
use puffer_provider_openai::{
    build_responses_request, OpenAIAuth, OpenAIRequestConfig, OpenAIResponsesRequest,
};
use puffer_provider_registry::{
    AuthStore, OAuthCredential, ProviderDescriptor, ProviderRegistry, StoredCredential,
};
use puffer_transport_anthropic::{
    build_messages_request, AnthropicAuth, AnthropicMessage, AnthropicModelRequest,
    AnthropicRequestConfig,
};
use reqwest::blocking::Client;
use serde_json::{json, Value};

/// Executes one user prompt against the currently selected provider and model.
pub fn execute_user_prompt(
    state: &AppState,
    providers: &ProviderRegistry,
    auth_store: &AuthStore,
    input: &str,
) -> Result<String> {
    let (provider, model_id) = resolve_provider_and_model(state, providers)?;
    match provider.id.as_str() {
        "anthropic" => execute_anthropic(state, provider, model_id, auth_store, input),
        "openai" => execute_openai(state, provider, model_id, auth_store, input),
        other => bail!("provider {other} is not executable yet"),
    }
}

fn resolve_provider_and_model<'a>(
    state: &AppState,
    providers: &'a ProviderRegistry,
) -> Result<(&'a ProviderDescriptor, String)> {
    if let Some(selected) = &state.current_model {
        if let Some(model) = providers.resolve_model(selected) {
            let provider = providers
                .provider(&model.provider)
                .ok_or_else(|| anyhow!("provider {} not found", model.provider))?;
            return Ok((provider, model.id.clone()));
        }
    }

    if let Some(provider_id) = &state.current_provider {
        let provider = providers
            .provider(provider_id)
            .ok_or_else(|| anyhow!("provider {provider_id} not found"))?;
        let model_id = provider
            .models
            .first()
            .map(|model| model.id.clone())
            .ok_or_else(|| anyhow!("provider {provider_id} has no configured models"))?;
        return Ok((provider, model_id));
    }

    let provider = providers
        .providers()
        .next()
        .ok_or_else(|| anyhow!("no providers are registered"))?;
    let model_id = provider
        .models
        .first()
        .map(|model| model.id.clone())
        .ok_or_else(|| anyhow!("provider {} has no configured models", provider.id))?;
    Ok((provider, model_id))
}

fn execute_anthropic(
    state: &AppState,
    provider: &ProviderDescriptor,
    model_id: String,
    auth_store: &AuthStore,
    input: &str,
) -> Result<String> {
    let auth = anthropic_auth_for_provider(auth_store, &provider.id)?;
    let request = build_messages_request(
        &AnthropicRequestConfig {
            base_url: provider.base_url.clone(),
            session_id: state.session.id.to_string(),
            custom_headers: provider.headers.clone(),
            remote_container_id: None,
            remote_session_id: None,
            client_app: None,
            entrypoint: "cli".to_string(),
            user_type: "external".to_string(),
            version: "0.1.0".to_string(),
            workload: None,
            additional_protection: false,
            cch_enabled: true,
            auth,
            beta_header: None,
            client_request_id: None,
        },
        &AnthropicModelRequest {
            model: model_id,
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: input.to_string(),
            }],
        },
    )?;

    let mut body: Value = serde_json::from_str(&request.body)?;
    body["system"] = json!([
        {
            "type": "text",
            "text": request.attribution_prefix_block,
        }
    ]);

    let response = send_http_request(&request.url, &request.headers, &body.to_string(), true)?;
    parse_anthropic_text(&response)
}

fn execute_openai(
    state: &AppState,
    provider: &ProviderDescriptor,
    model_id: String,
    auth_store: &AuthStore,
    input: &str,
) -> Result<String> {
    let auth = openai_auth_for_provider(auth_store, &provider.id)?;
    let request = build_responses_request(
        &OpenAIRequestConfig {
            base_url: provider.base_url.clone(),
            version: "0.1.0".to_string(),
            auth,
        },
        &OpenAIResponsesRequest {
            model: model_id,
            input: input.to_string(),
        },
    )?;
    let response = send_http_request(&request.url, &request.headers, &request.body, false)?;
    parse_openai_text(&response).or_else(|_| parse_openai_text_fallback(&response, state))
}

fn send_http_request(
    url: &str,
    headers: &[(String, String)],
    body: &str,
    anthropic: bool,
) -> Result<Value> {
    let client = Client::new();
    let mut request = client.post(url);
    for (key, value) in headers {
        request = request.header(key, value);
    }
    request = request.header("content-type", "application/json");
    if anthropic {
        request = request.header("anthropic-version", "2023-06-01");
    }
    let response = request
        .body(body.to_string())
        .send()
        .with_context(|| format!("request to {url} failed"))?;
    let status = response.status();
    let text = response.text()?;
    if !status.is_success() {
        bail!("request failed with status {}: {}", status, text);
    }
    let json = serde_json::from_str::<Value>(&text)
        .with_context(|| format!("response from {url} was not valid JSON"))?;
    Ok(json)
}

fn anthropic_auth_for_provider(auth_store: &AuthStore, provider_id: &str) -> Result<AnthropicAuth> {
    match auth_store.get(provider_id) {
        Some(StoredCredential::ApiKey { key }) => Ok(AnthropicAuth::ApiKey(key.clone())),
        Some(StoredCredential::OAuth(OAuthCredential { access_token, .. })) => {
            Ok(AnthropicAuth::OAuthBearer(access_token.clone()))
        }
        None => bail!(
            "no credentials configured for provider {provider_id}; use `puffer auth set-api-key {provider_id}` first"
        ),
    }
}

fn openai_auth_for_provider(auth_store: &AuthStore, provider_id: &str) -> Result<OpenAIAuth> {
    match auth_store.get(provider_id) {
        Some(StoredCredential::ApiKey { key }) => Ok(OpenAIAuth::ApiKey(key.clone())),
        Some(StoredCredential::OAuth(OAuthCredential { access_token, .. })) => {
            Ok(OpenAIAuth::OAuthBearer(access_token.clone()))
        }
        None => bail!(
            "no credentials configured for provider {provider_id}; use `puffer auth set-api-key {provider_id}` first"
        ),
    }
}

fn parse_anthropic_text(response: &Value) -> Result<String> {
    let parts = response
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("anthropic response missing content array"))?
        .iter()
        .filter_map(|item| {
            let item_type = item.get("type").and_then(Value::as_str)?;
            if item_type == "text" {
                item.get("text").and_then(Value::as_str).map(str::to_string)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        bail!("anthropic response did not contain text content");
    }
    Ok(parts.join("\n"))
}

fn parse_openai_text(response: &Value) -> Result<String> {
    if let Some(text) = response.get("output_text").and_then(Value::as_str) {
        return Ok(text.to_string());
    }

    let mut parts = Vec::new();
    if let Some(items) = response.get("output").and_then(Value::as_array) {
        for item in items {
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                for block in content {
                    let block_type = block
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if matches!(block_type, "output_text" | "text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            parts.push(text.to_string());
                        }
                    }
                }
            }
        }
    }
    if parts.is_empty() {
        bail!("openai response did not contain output text");
    }
    Ok(parts.join("\n"))
}

fn parse_openai_text_fallback(response: &Value, state: &AppState) -> Result<String> {
    if let Some(text) = response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        return Ok(text);
    }
    bail!(
        "provider {} returned an unsupported response shape for session {}",
        state.current_provider.as_deref().unwrap_or("unknown"),
        state.session.id
    )
}
