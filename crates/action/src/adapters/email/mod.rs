//! `email` namespace adapter: IMAP-shaped fetch/search plus SMTP send
//! (`docs/specs/action.md`: "Email: IMAP fetch/search and SMTP send (send
//! is irreversible, labeled)").
//!
//! - [`store`]: the [`MailStore`] read side ([`FixtureMailStore`] parses
//!   `.eml` files off disk; a real IMAP backend implements the same
//!   trait).
//! - [`send`]: the [`Mailer`] write side ([`RecordingMailer`] is what
//!   every test runs against; [`SmtpMailer`] is a minimal real client).
//! - [`parse`]: the RFC 5322 header parser both fixtures and real mail
//!   need.

pub mod parse;
pub mod send;
pub mod store;

use std::path::Path;
use std::sync::Arc;

use operant_ir::RiskClass;
use serde_json::json;
use thiserror::Error;

use crate::adapter::{Adapter, AdapterError, Idempotency, VerbSpec};

pub use send::{Mailer, OutboundMessage, RecordingMailer, SendReceipt, SmtpMailer};
pub use store::{FixtureMailStore, MailMessage, MailStore, SearchQuery};

const NAMESPACE: &str = "email";

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("io error on `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("message `{0}` not found")]
    NotFound(String),
    #[error("`{id}` is not a well-formed message: {reason}")]
    MalformedMessage { id: String, reason: String },
    #[error("bad search pattern `{pattern}`: {reason}")]
    BadPattern { pattern: String, reason: String },
    #[error("smtp error: {0}")]
    Smtp(String),
    #[error("missing required argument `{0}`")]
    MissingArg(&'static str),
}

/// `email` namespace adapter. Read side ([`MailStore`]) and write side
/// ([`Mailer`]) are injected traits so production (real IMAP/SMTP) and
/// tests (fixtures, in-memory recording) share one adapter implementation.
pub struct EmailAdapter {
    verbs: Vec<VerbSpec>,
    store: Arc<dyn MailStore>,
    mailer: Arc<dyn Mailer>,
}

impl EmailAdapter {
    pub fn new(store: Arc<dyn MailStore>, mailer: Arc<dyn Mailer>) -> Self {
        Self {
            verbs: build_verbs(),
            store,
            mailer,
        }
    }

    /// Convenience for tests and fixture-driven demos: a
    /// [`FixtureMailStore`] over `dir` plus a [`RecordingMailer`] that
    /// never touches the network. Tests that need to inspect what was
    /// sent should build their own `Arc<RecordingMailer>` and pass it to
    /// [`EmailAdapter::new`] directly instead, so they keep a handle to it.
    pub fn with_fixture_store(dir: impl AsRef<Path>) -> Result<Self, EmailError> {
        Ok(Self::new(
            Arc::new(FixtureMailStore::open(dir)?),
            Arc::new(RecordingMailer::new()),
        ))
    }

    fn call_inner(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, EmailError> {
        match verb {
            "fetch" => self.fetch(args),
            "search" => self.search(args),
            "send" => self.send(args),
            other => unreachable!(
                "AdapterRegistry only dispatches verbs EmailAdapter::verbs() declared, got `{other}`"
            ),
        }
    }

    fn fetch(&self, args: &serde_json::Value) -> Result<serde_json::Value, EmailError> {
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let messages = if let Some(id) = args.get("id").and_then(|v| v.as_str()) {
            vec![self.store.fetch(id)?]
        } else {
            let mut ids = self.store.list()?;
            if let Some(limit) = limit {
                ids.truncate(limit);
            }
            ids.iter()
                .map(|id| self.store.fetch(id))
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(json!({ "messages": messages.iter().map(message_json).collect::<Vec<_>>() }))
    }

    fn search(&self, args: &serde_json::Value) -> Result<serde_json::Value, EmailError> {
        let query = SearchQuery {
            from_contains: str_arg(args, "from_contains"),
            subject_contains: str_arg(args, "subject_contains"),
            body_contains: str_arg(args, "body_contains"),
            subject_matches: str_arg(args, "subject_matches"),
            body_matches: str_arg(args, "body_matches"),
            limit: args
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize),
        };
        let messages = self.store.search(&query)?;
        Ok(json!({ "messages": messages.iter().map(message_json).collect::<Vec<_>>() }))
    }

    fn send(&self, args: &serde_json::Value) -> Result<serde_json::Value, EmailError> {
        let from = args
            .get("from")
            .and_then(|v| v.as_str())
            .ok_or(EmailError::MissingArg("from"))?
            .to_string();
        let to: Vec<String> = args
            .get("to")
            .and_then(|v| v.as_array())
            .ok_or(EmailError::MissingArg("to"))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let subject = args
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or(EmailError::MissingArg("subject"))?
            .to_string();
        let body = args
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or(EmailError::MissingArg("body"))?
            .to_string();

        let receipt = self.mailer.send(&OutboundMessage {
            from,
            to,
            subject,
            body,
        })?;
        Ok(json!({
            "accepted_to": receipt.accepted_to,
            "message_id": receipt.message_id
        }))
    }
}

impl Adapter for EmailAdapter {
    fn namespace(&self) -> &str {
        NAMESPACE
    }

    fn verbs(&self) -> &[VerbSpec] {
        &self.verbs
    }

    fn call(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, AdapterError> {
        self.call_inner(verb, args)
            .map_err(|e| AdapterError::CallFailed {
                namespace: NAMESPACE.to_string(),
                verb: verb.to_string(),
                message: e.to_string(),
            })
    }
}

fn str_arg(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn message_json(m: &MailMessage) -> serde_json::Value {
    json!({
        "id": m.id,
        "from": m.from,
        "to": m.to,
        "subject": m.subject,
        "date": m.date,
        "message_id": m.message_id,
        "body": m.body,
        "headers": m.headers,
    })
}

fn build_verbs() -> Vec<VerbSpec> {
    vec![
        VerbSpec::new(
            "fetch",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "minLength": 1 },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "search",
            json!({
                "type": "object",
                "properties": {
                    "from_contains": { "type": "string" },
                    "subject_contains": { "type": "string" },
                    "body_contains": { "type": "string" },
                    "subject_matches": { "type": "string" },
                    "body_matches": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "send",
            json!({
                "type": "object",
                "required": ["from", "to", "subject", "body"],
                "properties": {
                    "from": { "type": "string", "minLength": 1 },
                    "to": {
                        "type": "array",
                        "items": { "type": "string", "minLength": 1 },
                        "minItems": 1
                    },
                    "subject": { "type": "string" },
                    "body": { "type": "string" }
                },
                "additionalProperties": false
            }),
            RiskClass::Destructive,
            // Matches crate::adapter's own doc example for this enum
            // variant to the letter: retrying `email.send` may duplicate
            // the side effect (a second message lands in the recipient's
            // inbox), so it never auto-retries.
            Idempotency::NotIdempotent,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdapterRegistry, Approval, Executor, MockSynthesizer, NoopSleeper};
    use operant_ir::{Action, ActionKind, Grounding, Pace, Retry};

    fn fixtures_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/fixtures/imap")
    }

    fn adapter_call_action(
        id: &str,
        verb: &str,
        args: serde_json::Value,
        risk: RiskClass,
    ) -> Action {
        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("email"));
        params.insert("verb".into(), json!(verb));
        params.insert("args".into(), args);
        Action {
            v: 1,
            id: id.into(),
            kind: ActionKind::AdapterCall,
            intent: None,
            target: None,
            params,
            pace: Pace::Instant,
            risk_class: risk,
            irreversible: risk == RiskClass::Destructive,
            grounding: Grounding::Adapter,
            timeout_ms: 5000,
            retry: Retry {
                attempts: 0,
                backoff_ms: 0,
            },
        }
    }

    #[test]
    fn fetch_round_trips_the_invoice_fixture_through_action_ir() {
        let adapter = EmailAdapter::with_fixture_store(fixtures_dir()).unwrap();
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(adapter));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let action = adapter_call_action(
            "fetch-1",
            "fetch",
            json!({ "id": "01_invoice" }),
            RiskClass::Read,
        );
        let outcome = exec.execute(&action, None, None).unwrap();
        let result = outcome.adapter_result.unwrap();
        let messages = result["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["from"], json!("billing@acme-fixture.example"));
        assert!(messages[0]["body"]
            .as_str()
            .unwrap()
            .contains("INV-2026-0711"));
    }

    #[test]
    fn search_round_trips_through_action_ir_and_excludes_the_negative_case() {
        let adapter = EmailAdapter::with_fixture_store(fixtures_dir()).unwrap();
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(adapter));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let action = adapter_call_action(
            "search-1",
            "search",
            json!({ "subject_contains": "Invoice" }),
            RiskClass::Read,
        );
        let outcome = exec.execute(&action, None, None).unwrap();
        let messages = outcome.adapter_result.unwrap()["messages"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(messages, 2, "both invoices match, the newsletter must not");
    }

    #[test]
    fn send_is_destructive_and_refused_without_approval_through_the_executor() {
        let mailer = Arc::new(RecordingMailer::new());
        let store = Arc::new(FixtureMailStore::open(fixtures_dir()).unwrap());
        let adapter = EmailAdapter::new(store, mailer.clone());
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(adapter));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let action = adapter_call_action(
            "send-1",
            "send",
            json!({
                "from": "owner@operant-fixture.example",
                "to": ["customer@example.com"],
                "subject": "Re: invoice",
                "body": "Paid, thanks."
            }),
            RiskClass::Destructive,
        );

        let refused = exec.execute(&action, None, None).unwrap_err();
        assert!(matches!(
            refused,
            crate::ActionError::ApprovalRequired { .. }
        ));
        assert!(
            mailer.sent().is_empty(),
            "a refused send must not touch the mailer"
        );

        let approval = Approval::for_action("send-1", "josef");
        let outcome = exec.execute(&action, None, Some(&approval)).unwrap();
        let result = outcome.adapter_result.unwrap();
        assert_eq!(result["accepted_to"], json!(["customer@example.com"]));
        assert_eq!(mailer.sent().len(), 1);
    }

    #[test]
    fn schema_rejects_send_missing_a_required_field() {
        let adapter = EmailAdapter::with_fixture_store(fixtures_dir()).unwrap();
        let mut reg = AdapterRegistry::new();
        reg.register(Box::new(adapter));
        let err = reg
            .validate(
                "email",
                "send",
                &json!({ "to": ["a@example.com"], "subject": "hi", "body": "hi" }), // missing `from`
            )
            .unwrap_err();
        assert!(matches!(err, AdapterError::SchemaValidation { .. }));
    }

    #[test]
    fn namespace_and_verbs_match_the_action_ir_contract() {
        let adapter = EmailAdapter::with_fixture_store(fixtures_dir()).unwrap();
        assert_eq!(adapter.namespace(), "email");
        let names: Vec<_> = adapter.verbs().iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["fetch", "search", "send"]);
    }
}
