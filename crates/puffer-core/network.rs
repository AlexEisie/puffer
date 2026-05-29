use anyhow::{Context, Result};
use puffer_config::{ProxyConfig, ProxyEndpoint};
use reqwest::blocking::Client;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// Describes why an HTTP client is being constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpPurpose {
    Model,
    Discovery,
    OAuth,
    ConnectivityTest,
}

/// Builds a blocking reqwest client using the selected proxy when enabled.
pub fn blocking_client(
    proxy: &ProxyConfig,
    purpose: HttpPurpose,
    timeout: Duration,
) -> Result<Client> {
    let mut builder = Client::builder().timeout(timeout);
    if proxy.enabled {
        if let Some(endpoint) = selected_endpoint(proxy)? {
            builder = builder.proxy(reqwest::Proxy::all(proxy_uri(endpoint)?)?);
        }
    }
    let _ = purpose;
    builder.build().context("failed to build HTTP client")
}

/// Builds a blocking reqwest client for one target URL, honoring bypass entries.
pub fn blocking_client_for_url(
    proxy: &ProxyConfig,
    purpose: HttpPurpose,
    url: &str,
    timeout: Duration,
) -> Result<Client> {
    if proxy.enabled && !bypass_matches(proxy, url) {
        blocking_client(proxy, purpose, timeout)
    } else {
        Client::builder()
            .timeout(timeout)
            .build()
            .context("failed to build HTTP client")
    }
}

/// Tests a proxy endpoint against a URL and returns elapsed milliseconds.
pub fn test_proxy_endpoint(
    endpoint: &ProxyEndpoint,
    target_url: &str,
    timeout: Duration,
) -> Result<u128> {
    let mut config = ProxyConfig::default();
    config.enabled = true;
    config.selected = Some(endpoint.id.clone());
    config.proxies = vec![endpoint.clone()];
    let started = Instant::now();
    let client =
        blocking_client_for_url(&config, HttpPurpose::ConnectivityTest, target_url, timeout)?;
    let response = client
        .get(target_url)
        .send()
        .with_context(|| format!("proxy test request to {target_url} failed"))?;
    if !response.status().is_success() {
        anyhow::bail!("proxy test failed with HTTP {}", response.status());
    }
    Ok(started.elapsed().as_millis())
}

/// Returns true when the URL host matches a configured bypass entry.
pub fn bypass_matches(proxy: &ProxyConfig, url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    proxy
        .bypass
        .iter()
        .any(|entry| bypass_entry_matches(entry, host))
}

/// Builds the proxy URI accepted by reqwest.
pub fn proxy_uri(endpoint: &ProxyEndpoint) -> Result<String> {
    if endpoint.host.trim().is_empty() {
        anyhow::bail!("proxy host must not be empty");
    }
    let scheme = endpoint.scheme.as_uri_scheme();
    let host = endpoint.host.trim();
    let auth = match (
        endpoint
            .username
            .as_deref()
            .filter(|value| !value.is_empty()),
        endpoint
            .password
            .as_deref()
            .filter(|value| !value.is_empty()),
    ) {
        (Some(username), Some(password)) => format!(
            "{}:{}@",
            urlencoding::encode(username),
            urlencoding::encode(password)
        ),
        (Some(username), None) => format!("{}@", urlencoding::encode(username)),
        _ => String::new(),
    };
    Ok(format!("{scheme}://{auth}{host}:{}", endpoint.port))
}

fn selected_endpoint(proxy: &ProxyConfig) -> Result<Option<&ProxyEndpoint>> {
    let Some(selected) = proxy.selected.as_deref() else {
        return Ok(None);
    };
    proxy
        .proxies
        .iter()
        .find(|endpoint| endpoint.id == selected)
        .map(Some)
        .ok_or_else(|| anyhow::anyhow!("selected proxy `{selected}` does not exist"))
}

fn bypass_entry_matches(entry: &str, host: &str) -> bool {
    let entry = entry.trim();
    if entry.is_empty() {
        return false;
    }
    if entry.eq_ignore_ascii_case(host) {
        return true;
    }
    let Ok(host_ip) = host.parse::<IpAddr>() else {
        return false;
    };
    if let Ok(entry_ip) = entry.parse::<IpAddr>() {
        return host_ip == entry_ip;
    }
    if let Ok(net) = entry.parse::<ipnet::IpNet>() {
        return net.contains(&host_ip);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_config::ProxyScheme;

    fn proxy_config() -> ProxyConfig {
        ProxyConfig {
            enabled: true,
            selected: Some("local".to_string()),
            bypass: vec!["localhost".to_string(), "10.0.0.0/8".to_string()],
            proxies: vec![ProxyEndpoint {
                id: "local".to_string(),
                scheme: ProxyScheme::Socks5h,
                host: "127.0.0.1".to_string(),
                port: 7890,
                username: Some("user".to_string()),
                password: Some("pass".to_string()),
            }],
        }
    }

    #[test]
    fn proxy_uri_includes_encoded_credentials() {
        let endpoint = ProxyEndpoint {
            id: "auth".to_string(),
            scheme: ProxyScheme::Http,
            host: "proxy.example".to_string(),
            port: 8080,
            username: Some("user name".to_string()),
            password: Some("p@ss".to_string()),
        };
        assert_eq!(
            proxy_uri(&endpoint).expect("uri"),
            "http://user%20name:p%40ss@proxy.example:8080"
        );
    }

    #[test]
    fn bypass_matches_localhost_and_cidr() {
        let config = proxy_config();
        assert!(bypass_matches(&config, "http://localhost:3000/health"));
        assert!(bypass_matches(&config, "http://10.2.3.4/v1/models"));
        assert!(!bypass_matches(
            &config,
            "https://api.openai.com/v1/responses"
        ));
    }

    #[test]
    fn client_builder_accepts_selected_proxy() {
        let config = proxy_config();
        let client =
            blocking_client(&config, HttpPurpose::Model, Duration::from_secs(30)).expect("client");
        let _ = client;
    }
}
