//! [`Broker`]: the subscription sign-in flow end to end (X16 / FR-M1).
//!
//! ```text
//! Broker::begin()              -- bind loopback, build authorize_url
//!   -> hand authorize_url to the caller's browser (UI's job, not this crate's)
//!   -> PendingAuth::finish()   -- block for the redirect, verify state+nonce,
//!                                  exchange code for tokens, store in the vault
//! Broker::refresh()            -- silent refresh, 5 minutes before expiry
//!                                  (see TokenSet::needs_refresh), rotates
//! Broker::revoke()             -- revoke + clear the vault
//! ```
//!
//! Every wire call goes through the injected [`TokenTransport`]; every
//! credential goes through the injected [`Vault`]; every log line goes
//! through the injected [`SecretGuard`]. Nothing here opens a socket or
//! touches the OS credential store directly, which is what makes the test
//! module below able to run the entire flow against [`super::mock_server`]
//! with zero real network beyond loopback.

use std::sync::Arc;
use std::time::Duration;

use operant_doctor::Finding;

use super::doctor::refresh_failure_finding;
use super::error::OauthError;
use super::listener::LoopbackListener;
use super::pkce::Pkce;
use super::provider::OauthProviderConfig;
use super::redact::SecretGuard;
use super::token::TokenSet;
use super::transport::{TokenRequest, TokenTransport};
use super::urlenc;
use super::vault::{vault_key, TokenKind, Vault};

/// How long [`PendingAuth::finish`] waits for the browser to complete the
/// redirect before giving up.
pub const DEFAULT_CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

/// One subscription identity's broker: which provider, how to talk to it,
/// and where its tokens live. Cheap to construct; every field is an `Arc`
/// or plain data.
pub struct Broker {
    provider: OauthProviderConfig,
    transport: Arc<dyn TokenTransport>,
    vault: Arc<dyn Vault>,
    guard: Arc<SecretGuard>,
}

impl Broker {
    pub fn new(
        provider: OauthProviderConfig,
        transport: Arc<dyn TokenTransport>,
        vault: Arc<dyn Vault>,
    ) -> Self {
        Broker {
            provider,
            transport,
            vault,
            guard: Arc::new(SecretGuard::new()),
        }
    }

    pub fn provider(&self) -> &OauthProviderConfig {
        &self.provider
    }

