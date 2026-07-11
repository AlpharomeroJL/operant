//! Secrets redaction middleware (`contracts/model_backend.md` hard rule
//! #2): "strips keys and tokens from every log line and error." Applied in
//! `client.rs` before any request URL or failed-response body reaches a
//! `tracing` call or a `BackendError` message.

/// Redact anything that looks like a credential from a string that may be
/// logged or surfaced in an error: `Bearer <token>` / `bearer <token>`
/// authorization headers, `x-api-key: <token>`, `key=<token>` query
/// parameters (Gemini's auth shape), and any caller-supplied literal secret
/// (a backend always knows its own configured key, so it can redact that
/// exact value even where it does not match one of the shape-based rules,
/// e.g. inside an echoed request body in an error message).
pub fn redact(input: &str, known_secrets: &[&str]) -> String {
    let mut out = input.to_string();
    for secret in known_secrets {
        if secret.is_empty() {
            continue;
        }
        out = out.replace(*secret, "[REDACTED]");
    }
    out = redact_after_prefix(&out, "Bearer ");
    out = redact_after_prefix(&out, "bearer ");
    out = redact_after_prefix(&out, "x-api-key: ");
    out = redact_after_prefix(&out, "x-api-key=");
    out = redact_query_param(&out, "key=");
    out
}

/// Replace the token immediately following `prefix` (up to the next
/// whitespace, `&`, or quote) with `[REDACTED]`, everywhere `prefix`
/// appears.
fn redact_after_prefix(input: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(pos) = rest.find(prefix) {
        let (before, after_prefix_start) = rest.split_at(pos);
        out.push_str(before);
        out.push_str(prefix);
        let after = &after_prefix_start[prefix.len()..];
        let token_end = after
            .find(|c: char| c.is_whitespace() || c == '&' || c == '"' || c == '\'')
            .unwrap_or(after.len());
        out.push_str("[REDACTED]");
        rest = &after[token_end..];
    }
    out.push_str(rest);
    out
}

/// Replace the value of a `marker=value` query parameter (e.g. `key=...`)
/// with `[REDACTED]`, everywhere `marker` appears.
fn redact_query_param(input: &str, marker: &str) -> String {
    redact_after_prefix(input, marker)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fake, obviously-seeded credentials: real-looking shapes, never real
    // values, exactly what the contract's "grep-tested against seeded
    // fakes" calls for.
    const SEEDED_BEARER: &str = "sk-live-seeded-fake-0000000000000000";
    const SEEDED_GEMINI_KEY: &str = "AIzaSeededFakeKey00000000000000000";
    const SEEDED_ANTHROPIC_KEY: &str = "sk-ant-seeded-fake-1111111111111111";

    #[test]
    fn strips_bearer_token_from_error_text() {
        let msg = format!("request failed: Authorization: Bearer {SEEDED_BEARER} rejected");
        let clean = redact(&msg, &[]);
        assert!(!clean.contains(SEEDED_BEARER), "leaked: {clean}");
        assert!(clean.contains("[REDACTED]"));
        assert!(
            clean.contains("rejected"),
            "surrounding text must survive: {clean}"
        );
    }

    #[test]
    fn strips_x_api_key_header_value() {
        let msg = format!("sending headers: x-api-key: {SEEDED_ANTHROPIC_KEY}\nother: header");
        let clean = redact(&msg, &[]);
        assert!(!clean.contains(SEEDED_ANTHROPIC_KEY), "leaked: {clean}");
        assert!(clean.contains("other: header"));
    }

    #[test]
    fn strips_gemini_style_query_param_key() {
        let msg = format!("GET https://generativelanguage.googleapis.com/v1/models?key={SEEDED_GEMINI_KEY} -> 200");
        let clean = redact(&msg, &[]);
        assert!(!clean.contains(SEEDED_GEMINI_KEY), "leaked: {clean}");
        assert!(clean.contains("-> 200"));
    }

    #[test]
    fn strips_known_configured_key_anywhere_it_appears_even_without_a_recognized_shape() {
        let msg = format!("echoed request body: {{\"debug_key\":\"{SEEDED_BEARER}\"}}");
        let clean = redact(&msg, &[SEEDED_BEARER]);
        assert!(!clean.contains(SEEDED_BEARER), "leaked: {clean}");
    }

    #[test]
    fn leaves_ordinary_text_untouched() {
        let msg = "backend openai returned HTTP 429: rate limited, retry after 2s";
        assert_eq!(redact(msg, &[]), msg);
    }

    #[test]
    fn redacts_every_occurrence_not_just_the_first() {
        let msg = format!("Bearer {SEEDED_BEARER} then again Bearer {SEEDED_BEARER}");
        let clean = redact(&msg, &[]);
        assert!(!clean.contains(SEEDED_BEARER));
        assert_eq!(clean.matches("[REDACTED]").count(), 2);
    }
}
