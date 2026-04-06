use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The OAuth beta header used by Anthropic subscriber requests.
pub const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

/// The Claude.ai OAuth authorize endpoint used for subscriber flows.
pub const CLAUDE_AI_AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";

/// The Console OAuth authorize endpoint used for Console account flows.
pub const CONSOLE_AUTHORIZE_URL: &str = "https://platform.claude.com/oauth/authorize";

/// The Anthropic OAuth token endpoint.
pub const ANTHROPIC_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";

/// The manual redirect URL used by Claude Code for non-local auth completion.
pub const ANTHROPIC_MANUAL_REDIRECT_URL: &str = "https://platform.claude.com/oauth/code/callback";

/// The full OAuth scope set used by Claude Code.
pub const ANTHROPIC_ALL_SCOPES: &str =
    "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

/// Authentication modes supported by Anthropic-facing requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnthropicAuth {
    ApiKey(String),
    OAuthBearer(String),
    SessionIngress {
        token: String,
        organization_uuid: Option<String>,
    },
}

/// Persisted OAuth credentials for Anthropic subscriber flows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnthropicOAuthCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: u64,
    pub scopes: Vec<String>,
    pub account_uuid: Option<String>,
    pub email_address: Option<String>,
    pub organization_uuid: Option<String>,
}

/// Stores generated PKCE values for the Anthropic OAuth flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicPkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

/// Parameters required to build an Anthropic OAuth authorization URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicOAuthConfig {
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub state: String,
    pub code_challenge: String,
    pub scopes: String,
}

impl Default for AnthropicOAuthConfig {
    fn default() -> Self {
        Self {
            authorize_url: CLAUDE_AI_AUTHORIZE_URL.to_string(),
            client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string(),
            redirect_uri: "http://127.0.0.1:53692/callback".to_string(),
            state: "puffer-state".to_string(),
            code_challenge: "puffer-code-challenge".to_string(),
            scopes: ANTHROPIC_ALL_SCOPES.to_string(),
        }
    }
}

/// Generates a PKCE verifier/challenge pair and state for Anthropic OAuth.
pub fn generate_pkce() -> AnthropicPkce {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    AnthropicPkce {
        state: verifier.clone(),
        verifier,
        challenge,
    }
}

/// Builds an Anthropic OAuth authorization URL.
pub fn build_authorization_url(config: &AnthropicOAuthConfig) -> String {
    let mut url =
        url::Url::parse(&config.authorize_url).expect("Anthropic authorize URL must be valid");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("scope", &config.scopes)
        .append_pair("code_challenge", &config.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &config.state);
    url.to_string()
}

/// Extracts a code and optional state from a pasted Anthropic OAuth input.
pub fn parse_authorization_input(input: &str) -> (Option<String>, Option<String>) {
    let value = input.trim();
    if value.is_empty() {
        return (None, None);
    }

    if let Ok(url) = url::Url::parse(value) {
        let code = url.query_pairs().find_map(|(key, value)| {
            if key == "code" {
                Some(value.into_owned())
            } else {
                None
            }
        });
        let state = url.query_pairs().find_map(|(key, value)| {
            if key == "state" {
                Some(value.into_owned())
            } else {
                None
            }
        });
        return (code, state);
    }

    if let Some((code, state)) = value.split_once('#') {
        return (Some(code.to_string()), Some(state.to_string()));
    }

    if value.contains("code=") {
        let mut code = None;
        let mut state = None;
        for (key, value) in url::form_urlencoded::parse(value.as_bytes()) {
            if key == "code" {
                code = Some(value.into_owned());
            } else if key == "state" {
                state = Some(value.into_owned());
            }
        }
        return (code, state);
    }

    (Some(value.to_string()), None)
}

/// Exchanges an Anthropic OAuth authorization code for persisted OAuth credentials.
pub fn exchange_authorization_code(
    code: &str,
    verifier: &str,
    state: &str,
    redirect_uri: Option<&str>,
) -> Result<AnthropicOAuthCredentials> {
    let client = Client::new();
    let response = client
        .post(ANTHROPIC_TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "client_id": AnthropicOAuthConfig::default().client_id,
            "code": code,
            "state": state,
            "redirect_uri": redirect_uri.unwrap_or(AnthropicOAuthConfig::default().redirect_uri.as_str()),
            "code_verifier": verifier,
        }))
        .send()
        .context("failed to exchange Anthropic authorization code")?;
    let status = response.status();
    let payload: AnthropicTokenResponse = response
        .json()
        .context("failed to parse Anthropic token response")?;
    if !status.is_success() {
        return Err(anyhow!(
            "Anthropic token exchange failed with status {}",
            status
        ));
    }
    token_response_to_credentials(payload)
}

/// Refreshes Anthropic OAuth credentials using a stored refresh token.
pub fn refresh_oauth_token(refresh_token: &str) -> Result<AnthropicOAuthCredentials> {
    let client = Client::new();
    let response = client
        .post(ANTHROPIC_TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "client_id": AnthropicOAuthConfig::default().client_id,
            "refresh_token": refresh_token,
            "scope": ANTHROPIC_ALL_SCOPES,
        }))
        .send()
        .context("failed to refresh Anthropic OAuth token")?;
    let status = response.status();
    let payload: AnthropicTokenResponse = response
        .json()
        .context("failed to parse Anthropic refresh response")?;
    if !status.is_success() {
        return Err(anyhow!(
            "Anthropic token refresh failed with status {}",
            status
        ));
    }
    token_response_to_credentials(payload)
}

fn token_response_to_credentials(
    payload: AnthropicTokenResponse,
) -> Result<AnthropicOAuthCredentials> {
    let access_token = payload
        .access_token
        .ok_or_else(|| anyhow!("Anthropic token response did not include access_token"))?;
    let refresh_token = payload
        .refresh_token
        .ok_or_else(|| anyhow!("Anthropic token response did not include refresh_token"))?;
    let expires_in = payload
        .expires_in
        .ok_or_else(|| anyhow!("Anthropic token response did not include expires_in"))?;
    let scopes = payload
        .scope
        .unwrap_or_else(|| ANTHROPIC_ALL_SCOPES.to_string())
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    Ok(AnthropicOAuthCredentials {
        access_token,
        refresh_token,
        expires_at_ms: now_ms() + (expires_in as u64) * 1000,
        scopes,
        account_uuid: None,
        email_address: None,
        organization_uuid: None,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Deserialize)]
struct AnthropicTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u32>,
    scope: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_url_contains_expected_values() {
        let url = build_authorization_url(&AnthropicOAuthConfig {
            state: "state-1".to_string(),
            code_challenge: "challenge-1".to_string(),
            ..AnthropicOAuthConfig::default()
        });
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=state-1"));
        assert!(url.contains("code_challenge=challenge-1"));
    }

    #[test]
    fn parse_authorization_input_reads_callback_url() {
        let (code, state) =
            parse_authorization_input("http://127.0.0.1:53692/callback?code=abc&state=xyz");
        assert_eq!(code.as_deref(), Some("abc"));
        assert_eq!(state.as_deref(), Some("xyz"));
    }

    #[test]
    fn generate_pkce_uses_verifier_as_state() {
        let pkce = generate_pkce();
        assert_eq!(pkce.state, pkce.verifier);
        assert!(!pkce.challenge.is_empty());
    }
}
