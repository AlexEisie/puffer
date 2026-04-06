use crate::auth::{AnthropicAuth, OAUTH_BETA_HEADER};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A minimal Anthropic messages request payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnthropicModelRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<AnthropicMessage>,
}

/// A text-only Anthropic message block used by the current scaffold.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: String,
}

/// Runtime request configuration for Anthropic message calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicRequestConfig {
    pub base_url: String,
    pub session_id: String,
    pub custom_headers: IndexMap<String, String>,
    pub remote_container_id: Option<String>,
    pub remote_session_id: Option<String>,
    pub client_app: Option<String>,
    pub entrypoint: String,
    pub user_type: String,
    pub version: String,
    pub fingerprint: String,
    pub workload: Option<String>,
    pub additional_protection: bool,
    pub cch_enabled: bool,
    pub auth: AnthropicAuth,
    pub beta_header: Option<String>,
    pub client_request_id: Option<String>,
}

/// An ordered HTTP request representation for Anthropic message calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltAnthropicRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub attribution_prefix_block: String,
}

/// Builds an Anthropic `/v1/messages` request while preserving Claude-like header order.
pub fn build_messages_request(
    config: &AnthropicRequestConfig,
    payload: &AnthropicModelRequest,
) -> anyhow::Result<BuiltAnthropicRequest> {
    let mut headers = vec![
        ("x-app".to_string(), "cli".to_string()),
        ("User-Agent".to_string(), anthropic_user_agent(config)),
        (
            "X-Claude-Code-Session-Id".to_string(),
            config.session_id.clone(),
        ),
    ];

    headers.extend(
        config
            .custom_headers
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );

    if let Some(container_id) = &config.remote_container_id {
        headers.push((
            "x-claude-remote-container-id".to_string(),
            container_id.clone(),
        ));
    }
    if let Some(remote_session_id) = &config.remote_session_id {
        headers.push((
            "x-claude-remote-session-id".to_string(),
            remote_session_id.clone(),
        ));
    }
    if let Some(client_app) = &config.client_app {
        headers.push(("x-client-app".to_string(), client_app.clone()));
    }
    if config.additional_protection {
        headers.push((
            "x-anthropic-additional-protection".to_string(),
            "true".to_string(),
        ));
    }

    match &config.auth {
        AnthropicAuth::ApiKey(key) => {
            headers.push(("x-api-key".to_string(), key.clone()));
        }
        AnthropicAuth::OAuthBearer(token) => {
            headers.push(("Authorization".to_string(), format!("Bearer {token}")));
            headers.push((
                "anthropic-beta".to_string(),
                config
                    .beta_header
                    .clone()
                    .unwrap_or_else(|| OAUTH_BETA_HEADER.to_string()),
            ));
        }
        AnthropicAuth::SessionIngress {
            token,
            organization_uuid,
        } => {
            if token.starts_with("sk-ant-sid") {
                headers.push(("Cookie".to_string(), format!("sessionKey={token}")));
                if let Some(org_uuid) = organization_uuid {
                    headers.push(("X-Organization-Uuid".to_string(), org_uuid.clone()));
                }
            } else {
                headers.push(("Authorization".to_string(), format!("Bearer {token}")));
            }
        }
    }

    if let Some(client_request_id) = &config.client_request_id {
        headers.push(("x-client-request-id".to_string(), client_request_id.clone()));
    }

    Ok(BuiltAnthropicRequest {
        method: "POST",
        url: format!("{}/v1/messages", config.base_url.trim_end_matches('/')),
        headers,
        body: serde_json::to_string(payload)?,
        attribution_prefix_block: attribution_header(config),
    })
}

/// Returns the Claude-style Anthropic user-agent string.
pub fn anthropic_user_agent(config: &AnthropicRequestConfig) -> String {
    let mut suffix_parts = vec![config.user_type.clone(), config.entrypoint.clone()];
    if let Some(client_app) = &config.client_app {
        suffix_parts.push(format!("client-app/{client_app}"));
    }
    if let Some(workload) = &config.workload {
        suffix_parts.push(format!("workload/{workload}"));
    }
    format!(
        "claude-cli/{} ({})",
        config.version,
        suffix_parts.join(", ")
    )
}

/// Builds the Anthropic attribution system-block payload, including the optional CCH placeholder.
pub fn attribution_header(config: &AnthropicRequestConfig) -> String {
    let cch = if config.cch_enabled {
        " cch=00000;"
    } else {
        ""
    };
    let workload = config
        .workload
        .as_ref()
        .map(|value| format!(" cc_workload={value};"))
        .unwrap_or_default();
    format!(
        "x-anthropic-billing-header: cc_version={}.{}; cc_entrypoint={};{}{}",
        config.version, config.fingerprint, config.entrypoint, cch, workload
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config(auth: AnthropicAuth) -> AnthropicRequestConfig {
        AnthropicRequestConfig {
            base_url: "https://api.anthropic.com".to_string(),
            session_id: "session-1".to_string(),
            custom_headers: IndexMap::new(),
            remote_container_id: None,
            remote_session_id: None,
            client_app: None,
            entrypoint: "cli".to_string(),
            user_type: "external".to_string(),
            version: "1.2.3".to_string(),
            fingerprint: "abcd".to_string(),
            workload: None,
            additional_protection: false,
            cch_enabled: true,
            auth,
            beta_header: None,
            client_request_id: Some("req-1".to_string()),
        }
    }

    #[test]
    fn oauth_request_preserves_expected_header_order() {
        let request = build_messages_request(
            &base_config(AnthropicAuth::OAuthBearer("token-1".to_string())),
            &AnthropicModelRequest {
                model: "claude-sonnet-4-5".to_string(),
                max_tokens: 1024,
                messages: vec![AnthropicMessage {
                    role: "user".to_string(),
                    content: "hello".to_string(),
                }],
            },
        )
        .unwrap();

        let keys: Vec<&str> = request.headers.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "x-app",
                "User-Agent",
                "X-Claude-Code-Session-Id",
                "Authorization",
                "anthropic-beta",
                "x-client-request-id",
            ]
        );
        assert!(request
            .attribution_prefix_block
            .starts_with("x-anthropic-billing-header: cc_version=1.2.3.abcd;"));
    }

    #[test]
    fn session_ingress_sid_uses_cookie_auth() {
        let request = build_messages_request(
            &AnthropicRequestConfig {
                auth: AnthropicAuth::SessionIngress {
                    token: "sk-ant-sid-123".to_string(),
                    organization_uuid: Some("org-1".to_string()),
                },
                ..base_config(AnthropicAuth::ApiKey("unused".to_string()))
            },
            &AnthropicModelRequest {
                model: "claude-sonnet-4-5".to_string(),
                max_tokens: 1,
                messages: vec![],
            },
        )
        .unwrap();
        assert!(request
            .headers
            .iter()
            .any(|(key, value)| key == "Cookie" && value == "sessionKey=sk-ant-sid-123"));
        assert!(request
            .headers
            .iter()
            .any(|(key, value)| key == "X-Organization-Uuid" && value == "org-1"));
    }
}
