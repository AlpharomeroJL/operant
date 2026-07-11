//! The outbound HTTP seam for authorize probing (tests only), token
//! exchange, refresh, and revoke. Two implementations:
//!
//! - [`LoopbackHttpClient`]: always compiled, hand-rolled on `std::net`,
//!   and refuses any URL that is not `http://127.0.0.1:*`. This is what
//!   every test in this crate uses to talk to [`super::mock_server`]
//!   (`contracts/fixtures/oauth/config.json`'s own `base_url` shape) --
//!   `cargo test -p operant-orchestrator` never needs the `real-transport`
//!   feature, `reqwest`, or a real socket to anywhere but itself.
//! - [`RealTransport`] (behind `real-transport`, the same feature
//!   `backends::ReqwestTransport` already gates): a thin adapter over that
//!   exact transport, so this module does not maintain a second TLS
//!   client. Real subscription-OAuth endpoints are internet hosts over
//!   HTTPS, which [`LoopbackHttpClient`] refuses by construction.

use futures::future::BoxFuture;
use futures::FutureExt;

use super::error::OauthError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub struct TokenRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl TokenRequest {
    pub fn get(url: impl Into<String>) -> Self {
        TokenRequest {
            method: Method::Get,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn post_form(url: impl Into<String>, form_body: String) -> Self {
        TokenRequest {
            method: Method::Post,
            url: url.into(),
            headers: vec![(
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            )],
            body: form_body.into_bytes(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl TokenResponse {
    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// The seam every oauth wire call goes through.
pub trait TokenTransport: Send + Sync {
    fn send(&self, request: TokenRequest) -> BoxFuture<'static, Result<TokenResponse, OauthError>>;
}

/// Hand-rolled HTTP/1.1 client restricted to `http://127.0.0.1:*`. See
/// module doc for why this exists instead of always requiring
/// `real-transport`.
#[derive(Default)]
pub struct LoopbackHttpClient;

impl LoopbackHttpClient {
    pub fn new() -> Self {
        LoopbackHttpClient
    }
}

impl TokenTransport for LoopbackHttpClient {
    fn send(&self, request: TokenRequest) -> BoxFuture<'static, Result<TokenResponse, OauthError>> {
        async move {
            tokio::task::spawn_blocking(move || send_blocking(request))
                .await
                .map_err(|e| OauthError::Transport(format!("loopback client task panicked: {e}")))?
        }
        .boxed()
    }
}

fn parse_loopback_url(url: &str) -> Result<(String, u16, String), OauthError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| OauthError::NonLoopbackTransportUrl(url.to_string()))?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let (host, port_str) = authority
        .split_once(':')
        .ok_or_else(|| OauthError::NonLoopbackTransportUrl(url.to_string()))?;
    if host != "127.0.0.1" {
        return Err(OauthError::NonLoopbackTransportUrl(url.to_string()));
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| OauthError::NonLoopbackTransportUrl(url.to_string()))?;
    Ok((host.to_string(), port, path))
}

fn send_blocking(request: TokenRequest) -> Result<TokenResponse, OauthError> {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;

    let (host, port, path) = parse_loopback_url(&request.url)?;

    let mut stream = TcpStream::connect((host.as_str(), port))
        .map_err(|e| OauthError::Transport(e.to_string()))?;
    let method_str = match request.method {
        Method::Get => "GET",
        Method::Post => "POST",
    };
    let mut head =
        format!("{method_str} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n");
    for (name, value) in &request.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", request.body.len()));

    stream
        .write_all(head.as_bytes())
        .map_err(|e| OauthError::Transport(e.to_string()))?;
    if !request.body.is_empty() {
        stream
            .write_all(&request.body)
            .map_err(|e| OauthError::Transport(e.to_string()))?;
    }

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| OauthError::Transport(e.to_string()))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| OauthError::Transport(format!("malformed status line: {status_line:?}")))?;

    let mut headers = Vec::new();
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| OauthError::Transport(e.to_string()))?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some((name, value)) = trimmed.split_once(':') {
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().ok();
            }
            headers.push((name, value));
        }
    }

    let mut body = Vec::new();
    match content_length {
        Some(len) => {
            body.resize(len, 0);
            reader
                .read_exact(&mut body)
                .map_err(|e| OauthError::Transport(e.to_string()))?;
        }
        None => {
            reader
                .read_to_end(&mut body)
                .map_err(|e| OauthError::Transport(e.to_string()))?;
        }
    }

    Ok(TokenResponse {
        status,
        headers,
        body,
    })
}

