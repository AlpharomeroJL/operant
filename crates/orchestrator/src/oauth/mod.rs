//! OAuth broker (X16, FR-M1): the subscription sign-in cloud path -- "Sign
//! in with ChatGPT" (Codex/ChatGPT plan) and "Sign in with Claude" (Claude
//! plan) -- so a user reaches a cloud model with a browser sign-in
//! instead of copy-pasting an API key. Mirrors `docs/specs/backends.md`'s
//! OAuth broker section and is contract-tested against
//! `contracts/fixtures/oauth/config.json`.
//!
//! - [`flow`]: [`Broker`] and [`PendingAuth`], the whole thing end to end
//!   -- bind a loopback listener, build the authorize URL, wait for the
//!   redirect, verify state and nonce, exchange the code, refresh 5
//!   minutes before expiry (silent, rotating), revoke. See `flow`'s own
//!   module doc for the exact call sequence.
//! - [`provider`]: [`ProviderId`] and [`OauthProviderConfig`], the OAuth
//!   identity table -- "the quirk table maps each OAuth identity to its
//!   API dialect and endpoints" -- mirroring `backends::quirks`'s own
//!   per-provider table in spirit.
//! - [`pkce`]: PKCE, S256 only. There is no code path that can produce a
//!   `plain` challenge; "S256 required; plain rejected" is a client-side
//!   invariant, not just a server-side check.
//! - [`token`]: [`SecretString`] and [`TokenSet`]. `SecretString`
//!   implements neither `Serialize` nor `Deserialize`, so a token cannot
//!   be embedded in a config struct without a compile error -- the
//!   primary mechanism behind "never written to config".
//! - [`vault`]: [`Vault`], [`MockVault`] (every test in this crate runs
//!   against this), and [`WindowsCredentialVault`] (the real Windows
//!   Credential Manager backend, behind the off-by-default `real-vault`
//!   feature, mirroring `operant-action`'s `real-input` convention).
//! - [`redact`]: [`SecretGuard`], the secrets-redaction guard -- every log
//!   line this module emits is scrubbed before it reaches a `tracing`
//!   event, independent of which subscriber a caller installs. Reuses
//!   [`crate::backends::redact`] for the shapes that overlap.
//! - [`listener`]: the loopback callback receiver (`127.0.0.1`, ephemeral
//!   port, one GET, hand-rolled on `std::net`).
//! - [`transport`]: [`TokenTransport`], the injectable outbound HTTP seam
//!   -- [`LoopbackHttpClient`] (always compiled, loopback-only, what every
//!   test in this crate uses) and, behind `real-transport`, an adapter
//!   over [`crate::backends::ReqwestTransport`] for real provider hosts.
//! - [`doctor`]: [`refresh_failure_finding`], the typed
//!   [`operant_doctor::Finding`] a refresh failure returns -- "refresh
//!   failure emits a doctor finding with a one-click re-auth card".
//!   Building the value is this lane's job; publishing it and rendering
//!   the card is doctor/UI wiring (FOLLOWUPS: U2B).
//! - [`mock_server`] (test-only): an in-process mock OAuth provider
//!   matching `contracts/fixtures/oauth/config.json`, used by this
//!   crate's own tests. The standalone, contract-identical
//!   `e2e/mock-oauth` server this lane also ships is independent code
//!   (Node, for other lanes' browser-driven e2e tests); see `flow`'s test
//!   module and the X16 handoff DECISIONS for why the two do not share an
//!   implementation.

mod doctor;
mod error;
mod flow;
mod listener;
mod pkce;
mod provider;
mod redact;
mod token;
mod transport;
mod urlenc;
mod vault;

#[cfg(test)]
mod mock_server;

pub use doctor::{refresh_failure_finding, refresh_failure_finding_id};
pub use error::OauthError;
pub use flow::{Broker, PendingAuth, DEFAULT_CALLBACK_TIMEOUT};
pub use pkce::Pkce;
pub use provider::{OauthProviderConfig, ProviderId};
pub use redact::SecretGuard;
pub use token::{SecretString, TokenSet, REFRESH_SKEW};
pub use transport::{LoopbackHttpClient, Method, TokenRequest, TokenResponse, TokenTransport};
pub use vault::{vault_key, MockVault, TokenKind, Vault, VaultError};

#[cfg(feature = "real-transport")]
pub use transport::RealTransport;

#[cfg(all(windows, feature = "real-vault"))]
pub use vault::WindowsCredentialVault;

/// Crate-tree marker used by the workspace smoke test, mirroring
/// `super::CRATE`.
pub const MODULE: &str = "oauth";

#[cfg(test)]
mod tests {
    #[test]
    fn module_present() {
        assert_eq!(super::MODULE, "oauth");
    }
}
