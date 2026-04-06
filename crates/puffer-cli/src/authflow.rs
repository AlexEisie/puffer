use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// Waits for a single OAuth callback request on the host/port implied by the redirect URI.
pub fn wait_for_callback_url(redirect_uri: &str, timeout: Duration) -> Result<Option<String>> {
    let url = url::Url::parse(redirect_uri)
        .with_context(|| format!("invalid redirect uri {redirect_uri}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("redirect uri is missing a host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("redirect uri is missing a port"))?;
    let expected_path = url.path().to_string();

    let listener = TcpListener::bind((host, port))
        .with_context(|| format!("failed to bind callback listener on {host}:{port}"))?;
    listener.set_nonblocking(true)?;

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buffer = [0_u8; 4096];
                let bytes_read = stream.read(&mut buffer)?;
                let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                if let Some(callback_url) =
                    parse_callback_request(&request, host, port, &expected_path)
                {
                    let _ = stream.write_all(success_response().as_bytes());
                    return Ok(Some(callback_url));
                }
                let _ = stream.write_all(error_response().as_bytes());
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.into()),
        }
    }

    Ok(None)
}

fn parse_callback_request(
    request: &str,
    host: &str,
    port: u16,
    expected_path: &str,
) -> Option<String> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    if method != "GET" {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != expected_path {
        return None;
    }
    let suffix = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };
    Some(format!("http://{host}:{port}{suffix}"))
}

fn success_response() -> &'static str {
    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 74\r\n\r\n<html><body>Authentication completed. You can return to Puffer.</body></html>"
}

fn error_response() -> &'static str {
    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 53\r\n\r\n<html><body>Invalid callback for Puffer.</body></html>"
}

#[cfg(test)]
mod tests {
    use super::parse_callback_request;
    use super::wait_for_callback_url;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn parses_matching_get_request() {
        let request = "GET /auth/callback?code=abc&state=xyz HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let parsed = parse_callback_request(request, "localhost", 1455, "/auth/callback");
        assert_eq!(
            parsed.as_deref(),
            Some("http://localhost:1455/auth/callback?code=abc&state=xyz")
        );
    }

    #[test]
    fn ignores_wrong_path() {
        let request = "GET /wrong HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert!(parse_callback_request(request, "localhost", 1455, "/auth/callback").is_none());
    }

    #[test]
    fn callback_listener_captures_matching_request() {
        let temp_listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = temp_listener.local_addr().unwrap().port();
        drop(temp_listener);

        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let thread_redirect = redirect_uri.clone();
        let handle = thread::spawn(move || {
            wait_for_callback_url(&thread_redirect, Duration::from_secs(2)).unwrap()
        });

        thread::sleep(Duration::from_millis(150));
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(
                b"GET /callback?code=test-code&state=test-state HTTP/1.1\r\nHost: localhost\r\n\r\n",
            )
            .unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("Authentication completed"));

        let callback = handle.join().unwrap();
        let expected = format!("http://127.0.0.1:{port}/callback?code=test-code&state=test-state");
        assert_eq!(callback.as_deref(), Some(expected.as_str()));
    }
}