/// Real transport: adapts [`crate::backends::ReqwestTransport`], which
/// already exists precisely to be the one real HTTP client this crate
/// compiles behind `real-transport`. Not used by any test in this crate.
#[cfg(feature = "real-transport")]
pub struct RealTransport {
    inner: crate::backends::ReqwestTransport,
}

#[cfg(feature = "real-transport")]
impl RealTransport {
    pub fn new() -> Self {
        RealTransport {
            inner: crate::backends::ReqwestTransport::new(),
        }
    }
}

#[cfg(feature = "real-transport")]
impl Default for RealTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "real-transport")]
impl TokenTransport for RealTransport {
    fn send(&self, request: TokenRequest) -> BoxFuture<'static, Result<TokenResponse, OauthError>> {
        // `HttpTransport` must be in scope for `self.inner.send(..)` to
        // resolve below -- it is a trait method, not an inherent one.
        use crate::backends::HttpTransport as _;

        let method = match request.method {
            Method::Get => crate::backends::HttpMethod::Get,
            Method::Post => crate::backends::HttpMethod::Post,
        };
        let http_request = crate::backends::HttpRequest {
            method,
            url: request.url,
            headers: request.headers,
            body: request.body,
        };
        // `send` resolves synchronously into an owned `'static` future
        // (see `backends::HttpTransport`'s own doc), so it does not
        // borrow `self` inside the `async move` block below.
        let response_fut = self.inner.send(http_request);
        async move {
            let response = response_fut
                .await
                .map_err(|e| OauthError::Transport(e.to_string()))?;
            let status = response.status;
            let body = response
                .collect_body()
                .await
                .map_err(|e| OauthError::Transport(e.to_string()))?;
            Ok(TokenResponse {
                status,
                headers: Vec::new(),
                body,
            })
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_loopback_url_rejects_a_non_loopback_host() {
        let err = parse_loopback_url("http://example.com/oauth/token").unwrap_err();
        assert!(matches!(err, OauthError::NonLoopbackTransportUrl(_)));
    }

    #[test]
    fn parse_loopback_url_rejects_https() {
        let err = parse_loopback_url("https://127.0.0.1:9/oauth/token").unwrap_err();
        assert!(matches!(err, OauthError::NonLoopbackTransportUrl(_)));
    }

    #[test]
    fn parse_loopback_url_accepts_127_0_0_1_and_splits_port_and_path() {
        let (host, port, path) = parse_loopback_url("http://127.0.0.1:54321/oauth/token").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 54321);
        assert_eq!(path, "/oauth/token");
    }

    #[test]
    fn parse_loopback_url_defaults_path_to_root() {
        let (_, _, path) = parse_loopback_url("http://127.0.0.1:9").unwrap();
        assert_eq!(path, "/");
    }

    #[tokio::test]
    async fn loopback_client_round_trips_a_get_against_a_hand_rolled_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let body = b"{\"ok\":true}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });

        let client = LoopbackHttpClient::new();
        let response = client
            .send(TokenRequest::get(format!(
                "http://127.0.0.1:{port}/anything"
            )))
            .await
            .unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(response.body_str(), "{\"ok\":true}");
        assert_eq!(response.header("content-type"), Some("application/json"));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn loopback_client_refuses_a_non_loopback_url_without_connecting() {
        let client = LoopbackHttpClient::new();
        let err = client
            .send(TokenRequest::get("http://example.com/oauth/token"))
            .await
            .unwrap_err();
        assert!(matches!(err, OauthError::NonLoopbackTransportUrl(_)));
    }
}
