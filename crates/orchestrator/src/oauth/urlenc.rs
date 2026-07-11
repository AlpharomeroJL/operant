//! Minimal `application/x-www-form-urlencoded`-compatible percent-encoding
//! for the query strings and token-endpoint bodies this module builds and
//! parses. Every value this flow ever sends is either random base64url
//! (already unreserved) or a fixed identifier, and every value it parses
//! comes from its own loopback listener or the provider it just called, so
//! this only has to be correct for that closed set, not general-purpose
//! fast. No new dependency: percent-encoding is a handful of lines.

use std::collections::HashMap;

const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";

pub fn encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.as_bytes() {
        if UNRESERVED.contains(b) {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&input[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse a `key=value&key=value` query string (with or without a leading
/// `?`) into a map. Last occurrence of a repeated key wins.
pub fn parse_query(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let query = query.strip_prefix('?').unwrap_or(query);
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(decode(k), decode(v));
    }
    map
}

/// Build a `key=value&key=value` query/body string, percent-encoding each
/// key and value.
pub fn build_query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_loopback_redirect_uri_through_encode_and_decode() {
        let raw = "http://127.0.0.1:54321/callback";
        let encoded = encode(raw);
        assert!(!encoded.contains(':'));
        assert_eq!(decode(&encoded), raw);
    }

    #[test]
    fn parse_query_handles_the_leading_question_mark_and_multiple_pairs() {
        let params = parse_query("?code=abc123&state=xyz&scope=model.complete");
        assert_eq!(params.get("code"), Some(&"abc123".to_string()));
        assert_eq!(params.get("state"), Some(&"xyz".to_string()));
        assert_eq!(params.get("scope"), Some(&"model.complete".to_string()));
    }

    #[test]
    fn parse_query_decodes_percent_and_plus() {
        let params = parse_query("redirect_uri=http%3A%2F%2F127.0.0.1%3A9%2Fcb&label=a+b");
        assert_eq!(
            params.get("redirect_uri"),
            Some(&"http://127.0.0.1:9/cb".to_string())
        );
        assert_eq!(params.get("label"), Some(&"a b".to_string()));
    }

    #[test]
    fn build_query_matches_parse_query_round_trip() {
        let pairs = [
            ("code_verifier", "abc~DEF-123_45.6"),
            ("state", "s p a c e"),
        ];
        let built = build_query(&pairs);
        let parsed = parse_query(&built);
        assert_eq!(
            parsed.get("code_verifier"),
            Some(&"abc~DEF-123_45.6".to_string())
        );
        assert_eq!(parsed.get("state"), Some(&"s p a c e".to_string()));
    }

    #[test]
    fn parse_query_on_an_empty_string_is_empty() {
        assert!(parse_query("").is_empty());
    }
}
