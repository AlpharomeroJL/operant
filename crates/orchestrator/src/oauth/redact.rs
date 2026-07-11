//! Secrets redaction: `docs/specs/backends.md`'s oauth-broker hard rule --
//! tokens "NEVER written to config or logs" -- backed by two independent
//! mechanisms, not one:
//!
//! 1. [`super::token::SecretString`] structurally keeps a raw token out of
//!    anything that derives `serde::Serialize`, so a token cannot land in
//!    a config file by accident; the compiler refuses the derive.
//! 2. [`SecretGuard`], this module: every oauth log line is built by
//!    calling [`SecretGuard::redact`] first (via [`SecretGuard::info`] /
//!    [`SecretGuard::warn`] / [`SecretGuard::error`], which the flow calls
//!    instead of the bare `tracing::*!` macros), so a raw token never
//!    reaches a `tracing` event at all, independent of which subscriber a
//!    caller installs. Reuses [`crate::backends::redact`]'s shape-based
//!    rules (`Bearer <token>`, `x-api-key: <token>`, `key=<token>`) for the
//!    shapes that overlap, and adds the OAuth wire fields (`access_token`,
//!    `refresh_token`, `id_token`, `code_verifier`, `client_secret`,
//!    `code`) in both form- and JSON-encoded shapes.

use std::collections::HashSet;
use std::sync::Mutex;

use super::token::SecretString;

/// Wire field names that carry a credential or credential-adjacent value,
/// redacted wherever they appear as `field=value` or `"field":"value"`,
/// registered or not.
const SENSITIVE_FIELDS: &[&str] = &[
    "access_token",
    "refresh_token",
    "id_token",
    "code_verifier",
    "client_secret",
    "code",
];

/// Registers live secrets as they are minted and scrubs them -- plus every
/// known OAuth wire-field shape, registered or not -- out of any string
/// before it reaches a log line or error message. One guard is shared for
/// the lifetime of a [`super::flow::Broker`].
#[derive(Default)]
pub struct SecretGuard {
    known: Mutex<HashSet<String>>,
}

impl SecretGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember a secret so every future [`SecretGuard::redact`] call
    /// scrubs it verbatim, however it appears (query param, JSON field,
    /// form field, bare).
    pub fn register(&self, secret: &SecretString) {
        self.register_str(secret.expose_secret());
    }

    pub fn register_str(&self, value: &str) {
        if value.is_empty() {
            return;
        }
        self.known.lock().unwrap().insert(value.to_string());
    }

    /// Scrub every registered secret plus every recognized OAuth
    /// wire-field shape out of `input`.
    pub fn redact(&self, input: &str) -> String {
        let known: Vec<String> = self.known.lock().unwrap().iter().cloned().collect();
        let known_refs: Vec<&str> = known.iter().map(String::as_str).collect();
        let mut out = crate::backends::redact(input, &known_refs);
        for field in SENSITIVE_FIELDS {
            out = redact_field(&out, field);
        }
        out
    }

    /// Redact, then log at `info`. Flow code calls this (never
    /// `tracing::info!` directly) for anything that might embed request or
    /// response detail.
    pub fn info(&self, msg: &str) {
        tracing::info!("{}", self.redact(msg));
    }

    pub fn warn(&self, msg: &str) {
        tracing::warn!("{}", self.redact(msg));
    }

    pub fn error(&self, msg: &str) {
        tracing::error!("{}", self.redact(msg));
    }
}

/// Redact `field=value` (form-encoded) and `"field":"value"` (JSON)
/// occurrences of one sensitive field name, everywhere they appear.
fn redact_field(input: &str, field: &str) -> String {
    let form_prefix = format!("{field}=");
    let json_prefix = format!("\"{field}\":\"");
    let out = redact_after(input, &form_prefix, &['&', '"', '\'']);
    redact_after(&out, &json_prefix, &['"'])
}

