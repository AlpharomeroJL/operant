//! Read side: [`MailStore`] is what `email.fetch`/`email.search` dispatch
//! against. [`FixtureMailStore`] is the "no real network" backend the
//! adapter is tested with: it parses every `*.eml` under a directory once
//! at construction (`contracts/fixtures/imap/` in tests), exactly the
//! "MailStore reading the fixtures dir" the brief asks for. A real IMAP
//! backend implements the same trait against a live connection later
//! without changing [`super::EmailAdapter`]'s public shape.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use super::parse::parse_eml;
use super::EmailError;

/// One parsed message, IMAP-fetch-shaped: promoted headers plus the raw
/// body text and the full header map for anything not promoted.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MailMessage {
    /// Store-assigned id. [`FixtureMailStore`] uses the `.eml` filename
    /// stem (`"01_invoice"`), stable across runs since fixtures are
    /// checked-in files.
    pub id: String,
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
}

/// Criteria for `email.search`. Every field is optional and criteria
/// combine with AND. The `_contains` fields are plain case-insensitive
/// substring checks; the `_matches` fields are regexes for callers that
/// need more than substring matching (mirrors `target.selectors[].
/// title_pattern`'s anchored-regex convention elsewhere in the IR).
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub from_contains: Option<String>,
    pub subject_contains: Option<String>,
    pub body_contains: Option<String>,
    pub subject_matches: Option<String>,
    pub body_matches: Option<String>,
    pub limit: Option<usize>,
}

