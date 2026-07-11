//! An in-process mock OAuth provider matching
//! `contracts/fixtures/oauth/config.json` exactly: `authorize` / `token`
//! (doubling as `refresh`, per the contract's endpoint map) / `revoke`,
//! PKCE S256 enforcement, state+nonce echoed back, refresh rotation,
//! revoked-refresh-returns-400. Used only by `super::flow`'s own tests
//! (`cargo test -p operant-orchestrator`'s "full flow green against the
//! mock server" bar).
//!
//! Independent from -- but contract-identical to -- the standalone
//! `e2e/mock-oauth` server this lane also ships (a Node process other
//! lanes' browser-driven e2e tests spawn against a real browser). The two
//! do not share code: duplicating this small a contract keeps
//! `cargo test -p operant-orchestrator` hermetic (no Node, no subprocess,
//! no port-coordination flakiness) while still satisfying the "against
//! the mock server or an in-process mock" bar with the in-process option.
//! See DECISIONS in the X16 handoff.

#![cfg(test)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::pkce::{challenge_for, random_token};
use super::urlenc;

struct AuthCodeRecord {
    client_id: String,
    code_challenge: String,
    redirect_uri: String,
    used: bool,
}

struct RefreshRecord {
    client_id: String,
}

#[derive(Default)]
struct ServerState {
    codes: HashMap<String, AuthCodeRecord>,
    refresh_tokens: HashMap<String, RefreshRecord>,
    access_tokens: HashMap<String, String>,
}

/// A running mock OAuth provider bound to `127.0.0.1` on an ephemeral
/// port. Stopped automatically when dropped.
pub struct MockOauthServer {
    port: u16,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    authorize_requests: Arc<AtomicU64>,
    token_requests: Arc<AtomicU64>,
    revoke_requests: Arc<AtomicU64>,
}

