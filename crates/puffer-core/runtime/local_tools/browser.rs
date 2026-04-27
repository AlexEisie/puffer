//! Runtime-local Browser tool client for the desktop daemon.

use anyhow::{anyhow, bail, Context, Result};
use puffer_config::ConfigPaths;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tungstenite::{connect, Message};
use url::Url;
use uuid::Uuid;

/// Executes the model-facing Browser tool against the local Puffer daemon.
pub(super) fn execute_browser_tool(
    cwd: &Path,
    current_session_id: &Uuid,
    input: Value,
) -> Result<String> {
    let mut params: BrowserToolInput =
        serde_json::from_value(input).context("invalid Browser tool input")?;
    normalize_session_id(&mut params, current_session_id);
    let handshake = read_handshake(cwd)?;
    let response = send_daemon_request(&handshake, "browser_agent", serde_json::to_value(params)?)?;
    Ok(serde_json::to_string_pretty(&response)?)
}

fn normalize_session_id(params: &mut BrowserToolInput, current_session_id: &Uuid) {
    let use_current = params
        .session_id
        .as_deref()
        .map(|value| value.trim().is_empty() || value == "current")
        .unwrap_or(true);
    if use_current {
        params.session_id = Some(current_session_id.to_string());
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserToolInput {
    action: String,
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    #[serde(default, rename = "tabId")]
    tab_id: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default, rename = "ref")]
    ref_id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    activate: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DaemonHandshake {
    url: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct DaemonResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<DaemonError>,
}

#[derive(Debug, Deserialize)]
struct DaemonError {
    message: String,
}

fn read_handshake(cwd: &Path) -> Result<DaemonHandshake> {
    let paths = ConfigPaths::discover(cwd);
    let path = paths.user_config_dir.join("daemon.handshake");
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "Browser tool requires a running Puffer daemon at {}",
            path.display()
        )
    })?;
    serde_json::from_str(&text).context("decode daemon handshake")
}

fn send_daemon_request(handshake: &DaemonHandshake, method: &str, params: Value) -> Result<Value> {
    let request_id = Uuid::new_v4().to_string();
    let endpoint = endpoint_with_token(&handshake.url, &handshake.token)?;
    let (mut socket, _) = connect(endpoint.as_str()).context("connect to Puffer daemon")?;
    let request = json!({
        "id": request_id,
        "method": method,
        "params": params
    });
    socket
        .send(Message::Text(request.to_string().into()))
        .context("send Browser request to daemon")?;
    loop {
        let message = socket.read().context("read Browser response from daemon")?;
        let Message::Text(text) = message else {
            continue;
        };
        let response: DaemonResponse =
            serde_json::from_str(&text).context("decode Browser daemon response")?;
        if response.id.as_deref() != Some(request_id.as_str()) {
            continue;
        }
        if let Some(error) = response.error {
            bail!("{}", error.message);
        }
        return response
            .result
            .ok_or_else(|| anyhow!("daemon returned no Browser result"));
    }
}

fn endpoint_with_token(raw: &str, token: &str) -> Result<String> {
    let mut url = Url::parse(raw).context("parse daemon URL")?;
    url.query_pairs_mut().append_pair("token", token);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_missing_current_and_empty_session_ids() {
        let current = Uuid::parse_str("2ba8b01d-5e7a-46b6-b747-7bfe5f6fa36a").unwrap();
        let current_string = current.to_string();
        for value in [None, Some("current"), Some("  ")] {
            let mut params = BrowserToolInput {
                action: "list".to_string(),
                session_id: value.map(ToString::to_string),
                tab_id: None,
                label: None,
                url: None,
                ref_id: None,
                text: None,
                key: None,
                script: None,
                width: None,
                height: None,
                activate: None,
            };
            normalize_session_id(&mut params, &current);
            assert_eq!(params.session_id.as_deref(), Some(current_string.as_str()));
        }
    }

    #[test]
    fn preserves_explicit_real_session_id() {
        let current = Uuid::parse_str("2ba8b01d-5e7a-46b6-b747-7bfe5f6fa36a").unwrap();
        let explicit = "b4f239fd-1493-4be7-a3a1-9e58fe612576";
        let mut params = BrowserToolInput {
            action: "list".to_string(),
            session_id: Some(explicit.to_string()),
            tab_id: None,
            label: None,
            url: None,
            ref_id: None,
            text: None,
            key: None,
            script: None,
            width: None,
            height: None,
            activate: None,
        };
        normalize_session_id(&mut params, &current);
        assert_eq!(params.session_id.as_deref(), Some(explicit));
    }
}