impl SearchQuery {
    fn matches(&self, msg: &MailMessage) -> Result<bool, EmailError> {
        if let Some(needle) = &self.from_contains {
            if !contains_ci(&msg.from, needle) {
                return Ok(false);
            }
        }
        if let Some(needle) = &self.subject_contains {
            if !contains_ci(&msg.subject, needle) {
                return Ok(false);
            }
        }
        if let Some(needle) = &self.body_contains {
            if !contains_ci(&msg.body, needle) {
                return Ok(false);
            }
        }
        if let Some(pattern) = &self.subject_matches {
            if !compile(pattern)?.is_match(&msg.subject) {
                return Ok(false);
            }
        }
        if let Some(pattern) = &self.body_matches {
            if !compile(pattern)?.is_match(&msg.body) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn compile(pattern: &str) -> Result<Regex, EmailError> {
    Regex::new(pattern).map_err(|e| EmailError::BadPattern {
        pattern: pattern.to_string(),
        reason: e.to_string(),
    })
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// Read side an `email` adapter dispatches `fetch`/`search` against.
pub trait MailStore: Send + Sync {
    /// Every message id, in a stable, deterministic order.
    fn list(&self) -> Result<Vec<String>, EmailError>;

    fn fetch(&self, id: &str) -> Result<MailMessage, EmailError>;

    fn search(&self, query: &SearchQuery) -> Result<Vec<MailMessage>, EmailError>;
}

/// [`MailStore`] over a directory of `.eml` files, parsed once at
/// construction. Deterministic id order (sorted filename), so fixture
/// tests can assert on order without re-reading the directory per call.
pub struct FixtureMailStore {
    messages: Vec<MailMessage>,
}

impl FixtureMailStore {
    /// Parse every `*.eml` file directly under `dir` (not recursive).
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, EmailError> {
        let dir = dir.as_ref();
        let mut paths: Vec<_> = fs::read_dir(dir)
            .map_err(|source| EmailError::Io {
                path: dir.display().to_string(),
                source,
            })?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("eml"))
            .collect();
        paths.sort();

        let mut messages = Vec::with_capacity(paths.len());
        for path in paths {
            let id = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let raw = fs::read(&path).map_err(|source| EmailError::Io {
                path: path.display().to_string(),
                source,
            })?;
            messages.push(parse_eml(&id, &raw)?);
        }
        Ok(Self { messages })
    }

    /// `contracts/fixtures/imap` relative to the workspace root, resolved
    /// via `CARGO_MANIFEST_DIR` so it works regardless of the process's
    /// current directory.
    pub fn open_default_fixtures() -> Result<Self, EmailError> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/fixtures/imap");
        Self::open(dir)
    }
}

impl MailStore for FixtureMailStore {
    fn list(&self) -> Result<Vec<String>, EmailError> {
        Ok(self.messages.iter().map(|m| m.id.clone()).collect())
    }

    fn fetch(&self, id: &str) -> Result<MailMessage, EmailError> {
        self.messages
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| EmailError::NotFound(id.to_string()))
    }

    fn search(&self, query: &SearchQuery) -> Result<Vec<MailMessage>, EmailError> {
        let mut out = Vec::new();
        for msg in &self.messages {
            if query.matches(msg)? {
                out.push(msg.clone());
                if let Some(limit) = query.limit {
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> FixtureMailStore {
        FixtureMailStore::open_default_fixtures().expect("contracts/fixtures/imap parses")
    }

    #[test]
    fn opens_all_three_eml_fixtures_in_filename_order() {
        let store = fixtures();
        assert_eq!(
            store.list().unwrap(),
            vec!["01_invoice", "02_plain", "03_trigger"]
        );
    }

    #[test]
    fn fetch_by_id_returns_the_parsed_message() {
        let store = fixtures();
        let msg = store.fetch("01_invoice").unwrap();
        assert_eq!(msg.from, "billing@acme-fixture.example");
        assert_eq!(msg.subject, "Invoice 2026-07 for Acme Services");
        assert!(msg.body.contains("INV-2026-0711"));
        assert!(msg.body.contains("$142.50"));
    }

    #[test]
    fn fetch_unknown_id_is_a_typed_not_found() {
        let store = fixtures();
        assert!(matches!(
            store.fetch("nope").unwrap_err(),
            EmailError::NotFound(id) if id == "nope"
        ));
    }

    /// `03_trigger.eml` is the fixture the email-trigger consumer keys off
    /// (`contracts/fixtures/README.md`: "imap/*.eml | email adapter, email
    /// trigger"). This is the shape that consumer sees: sender, subject,
    /// and body fields it can match an invoice trigger against.
    #[test]
    fn trigger_fixture_parses_to_the_expected_shape() {
        let store = fixtures();
        let msg = store.fetch("03_trigger").unwrap();
        assert_eq!(msg.from, "billing@acme-fixture.example");
        assert_eq!(msg.to, vec!["owner@operant-fixture.example"]);
        assert!(msg.subject.contains("Invoice"));
        assert!(msg.body.contains("INV-2026-0811"));
        assert!(msg.body.contains("$98.00"));
        assert!(msg.message_id.is_some());
    }

    #[test]
    fn search_by_subject_substring_finds_both_invoices_not_the_newsletter() {
        let store = fixtures();
        let query = SearchQuery {
            subject_contains: Some("invoice".into()), // lowercase: case-insensitive
            ..Default::default()
        };
        let mut ids: Vec<_> = store
            .search(&query)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["01_invoice", "03_trigger"]);
    }

    #[test]
    fn search_by_body_regex_isolates_one_invoice_number() {
        let store = fixtures();
        let query = SearchQuery {
            body_matches: Some(r"INV-2026-0711".into()),
            ..Default::default()
        };
        let results = store.search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "01_invoice");
    }

    #[test]
    fn search_with_no_criteria_matches_everything_and_limit_caps_results() {
        let store = fixtures();
        let all = store.search(&SearchQuery::default()).unwrap();
        assert_eq!(all.len(), 3);

        let limited = store
            .search(&SearchQuery {
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[test]
    fn negative_case_the_plain_newsletter_never_matches_the_invoice_filter() {
        let store = fixtures();
        let query = SearchQuery {
            subject_contains: Some("invoice".into()),
            ..Default::default()
        };
        let ids: Vec<_> = store
            .search(&query)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert!(!ids.contains(&"02_plain".to_string()));
    }

    #[test]
    fn bad_regex_pattern_is_a_typed_error_not_a_panic() {
        let store = fixtures();
        let query = SearchQuery {
            subject_matches: Some("(unclosed".into()),
            ..Default::default()
        };
        assert!(matches!(
            store.search(&query).unwrap_err(),
            EmailError::BadPattern { .. }
        ));
    }
}
