//! Minimal RFC 5322 header parsing for the `.eml` fixtures under
//! `contracts/fixtures/imap/`. Handles what real IMAP-delivered mail and
//! these fixtures both need: folded headers, a header block terminated by
//! the first blank line, and an optional `quoted-printable`/`base64`
//! `Content-Transfer-Encoding` on the body. Multipart MIME is out of
//! scope (see FOLLOWUPS in `RESULT.md`); every fixture here is a single
//! `text/plain` part.

use std::collections::BTreeMap;

use super::store::MailMessage;
use super::EmailError;

/// Parse one `.eml` file's raw bytes into a [`MailMessage`]. `id` is the
/// caller-assigned message id (a [`super::store::FixtureMailStore`] uses
/// the filename stem).
pub fn parse_eml(id: &str, raw: &[u8]) -> Result<MailMessage, EmailError> {
    let text = String::from_utf8_lossy(raw);
    let (header_block, body_raw) = split_headers_and_body(&text);
    let headers = parse_headers(header_block);

    let from = headers
        .get("from")
        .cloned()
        .ok_or_else(|| EmailError::MalformedMessage {
            id: id.to_string(),
            reason: "missing From header".into(),
        })?;
    let to = headers
        .get("to")
        .map(|v| split_addresses(v))
        .unwrap_or_default();
    let subject = headers.get("subject").cloned().unwrap_or_default();
    let date = headers.get("date").cloned().unwrap_or_default();
    let message_id = headers.get("message-id").cloned();

    let body = decode_body(
        body_raw,
        headers.get("content-transfer-encoding").map(String::as_str),
    );

    Ok(MailMessage {
        id: id.to_string(),
        from,
        to,
        subject,
        date,
        message_id,
        headers,
        body,
    })
}

/// Split on the first CRLF-CRLF or LF-LF blank line. Everything before is
/// the header block (unfolded below); everything after is the raw body.
fn split_headers_and_body(text: &str) -> (&str, &str) {
    if let Some(idx) = text.find("\r\n\r\n") {
        (&text[..idx], &text[idx + 4..])
    } else if let Some(idx) = text.find("\n\n") {
        (&text[..idx], &text[idx + 2..])
    } else {
        (text, "")
    }
}

/// RFC 5322 3.2.2 header folding: a line starting with space or tab is a
/// continuation of the previous header's value. Keys are lowercased so
/// lookups are case-insensitive per the RFC; the fixtures and real mail
/// both mix case (`Message-ID` vs `message-id`).
fn parse_headers(block: &str) -> BTreeMap<String, String> {
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    let mut last_key: Option<String> = None;

    for raw_line in block.split(['\n']) {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(existing) = last_key.as_ref().and_then(|key| headers.get_mut(key)) {
                existing.push(' ');
                existing.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            let key = name.trim().to_ascii_lowercase();
            headers.insert(key.clone(), value.trim().to_string());
            last_key = Some(key);
        }
    }
    headers
}

/// Split a `To`/`Cc`-style comma-separated address list. Does not attempt
/// full RFC 5322 mailbox grammar (quoted display names with embedded
/// commas); every fixture and the common case are bare or
/// `Name <addr>`-shaped addresses without quoted commas.
fn split_addresses(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn decode_body(raw: &str, encoding: Option<&str>) -> String {
    match encoding.map(|e| e.trim().to_ascii_lowercase()) {
        Some(ref enc) if enc == "base64" => {
            use base64::engine::general_purpose::STANDARD as BASE64;
            use base64::Engine as _;
            let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            match BASE64.decode(cleaned) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                // Malformed base64 body: surface the raw text rather than
                // failing the whole message parse over one bad part.
                Err(_) => raw.to_string(),
            }
        }
        // `quoted-printable` and the default `7bit`/`8bit`/absent case are
        // both left as-is: every fixture is plain 7-bit text, and a
        // conservative pass-through never corrupts already-plain bodies.
        // Decoding quoted-printable soft line breaks is a FOLLOWUP.
        _ => raw.trim_end_matches(['\r', '\n']).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_invoice_fixture_shape() {
        let raw = b"From: billing@acme-fixture.example\r\n\
To: owner@operant-fixture.example\r\n\
Subject: Invoice 2026-07 for Acme Services\r\n\
Date: Fri, 10 Jul 2026 09:15:00 +0000\r\n\
Message-ID: <fixture-invoice-001@acme-fixture.example>\r\n\
MIME-Version: 1.0\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Hello,\r\n\
\r\n\
Invoice number: INV-2026-0711\r\n\
Amount due: $142.50\r\n";
        let msg = parse_eml("01_invoice", raw).unwrap();
        assert_eq!(msg.from, "billing@acme-fixture.example");
        assert_eq!(msg.to, vec!["owner@operant-fixture.example"]);
        assert_eq!(msg.subject, "Invoice 2026-07 for Acme Services");
        assert_eq!(
            msg.message_id.as_deref(),
            Some("<fixture-invoice-001@acme-fixture.example>")
        );
        assert!(msg.body.contains("INV-2026-0711"));
        assert!(msg.body.contains("$142.50"));
    }

    #[test]
    fn folded_header_lines_are_joined() {
        let raw =
            b"From: a@example.com\r\nSubject: line one\r\n continued line two\r\n\r\nbody\r\n";
        let msg = parse_eml("fold", raw).unwrap();
        assert_eq!(msg.subject, "line one continued line two");
    }

    #[test]
    fn missing_from_header_is_a_typed_error() {
        let raw = b"Subject: no sender\r\n\r\nbody";
        let err = parse_eml("bad", raw).unwrap_err();
        assert!(matches!(err, EmailError::MalformedMessage { .. }));
    }

    #[test]
    fn base64_body_is_decoded() {
        let raw = format!(
            "From: a@example.com\r\nContent-Transfer-Encoding: base64\r\n\r\n{}",
            "aGVsbG8gd29ybGQ=" // "hello world"
        );
        let msg = parse_eml("b64", raw.as_bytes()).unwrap();
        assert_eq!(msg.body, "hello world");
    }
}