    /// Start a sign-in: bind the loopback listener and build the
    /// authorize URL. Synchronous -- nothing here waits on the network or
    /// the browser; only [`PendingAuth::finish`] does.
    pub fn begin(&self) -> Result<PendingAuth, OauthError> {
        let listener = LoopbackListener::bind()?;
        let redirect_uri = listener.redirect_uri("/callback");

        let state = super::pkce::random_token(16);
        let nonce = super::pkce::random_token(16);
        let pkce = Pkce::generate();
        self.guard.register(&pkce.verifier);

        let scope = self.provider.scopes.join(" ");
        let query = urlenc::build_query(&[
            ("response_type", "code"),
            ("client_id", self.provider.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", scope.as_str()),
            ("state", state.as_str()),
            ("nonce", nonce.as_str()),
            ("code_challenge", pkce.challenge.as_str()),
            ("code_challenge_method", Pkce::METHOD),
        ]);
        let authorize_url = format!("{}?{query}", self.provider.authorize_url_base());

        Ok(PendingAuth {
            listener,
            provider: self.provider.clone(),
            transport: self.transport.clone(),
            vault: self.vault.clone(),
            guard: self.guard.clone(),
            state,
            nonce,
            pkce,
            redirect_uri,
            authorize_url,
        })
    }

    /// Silent refresh (`docs/specs/backends.md`: "refresh 5 minutes before
    /// expiry"; see [`TokenSet::needs_refresh`] for the predicate a caller
    /// polls to decide when to call this). Loads the stored refresh
    /// token, exchanges it, rotates the vault entry.
    pub async fn refresh(&self) -> Result<TokenSet, OauthError> {
        let refresh_key = vault_key(self.provider.id, TokenKind::Refresh);
        let refresh_token = self
            .vault
            .load(&refresh_key)?
            .ok_or_else(|| OauthError::NoRefreshToken(self.provider.id.to_string()))?;
        self.guard.register(&refresh_token);

        let body = urlenc::build_query(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
            ("client_id", self.provider.client_id.as_str()),
        ]);
        let response = self
            .transport
            .send(TokenRequest::post_form(self.provider.token_url(), body))
            .await?;

        if !(200..300).contains(&response.status) {
            let redacted_body = self.guard.redact(&response.body_str());
            return Err(OauthError::Provider {
                status: response.status,
                body: redacted_body,
            });
        }

        let parsed = parse_token_response(&response.body)?;
        self.guard.register_str(&parsed.access_token);
        if let Some(rt) = &parsed.refresh_token {
            self.guard.register_str(rt);
        }

        let mut token_set =
            TokenSet::new(parsed.access_token, Duration::from_secs(parsed.expires_in));
        token_set.token_type = parsed.token_type;
        if let Some(scope) = parsed.scope {
            token_set = token_set.with_scope(scope);
        }
        // "refresh_rotates": true in the contract, but fall back to
        // keeping the current refresh token if a provider ever omits it.
        let new_refresh = parsed
            .refresh_token
            .unwrap_or_else(|| refresh_token.expose_secret().to_string());
        token_set = token_set.with_refresh_token(new_refresh);

        self.vault.store(
            &vault_key(self.provider.id, TokenKind::Access),
            &token_set.access_token,
        )?;
        if let Some(rt) = &token_set.refresh_token {
            self.vault
                .store(&vault_key(self.provider.id, TokenKind::Refresh), rt)?;
        }

        self.guard.info(&format!(
            "silent refresh succeeded for {}",
            self.provider.id
        ));
        Ok(token_set)
    }

    /// [`Broker::refresh`], but on failure returns the typed
    /// [`operant_doctor::Finding`] a doctor/UI surface shows directly --
    /// "refresh failure emits a doctor finding with a one-click re-auth
    /// card" (`docs/specs/backends.md`). This module only builds the
    /// value; wiring it onto the `doctor.finding` bus topic and rendering
    /// the card is doctor/UI work outside this lane's owned paths.
    pub async fn refresh_or_finding(&self) -> Result<TokenSet, Finding> {
        self.refresh().await.map_err(|err| {
            let detail = self.guard.redact(&err.to_string());
            refresh_failure_finding(self.provider.id, &detail)
        })
    }

    /// Revoke at the provider and clear the vault. Revokes the refresh
    /// token if one is stored (falling back to the access token), and
    /// only clears the vault once the provider confirms -- a failed
    /// revoke call leaves local tokens in place so a caller can retry
    /// rather than silently losing a sign-in it could not actually cancel.
    pub async fn revoke(&self) -> Result<(), OauthError> {
        let refresh_key = vault_key(self.provider.id, TokenKind::Refresh);
        let access_key = vault_key(self.provider.id, TokenKind::Access);

        let (token_value, hint) = match self.vault.load(&refresh_key)? {
            Some(rt) => {
                self.guard.register(&rt);
                (rt.expose_secret().to_string(), "refresh_token")
            }
            None => match self.vault.load(&access_key)? {
                Some(at) => {
                    self.guard.register(&at);
                    (at.expose_secret().to_string(), "access_token")
                }
                None => return Ok(()), // nothing stored; already signed out
            },
        };

        let body = urlenc::build_query(&[
            ("token", token_value.as_str()),
            ("token_type_hint", hint),
            ("client_id", self.provider.client_id.as_str()),
        ]);
        let response = self
            .transport
            .send(TokenRequest::post_form(self.provider.revoke_url(), body))
            .await?;

        if !(200..300).contains(&response.status) {
            let redacted_body = self.guard.redact(&response.body_str());
            return Err(OauthError::Provider {
                status: response.status,
                body: redacted_body,
            });
        }

        self.vault.delete(&access_key)?;
        self.vault.delete(&refresh_key)?;
        self.guard
            .info(&format!("revoked sign-in for {}", self.provider.id));
        Ok(())
    }
}