/// Replace the value immediately following `prefix` (up to the next
/// whitespace or any `stop` character) with `[REDACTED]`, everywhere
/// `prefix` appears.
fn redact_after(input: &str, prefix: &str, stop: &[char]) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find(prefix) {
        let (before, after_prefix_start) = rest.split_at(pos);
        out.push_str(before);
        out.push_str(prefix);
        let after = &after_prefix_start[prefix.len()..];
        let end = after
            .find(|c: char| c.is_whitespace() || stop.contains(&c))
            .unwrap_or(after.len());
        out.push_str("[REDACTED]");
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex as StdMutex};

    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    // Seeded, obviously-fake credentials: real-looking shapes, never real
    // values -- the "grep-tested against seeded fakes" convention this
    // codebase already uses in `backends::redact`.
    const SEEDED_ACCESS: &str = "at-seeded-fake-0000000000000000000000000000";
    const SEEDED_REFRESH: &str = "rt-seeded-fake-1111111111111111111111111111";

    #[test]
    fn redact_scrubs_a_registered_secret_wherever_it_appears() {
        let guard = SecretGuard::new();
        guard.register_str(SEEDED_ACCESS);
        let msg = format!("unexpected body: {{\"debug\":\"{SEEDED_ACCESS}\"}}");
        let clean = guard.redact(&msg);
        assert!(!clean.contains(SEEDED_ACCESS), "leaked: {clean}");
    }

    #[test]
    fn redact_scrubs_form_encoded_sensitive_fields_even_when_unregistered() {
        let guard = SecretGuard::new();
        let msg = format!(
            "POST /oauth/token body: grant_type=refresh_token&refresh_token={SEEDED_REFRESH}&client_id=x"
        );
        let clean = guard.redact(&msg);
        assert!(!clean.contains(SEEDED_REFRESH), "leaked: {clean}");
        assert!(
            clean.contains("client_id=x"),
            "surrounding text lost: {clean}"
        );
    }

    #[test]
    fn redact_scrubs_json_encoded_sensitive_fields_even_when_unregistered() {
        let guard = SecretGuard::new();
        let msg =
            format!("response: {{\"access_token\":\"{SEEDED_ACCESS}\",\"token_type\":\"Bearer\"}}");
        let clean = guard.redact(&msg);
        assert!(!clean.contains(SEEDED_ACCESS), "leaked: {clean}");
        assert!(clean.contains("token_type"));
    }

    #[test]
    fn redact_leaves_ordinary_status_code_text_alone() {
        let guard = SecretGuard::new();
        let msg = "provider returned status code 429, retry later";
        assert_eq!(guard.redact(msg), msg);
    }

    /// A `MakeWriter` that captures everything written to it into a shared
    /// buffer, so a test can inspect exactly what a subscriber would have
    /// sent to a log sink.
    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<StdMutex<Vec<u8>>>);

    impl io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// The grep-test bar (`docs/specs/backends.md`, X16 TESTS): tokens
    /// never appear in captured log output. Registers seeded fakes, emits
    /// them through `SecretGuard::error` under a real (scoped, not global)
    /// `tracing` subscriber writing to an in-memory buffer, then greps the
    /// captured bytes.
    #[test]
    fn seeded_fake_tokens_never_appear_in_captured_log_output() {
        let guard = SecretGuard::new();
        guard.register_str(SEEDED_ACCESS);
        guard.register_str(SEEDED_REFRESH);

        let writer = CaptureWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer.clone())
            .with_ansi(false)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            guard.error(&format!(
                "token refresh failed: access_token={SEEDED_ACCESS} refresh_token={SEEDED_REFRESH} (http 400)"
            ));
            guard.info("unrelated informational line stays intact");
        });

        let captured = String::from_utf8(writer.0.lock().unwrap().clone()).unwrap();
        assert!(
            !captured.contains(SEEDED_ACCESS),
            "leaked into logs: {captured}"
        );
        assert!(
            !captured.contains(SEEDED_REFRESH),
            "leaked into logs: {captured}"
        );
        assert!(
            captured.contains("[REDACTED]"),
            "redaction marker missing: {captured}"
        );
        assert!(captured.contains("unrelated informational line stays intact"));
    }
}
