//! [`SecretString`] and [`TokenSet`]: the shapes a token ever travels in
//! after it leaves the wire.
//!
//! `SecretString` deliberately implements neither `serde::Serialize` nor
//! `serde::Deserialize`. That is not an oversight: it is the mechanism
//! behind "NEVER written to config or logs" (`docs/specs/backends.md`).
//! Any struct that tries to derive `Serialize` while holding a
//! `SecretString` field fails to compile, so a token cannot be embedded in
//! a config struct by accident -- the compiler enforces the hard rule, not
//! just a code-review convention. The only way to see the raw value is
//! [`SecretString::expose_secret`], called at exactly three sites in this
//! module tree: building an `Authorization` header, writing to the vault,
//! and comparing a PKCE verifier to its challenge.

use std::fmt;
use std::time::{Duration, SystemTime};

/// A string that must never be logged, printed, or serialized whole.
/// `Debug` and `Display` both render `[REDACTED]`; the only way out is
/// [`SecretString::expose_secret`].
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        SecretString(value.into())
    }

    /// The raw value. Callers must not pass the result to `tracing::*!`,
    /// `format!` for anything that might be logged, or any `Serialize`
    /// impl. Named loudly on purpose, matching the `secrecy` crate's
    /// convention, so a call site reads as a deliberate exception.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// The tokens issued for one completed sign-in, plus enough bookkeeping to
/// drive silent refresh. Every field that is a credential is a
/// [`SecretString`]; every field that is not (expiry, token type, scope)
/// is a plain type, because it is safe to log.
#[derive(Clone)]
pub struct TokenSet {
    pub access_token: SecretString,
    pub refresh_token: Option<SecretString>,
    pub token_type: String,
    pub scope: Option<String>,
    pub expires_at: SystemTime,
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &self.access_token)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("token_type", &self.token_type)
            .field("scope", &self.scope)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// How long before expiry silent refresh should kick in
/// (`docs/specs/backends.md`: "refresh 5 minutes before expiry").
pub const REFRESH_SKEW: Duration = Duration::from_secs(5 * 60);

impl TokenSet {
    pub fn new(access_token: impl Into<String>, expires_in: Duration) -> Self {
        TokenSet {
            access_token: SecretString::new(access_token),
            refresh_token: None,
            token_type: "Bearer".to_string(),
            scope: None,
            expires_at: SystemTime::now() + expires_in,
        }
    }

    #[must_use]
    pub fn with_refresh_token(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token = Some(SecretString::new(refresh_token));
        self
    }

    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// True once we are within [`REFRESH_SKEW`] of `expires_at` (or past
    /// it). Pure function of `now` and `expires_at` so it is trivially
    /// unit-testable without sleeping a real clock.
    pub fn needs_refresh_at(&self, now: SystemTime) -> bool {
        match self.expires_at.duration_since(now) {
            Ok(remaining) => remaining <= REFRESH_SKEW,
            // expires_at is in the past relative to now.
            Err(_) => true,
        }
    }

    pub fn needs_refresh(&self) -> bool {
        self.needs_refresh_at(SystemTime::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_never_print_the_raw_value() {
        let s = SecretString::new("sk-live-seeded-fake-0000000000000000");
        assert_eq!(format!("{s:?}"), "[REDACTED]");
        assert_eq!(format!("{s}"), "[REDACTED]");
        assert_eq!(s.expose_secret(), "sk-live-seeded-fake-0000000000000000");
    }

    #[test]
    fn token_set_debug_redacts_access_and_refresh_tokens() {
        let ts = TokenSet::new("access-seeded-fake", Duration::from_secs(3600))
            .with_refresh_token("refresh-seeded-fake");
        let dbg = format!("{ts:?}");
        assert!(!dbg.contains("access-seeded-fake"), "leaked: {dbg}");
        assert!(!dbg.contains("refresh-seeded-fake"), "leaked: {dbg}");
    }

    #[test]
    fn needs_refresh_is_false_well_before_expiry() {
        let ts = TokenSet::new("a", Duration::from_secs(3600));
        assert!(!ts.needs_refresh_at(SystemTime::now()));
    }

    #[test]
    fn needs_refresh_is_true_inside_the_skew_window() {
        let ts = TokenSet::new("a", Duration::from_secs(60)); // expires in 60s < 5m skew
        assert!(ts.needs_refresh_at(SystemTime::now()));
    }

    #[test]
    fn needs_refresh_is_true_once_already_expired() {
        let ts = TokenSet::new("a", Duration::from_secs(0));
        let later = SystemTime::now() + Duration::from_secs(10);
        assert!(ts.needs_refresh_at(later));
    }
}