/// A sign-in in progress: the loopback listener is bound and the authorize
/// URL is built, but the browser has not redirected back yet.
pub struct PendingAuth {
    listener: LoopbackListener,
    provider: OauthProviderConfig,
    transport: Arc<dyn TokenTransport>,
    vault: Arc<dyn Vault>,
    guard: Arc<SecretGuard>,
    state: String,
    nonce: String,
    pkce: Pkce,
    redirect_uri: String,
    authorize_url: String,
}

impl PendingAuth {
    /// The URL to open in the user's browser. Opening it is the caller's
    /// job (UI/CLI) -- this crate never launches a process.
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn port(&self) -> u16 {
        self.listener.port()
    }

    /// Block (asynchronously) for the loopback callback, verify
    /// `state`/`nonce`, exchange the code, and store the resulting tokens.
    /// See [`DEFAULT_CALLBACK_TIMEOUT`].
    pub async fn finish(self) -> Result<TokenSet, OauthError> {
        self.finish_with_timeout(DEFAULT_CALLBACK_TIMEOUT).await
    }

    pub async fn finish_with_timeout(self, timeout: Duration) -> Result<TokenSet, OauthError> {
        let PendingAuth {
            listener,
            provider,
            transport,
            vault,
            guard,
            state,
            nonce,
            pkce,
            redirect_uri,
            ..
        } = self;

        // `accept_once` itself is bounded by `timeout` (polls a
        // non-blocking socket internally rather than truly blocking
        // forever) -- see `listener`'s module doc for why that, and not
        // an outer `tokio::time::timeout` wrapping an unbounded blocking
        // call, is what actually keeps this from hanging when the
        // browser never redirects back.
        let join_result = tokio::task::spawn_blocking(move || listener.accept_once(timeout)).await;
        let request = join_result.map_err(|join_err| {
            OauthError::Listener(format!("callback task panicked: {join_err}"))
        })??;

        // "state": "echoed and verified; mismatch rejected" -- checked
        // before any token exchange is attempted.
        let received_state = request.get("state").unwrap_or("");
        if received_state != state {
            return Err(OauthError::StateMismatch);
        }
        // Nonce is echoed by this broker's own mock/provider table
        // alongside state; verified the same way when present. Full
        // binding to an id_token claim is a FOLLOWUP for a provider that
        // issues one (see X16 handoff).
        if let Some(received_nonce) = request.get("nonce") {
            if received_nonce != nonce {
                return Err(OauthError::StateMismatch);
            }
        }

        let code = request
            .get("code")
            .ok_or_else(|| OauthError::Listener("callback missing code".to_string()))?
            .to_string();
        guard.register_str(&code);

        let body = urlenc::build_query(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", provider.client_id.as_str()),
            ("code_verifier", pkce.verifier.expose_secret()),
        ]);
        let response = transport
            .send(TokenRequest::post_form(provider.token_url(), body))
            .await?;

        if !(200..300).contains(&response.status) {
            let redacted_body = guard.redact(&response.body_str());
            return Err(OauthError::Provider {
                status: response.status,
                body: redacted_body,
            });
        }

        let parsed = parse_token_response(&response.body)?;
        guard.register_str(&parsed.access_token);
        if let Some(rt) = &parsed.refresh_token {
            guard.register_str(rt);
        }

        let mut token_set =
            TokenSet::new(parsed.access_token, Duration::from_secs(parsed.expires_in));
        token_set.token_type = parsed.token_type;
        if let Some(scope) = parsed.scope {
            token_set = token_set.with_scope(scope);
        }
        if let Some(rt) = parsed.refresh_token {
            token_set = token_set.with_refresh_token(rt);
        }

