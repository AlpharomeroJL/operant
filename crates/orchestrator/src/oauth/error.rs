//! Errors surfaced by the OAuth broker (X16, `docs/specs/backends.md` OAuth
//! broker section). Kept as one flat enum (rather than per-submodule error
//! types) because every variant here is something a caller (doctor, UI,
//! CLI) needs to branch on by name, not just display.

use thiserror::Error;

use super::vault::VaultError;

/// Everything that can go wrong starting, completing, refreshing, or
/// revoking a subscription sign-in.
#[derive(Debug, Error)]
pub enum OauthError {
    /// The loopback callback's `state` did not match the value this flow
    /// started with. Rejected before any token exchange is attempted, per
    /// `contracts/fixtures/oauth/config.json`'s `state: "echoed and
    /// verified; mismatch rejected"` rule.
    #[error("state mismatch: the callback did not carry the state this flow started with")]
    StateMismatch,

    /// The redirect URI a caller supplied is not a loopback address. Every
    /// authorize URL this broker builds targets `127.0.0.1`; refusing
    /// anything else here is defense in depth against a misconfigured
    /// provider table sending a code to a non-loopback host.
    #[error("redirect_uri must be a loopback 127.0.0.1 address, got: {0}")]
    NonLoopbackRedirect(String),

    /// A base URL supplied to the transport was not `http://127.0.0.1:*`.
    /// Only the non-TLS loopback transport enforces this (the real
    /// `reqwest`-backed transport, behind `real-transport`, talks to real
    /// provider hosts over HTTPS instead).
    #[error("loopback transport only accepts http://127.0.0.1 URLs, got: {0}")]
    NonLoopbackTransportUrl(String),

    /// The vault (credential store) failed a store/load/delete.
    #[error("credential vault error: {0}")]
    Vault(#[from] VaultError),

    /// The loopback listener could not bind, accept, or parse the
    /// callback request.
    #[error("loopback callback listener error: {0}")]
    Listener(String),

    /// Waiting for the browser to complete the redirect took too long.
    #[error("timed out waiting for the sign-in callback")]
    CallbackTimeout,

    /// A transport-level failure talking to the provider (connect, I/O,
    /// TLS). Distinct from a well-formed non-2xx response, which is
    /// [`OauthError::Provider`].
    #[error("transport error: {0}")]
    Transport(String),

    /// The provider responded, but with a non-2xx status.
    #[error("provider returned HTTP {status}: {body}")]
    Provider { status: u16, body: String },

    /// The provider's response body did not parse as the JSON shape this
    /// broker expects.
    #[error("could not parse provider response: {0}")]
    Parse(String),

    /// `refresh()` was called for a provider with no stored refresh token
    /// (never signed in, or already revoked).
    #[error("no refresh token stored for provider `{0}`; sign in again")]
    NoRefreshToken(String),
}

impl OauthError {
    /// A stable, log-safe identifier per variant, for callers that want to
    /// branch or report without matching the enum directly (mirrors
    /// `backends::BackendError::error_id`).
    pub fn error_id(&self) -> &'static str {
        match self {
            OauthError::StateMismatch => "oauth_state_mismatch",
            OauthError::NonLoopbackRedirect(_) => "oauth_non_loopback_redirect",
            OauthError::NonLoopbackTransportUrl(_) => "oauth_non_loopback_transport_url",
            OauthError::Vault(_) => "oauth_vault_error",
            OauthError::Listener(_) => "oauth_listener_error",
            OauthError::CallbackTimeout => "oauth_callback_timeout",
            OauthError::Transport(_) => "oauth_transport_error",
            OauthError::Provider { .. } => "oauth_provider_error",
            OauthError::Parse(_) => "oauth_parse_error",
            OauthError::NoRefreshToken(_) => "oauth_no_refresh_token",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_id_is_stable_per_variant() {
        assert_eq!(OauthError::StateMismatch.error_id(), "oauth_state_mismatch");
        assert_eq!(
            OauthError::NoRefreshToken("claude_plan".to_string()).error_id(),
            "oauth_no_refresh_token"
        );
    }

    #[test]
    fn display_never_needs_the_debug_impl_to_be_readable() {
        let e = OauthError::Provider {
            status: 400,
            body: "invalid_grant".to_string(),
        };
        assert_eq!(e.to_string(), "provider returned HTTP 400: invalid_grant");
    }
}