impl MockOauthServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock oauth server");
        let port = listener.local_addr().unwrap().port();
        listener
            .set_nonblocking(true)
            .expect("mock oauth server: set_nonblocking");

        let state = Arc::new(Mutex::new(ServerState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let authorize_requests = Arc::new(AtomicU64::new(0));
        let token_requests = Arc::new(AtomicU64::new(0));
        let revoke_requests = Arc::new(AtomicU64::new(0));

        let thread_state = state.clone();
        let thread_shutdown = shutdown.clone();
        let thread_authorize = authorize_requests.clone();
        let thread_token = token_requests.clone();
        let thread_revoke = revoke_requests.clone();

        let handle = std::thread::spawn(move || {
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).ok();
                        handle_connection(
                            stream,
                            &thread_state,
                            &thread_authorize,
                            &thread_token,
                            &thread_revoke,
                        );
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        MockOauthServer {
            port,
            shutdown,
            handle: Some(handle),
            authorize_requests,
            token_requests,
            revoke_requests,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn authorize_request_count(&self) -> u64 {
        self.authorize_requests.load(Ordering::SeqCst)
    }

    pub fn token_request_count(&self) -> u64 {
        self.token_requests.load(Ordering::SeqCst)
    }

    pub fn revoke_request_count(&self) -> u64 {
        self.revoke_requests.load(Ordering::SeqCst)
    }
}

impl Drop for MockOauthServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct RawResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl RawResponse {
    fn json(status: u16, body: serde_json::Value) -> Self {
        RawResponse {
            status,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn redirect(location: String) -> Self {
        RawResponse {
            status: 302,
            headers: vec![("Location".to_string(), location)],
            body: Vec::new(),
        }
    }

    fn not_found() -> Self {
        RawResponse {
            status: 404,
            headers: Vec::new(),
            body: b"not found".to_vec(),
        }
    }
}

fn handle_connection(
    mut stream: std::net::TcpStream,
    state: &Arc<Mutex<ServerState>>,
    authorize_requests: &Arc<AtomicU64>,
    token_requests: &Arc<AtomicU64>,
    revoke_requests: &Arc<AtomicU64>,
) {
    let cloned = match stream.try_clone() {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut reader = BufReader::new(cloned);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path_and_query = parts.next().unwrap_or("").to_string();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap_or(0);
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.trim_end_matches(['\r', '\n']).split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 && reader.read_exact(&mut body).is_err() {
        return;
    }

    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (path_and_query, String::new()),
    };

    let response = match (method.as_str(), path.as_str()) {
        ("GET", "/oauth/authorize") => {
            authorize_requests.fetch_add(1, Ordering::SeqCst);
            handle_authorize(&query, state)
        }
        ("POST", "/oauth/token") => {
            token_requests.fetch_add(1, Ordering::SeqCst);
            handle_token(&body, state)
        }
        ("POST", "/oauth/revoke") => {
            revoke_requests.fetch_add(1, Ordering::SeqCst);
            handle_revoke(&body, state)
        }
        _ => RawResponse::not_found(),
    };

    let _ = write_response(&mut stream, &response);
}

fn write_response(stream: &mut std::net::TcpStream, response: &RawResponse) -> std::io::Result<()> {
    let status_text = match response.status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {} {status_text}\r\n", response.status);
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.body.len()
    ));
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

/// `GET /oauth/authorize`: validates PKCE (S256 only -- `plain` and any
/// other/missing method are rejected per
/// `contracts/fixtures/oauth/config.json`'s `pkce` rule), validates the
/// redirect URI is loopback-only, mints a one-time code, and redirects
/// back to the caller's `redirect_uri` with `code`, `state`, and `nonce`
/// echoed.
fn handle_authorize(query: &str, state: &Arc<Mutex<ServerState>>) -> RawResponse {
    let params = urlenc::parse_query(query);
    let get = |k: &str| params.get(k).cloned().unwrap_or_default();

    if get("response_type") != "code" {
        return RawResponse::json(
            400,
            serde_json::json!({"error": "unsupported_response_type"}),
        );
    }
    if get("code_challenge_method") != "S256" {
        // "pkce": "S256 required; plain rejected"
        return RawResponse::json(
            400,
            serde_json::json!({"error": "invalid_request", "error_description": "code_challenge_method must be S256"}),
        );
    }
    if get("code_challenge").is_empty() {
        return RawResponse::json(
            400,
            serde_json::json!({"error": "invalid_request", "error_description": "code_challenge required"}),
        );
    }
    let redirect_uri = get("redirect_uri");
    if !redirect_uri.starts_with("http://127.0.0.1:") {
        // "redirect": "loopback 127.0.0.1 with ephemeral port only"
        return RawResponse::json(
            400,
            serde_json::json!({"error": "invalid_request", "error_description": "redirect_uri must be loopback"}),
        );
    }
    let client_id = get("client_id");
    if client_id.is_empty() {
        return RawResponse::json(400, serde_json::json!({"error": "invalid_client"}));
    }

    let code = format!("code-{}", random_token(18));
    state.lock().unwrap().codes.insert(
        code.clone(),
        AuthCodeRecord {
            client_id,
            code_challenge: get("code_challenge"),
            redirect_uri: redirect_uri.clone(),
            used: false,
        },
    );

    let location = format!(
        "{redirect_uri}?{}",
        urlenc::build_query(&[
            ("code", code.as_str()),
            ("state", get("state").as_str()),
            ("nonce", get("nonce").as_str()),
        ])
    );
    RawResponse::redirect(location)
}

/// `POST /oauth/token`: `grant_type=authorization_code` (initial exchange,
/// PKCE-verified against the challenge recorded at authorize time) or
/// `grant_type=refresh_token` (rotates -- issues a new access AND refresh
/// token, invalidates the old refresh token; an unknown/already-rotated
/// refresh token returns 400 per the contract's `revoked_refresh_returns`
/// rule).
fn handle_token(body: &[u8], state: &Arc<Mutex<ServerState>>) -> RawResponse {
    let params = urlenc::parse_query(&String::from_utf8_lossy(body));
    match params.get("grant_type").map(String::as_str) {
        Some("authorization_code") => handle_authorization_code_grant(&params, state),
        Some("refresh_token") => handle_refresh_grant(&params, state),
        _ => RawResponse::json(400, serde_json::json!({"error": "unsupported_grant_type"})),
    }
}

fn handle_authorization_code_grant(
    params: &HashMap<String, String>,
    state: &Arc<Mutex<ServerState>>,
) -> RawResponse {
    let empty = String::new();
    let code = params.get("code").unwrap_or(&empty);
    let verifier = params.get("code_verifier").cloned().unwrap_or_default();
    let redirect_uri = params.get("redirect_uri").cloned().unwrap_or_default();
    let client_id = params.get("client_id").cloned().unwrap_or_default();

    let mut guard = state.lock().unwrap();
    let Some(record) = guard.codes.get_mut(code) else {
        return RawResponse::json(400, serde_json::json!({"error": "invalid_grant"}));
    };
    if record.used || record.client_id != client_id || record.redirect_uri != redirect_uri {
        return RawResponse::json(400, serde_json::json!({"error": "invalid_grant"}));
    }
    if challenge_for(&verifier) != record.code_challenge {
        return RawResponse::json(
            400,
            serde_json::json!({"error": "invalid_grant", "error_description": "PKCE verification failed"}),
        );
    }
    record.used = true;

    let access_token = format!("at-{}", random_token(24));
    let refresh_token = format!("rt-{}", random_token(24));
    guard
        .access_tokens
        .insert(access_token.clone(), client_id.clone());
    guard
        .refresh_tokens
        .insert(refresh_token.clone(), RefreshRecord { client_id });
    drop(guard);

    RawResponse::json(
        200,
        serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "model.complete",
        }),
    )
}

fn handle_refresh_grant(
    params: &HashMap<String, String>,
    state: &Arc<Mutex<ServerState>>,
) -> RawResponse {
    let empty = String::new();
    let refresh_token = params.get("refresh_token").unwrap_or(&empty).clone();
    let client_id = params.get("client_id").cloned().unwrap_or_default();

    let mut guard = state.lock().unwrap();
    let Some(record) = guard.refresh_tokens.get(&refresh_token) else {
        // "revoked_refresh_returns": 400 -- also covers "never issued".
        return RawResponse::json(400, serde_json::json!({"error": "invalid_grant"}));
    };
    if record.client_id != client_id {
        return RawResponse::json(400, serde_json::json!({"error": "invalid_grant"}));
    }

    // "refresh_rotates": true -- the old refresh token stops working.
    guard.refresh_tokens.remove(&refresh_token);
    let new_access = format!("at-{}", random_token(24));
    let new_refresh = format!("rt-{}", random_token(24));
    guard
        .access_tokens
        .insert(new_access.clone(), client_id.clone());
    guard
        .refresh_tokens
        .insert(new_refresh.clone(), RefreshRecord { client_id });
    drop(guard);

    RawResponse::json(
        200,
        serde_json::json!({
            "access_token": new_access,
            "refresh_token": new_refresh,
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "model.complete",
        }),
    )
}

/// `POST /oauth/revoke`: accepts either an access or a refresh token
/// (`token_type_hint` is advisory only, matching RFC 7009 servers that
/// tolerate an absent/wrong hint); always returns 200, per the contract
/// (revoking an unknown token is not an error).
fn handle_revoke(body: &[u8], state: &Arc<Mutex<ServerState>>) -> RawResponse {
    let params = urlenc::parse_query(&String::from_utf8_lossy(body));
    let token = params.get("token").cloned().unwrap_or_default();

    let mut guard = state.lock().unwrap();
    guard.refresh_tokens.remove(&token);
    guard.access_tokens.remove(&token);
    drop(guard);

    RawResponse::json(200, serde_json::json!({"revoked": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::transport::{LoopbackHttpClient, TokenRequest, TokenTransport};

    #[tokio::test]
    async fn unknown_path_returns_404() {
        let server = MockOauthServer::start();
        let client = LoopbackHttpClient::new();
        let resp = client
            .send(TokenRequest::get(format!("{}/nope", server.base_url())))
            .await
            .unwrap();
        assert_eq!(resp.status, 404);
    }

    #[tokio::test]
    async fn revoke_with_an_unknown_token_still_returns_200() {
        let server = MockOauthServer::start();
        let client = LoopbackHttpClient::new();
        let resp = client
            .send(TokenRequest::post_form(
                format!("{}/oauth/revoke", server.base_url()),
                "token=never-issued&client_id=x".to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(server.revoke_request_count(), 1);
    }

    #[tokio::test]
    async fn token_endpoint_rejects_an_unsupported_grant_type() {
        let server = MockOauthServer::start();
        let client = LoopbackHttpClient::new();
        let resp = client
            .send(TokenRequest::post_form(
                format!("{}/oauth/token", server.base_url()),
                "grant_type=client_credentials".to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status, 400);
    }
}