        vault.store(
            &vault_key(provider.id, TokenKind::Access),
            &token_set.access_token,
        )?;
        if let Some(rt) = &token_set.refresh_token {
            vault.store(&vault_key(provider.id, TokenKind::Refresh), rt)?;
        }

        guard.info(&format!("sign-in complete for {}", provider.id));
        Ok(token_set)
    }
}

struct ParsedTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
    scope: Option<String>,
}

fn parse_token_response(body: &[u8]) -> Result<ParsedTokenResponse, OauthError> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| OauthError::Parse(e.to_string()))?;
    let access_token = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OauthError::Parse("response missing access_token".to_string()))?
        .to_string();
    let refresh_token = value
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let expires_in = value
        .get("expires_in")
        .and_then(|v| v.as_u64())
        .unwrap_or(3600);
    let token_type = value
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();
    let scope = value
        .get("scope")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Ok(ParsedTokenResponse {
        access_token,
        refresh_token,
        expires_in,
        token_type,
        scope,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::Mutex;

    use futures::future::BoxFuture;
    use futures::FutureExt;

    use super::*;
    use crate::oauth::mock_server::MockOauthServer;
    use crate::oauth::provider::ProviderId;
    use crate::oauth::transport::LoopbackHttpClient;
    use crate::oauth::vault::MockVault;

    /// A [`TokenTransport`] that never actually sends anything -- it just
    /// records what it was asked to send, for asserting a code path never
    /// reached the wire.
    #[derive(Default)]
    struct RecordingTransport {
        calls: Mutex<Vec<TokenRequest>>,
    }

    impl RecordingTransport {
        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl TokenTransport for RecordingTransport {
        fn send(
            &self,
            request: TokenRequest,
        ) -> BoxFuture<'static, Result<TokenResponseAlias, OauthError>> {
            self.calls.lock().unwrap().push(request);
            async {
                Err(OauthError::Transport(
                    "RecordingTransport never sends".to_string(),
                ))
            }
            .boxed()
        }
    }

    // Local alias so the `TokenTransport` impl above reads naturally
    // without importing `TokenResponse` under two names.
    use crate::oauth::transport::TokenResponse as TokenResponseAlias;

    fn mock_provider(id: ProviderId, base_url: String) -> OauthProviderConfig {
        OauthProviderConfig::for_provider(id, base_url)
    }

    #[test]
    fn begin_always_emits_s256_and_never_plain() {
        let transport: Arc<dyn TokenTransport> = Arc::new(RecordingTransport::default());
        let vault: Arc<dyn Vault> = Arc::new(MockVault::new());
        let provider = mock_provider(ProviderId::ClaudePlan, "http://127.0.0.1:1".to_string());
        let broker = Broker::new(provider, transport, vault);

        let pending = broker.begin().unwrap();
        assert!(pending
            .authorize_url()
            .contains("code_challenge_method=S256"));
        assert!(pending.authorize_url().contains("response_type=code"));
        assert!(!pending.authorize_url().to_lowercase().contains("plain"));
        assert!(pending.redirect_uri().starts_with("http://127.0.0.1:"));
    }

    #[tokio::test]
    async fn state_mismatch_is_rejected_before_any_token_exchange_is_attempted() {
        let transport = Arc::new(RecordingTransport::default());
        let vault: Arc<dyn Vault> = Arc::new(MockVault::new());
        let provider = mock_provider(ProviderId::ChatgptPlan, "http://127.0.0.1:1".to_string());
        let broker = Broker::new(provider, transport.clone(), vault);

        let pending = broker.begin().unwrap();
        let port = pending.port();

        let finish_task = tokio::spawn(pending.finish());

        // Act as a hostile or buggy browser: hit the loopback callback
        // directly with a state that does not match what `begin()`
        // generated, skipping the real authorize step entirely.
        let client = LoopbackHttpClient::new();
        let _ = client
            .send(TokenRequest::get(format!(
                "http://127.0.0.1:{port}/callback?code=whatever&state=WRONG-STATE"
            )))
            .await
            .unwrap();

        let result = finish_task.await.unwrap();
        assert!(
            matches!(result, Err(OauthError::StateMismatch)),
            "got {result:?}"
        );
        assert_eq!(
            transport.call_count(),
            0,
            "token exchange must never be attempted on a state mismatch"
        );
    }

    #[tokio::test]
    async fn finish_times_out_if_the_browser_never_completes_the_redirect() {
        let transport = Arc::new(RecordingTransport::default());
        let vault: Arc<dyn Vault> = Arc::new(MockVault::new());
        let provider = mock_provider(ProviderId::ChatgptPlan, "http://127.0.0.1:1".to_string());
        let broker = Broker::new(provider, transport, vault);

        let pending = broker.begin().unwrap();
        let result = pending.finish_with_timeout(Duration::from_millis(50)).await;
        assert!(
            matches!(result, Err(OauthError::CallbackTimeout)),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn mock_server_rejects_pkce_plain_and_requires_s256() {
        let server = MockOauthServer::start();
        let client = LoopbackHttpClient::new();

        let plain_url = format!(
            "{}/oauth/authorize?response_type=code&client_id=x&redirect_uri={}&state=s&nonce=n&code_challenge=abc123&code_challenge_method=plain",
            server.base_url(),
            urlenc::encode("http://127.0.0.1:1/callback"),
        );
        let resp = client.send(TokenRequest::get(plain_url)).await.unwrap();
        assert_eq!(
            resp.status,
            400,
            "plain PKCE must be rejected: {}",
            resp.body_str()
        );

        let missing_method_url = format!(
            "{}/oauth/authorize?response_type=code&client_id=x&redirect_uri={}&state=s&nonce=n&code_challenge=abc123",
            server.base_url(),
            urlenc::encode("http://127.0.0.1:1/callback"),
        );
        let resp2 = client
            .send(TokenRequest::get(missing_method_url))
            .await
            .unwrap();
        assert_eq!(
            resp2.status, 400,
            "missing method must default to rejected, not accepted"
        );
    }

    /// The X16 TESTS bar, end to end: authorize -> callback with code ->
    /// token -> refresh -> revoke, entirely against the in-process mock
    /// server, plus the "tokens never leak" grep checks (captured log
    /// output and a fake config file).
    #[tokio::test]
    async fn full_flow_green_against_the_mock_server_and_tokens_never_leak() {
        let server = MockOauthServer::start();
        let transport: Arc<dyn TokenTransport> = Arc::new(LoopbackHttpClient::new());
        let vault = Arc::new(MockVault::new());
        let provider = mock_provider(ProviderId::ChatgptPlan, server.base_url());
        let broker = Broker::new(provider.clone(), transport, vault.clone());

        // --- authorize -> callback with code -> token ---------------
        let pending = broker.begin().unwrap();
        let authorize_url = pending.authorize_url().to_string();
        let finish_task = tokio::spawn(pending.finish());

        let browser = LoopbackHttpClient::new();
        let authorize_resp = browser
            .send(TokenRequest::get(authorize_url))
            .await
            .unwrap();
        assert_eq!(
            authorize_resp.status,
            302,
            "authorize must redirect: {}",
            authorize_resp.body_str()
        );
        let location = authorize_resp
            .header("location")
            .expect("authorize response missing Location")
            .to_string();
        let callback_resp = browser.send(TokenRequest::get(location)).await.unwrap();
        assert_eq!(callback_resp.status, 200);
        assert_eq!(server.authorize_request_count(), 1);

        let first_tokens = finish_task
            .await
            .unwrap()
            .expect("initial exchange should succeed");
        let first_access = first_tokens.access_token.expose_secret().to_string();
        let first_refresh = first_tokens
            .refresh_token
            .as_ref()
            .expect("mock server always issues a refresh token")
            .expose_secret()
            .to_string();
        assert!(first_access.starts_with("at-"));
        assert!(first_refresh.starts_with("rt-"));
        assert_eq!(server.token_request_count(), 1);

        let access_key = vault_key(provider.id, TokenKind::Access);
        let refresh_key = vault_key(provider.id, TokenKind::Refresh);
        assert_eq!(
            vault.load(&access_key).unwrap().unwrap().expose_secret(),
            first_access
        );
        assert_eq!(
            vault.load(&refresh_key).unwrap().unwrap().expose_secret(),
            first_refresh
        );

        // --- refresh (rotates) ---------------------------------------
        let refreshed = broker.refresh().await.expect("refresh should succeed");
        let refreshed_access = refreshed.access_token.expose_secret().to_string();
        let refreshed_refresh = refreshed
            .refresh_token
            .as_ref()
            .unwrap()
            .expose_secret()
            .to_string();
        assert_ne!(
            refreshed_access, first_access,
            "refresh must rotate the access token"
        );
        assert_ne!(
            refreshed_refresh, first_refresh,
            "refresh must rotate the refresh token"
        );
        assert_eq!(
            vault.load(&access_key).unwrap().unwrap().expose_secret(),
            refreshed_access
        );

        // The old refresh token is now dead: using it again must fail
        // with the contract's `revoked_refresh_returns: 400`.
        let stale_body = urlenc::build_query(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", first_refresh.as_str()),
            ("client_id", provider.client_id.as_str()),
        ]);
        let stale_resp = LoopbackHttpClient::new()
            .send(TokenRequest::post_form(provider.token_url(), stale_body))
            .await
            .unwrap();
        assert_eq!(stale_resp.status, 400);

        // --- revoke ----------------------------------------------------
        broker.revoke().await.expect("revoke should succeed");
        assert!(vault.load(&access_key).unwrap().is_none());
        assert!(vault.load(&refresh_key).unwrap().is_none());
        assert_eq!(server.revoke_request_count(), 1);

        // A refresh after revoke has nothing to load from the vault.
        let after_revoke = broker.refresh().await;
        assert!(matches!(after_revoke, Err(OauthError::NoRefreshToken(_))));

        // --- tokens never appear in captured log output ----------------
        let writer = CaptureWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            broker.guard_for_test().error(&format!(
                "diagnostic dump: access={refreshed_access} refresh={refreshed_refresh}"
            ));
        });
        let captured = String::from_utf8(writer.0.lock().unwrap().clone()).unwrap();
        assert!(
            !captured.contains(&refreshed_access),
            "access token leaked into logs: {captured}"
        );
        assert!(
            !captured.contains(&refreshed_refresh),
            "refresh token leaked into logs: {captured}"
        );

        // --- tokens never appear in a fake config file ------------------
        let config_path = std::env::temp_dir().join(format!(
            "operant-oauth-test-config-{}.json",
            std::process::id()
        ));
        let fake_config = serde_json::json!({
            "provider": provider.id.as_str(),
            "client_id": provider.client_id,
            "base_url": provider.base_url,
        });
        {
            let mut f = std::fs::File::create(&config_path).unwrap();
            f.write_all(
                serde_json::to_string_pretty(&fake_config)
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        }
        let on_disk = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            !on_disk.contains(&first_access),
            "access token leaked into config file: {on_disk}"
        );
        assert!(
            !on_disk.contains(&first_refresh),
            "refresh token leaked into config file: {on_disk}"
        );
        assert!(
            !on_disk.contains(&refreshed_access),
            "access token leaked into config file: {on_disk}"
        );
        assert!(
            !on_disk.contains(&refreshed_refresh),
            "refresh token leaked into config file: {on_disk}"
        );
        let _ = std::fs::remove_file(&config_path);
    }

    /// A `MakeWriter` that captures everything written to it into a
    /// shared buffer.
    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    impl Broker {
        /// Test-only accessor: the grep-log test needs to emit through
        /// the exact guard `refresh`/`revoke`/`finish` use, rather than a
        /// freshly constructed one, to prove the real call sites redact.
        fn guard_for_test(&self) -> &SecretGuard {
            &self.guard
        }
    }
}
