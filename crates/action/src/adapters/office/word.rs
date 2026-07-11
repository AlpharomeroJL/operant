//! `word` namespace adapter: open, get text, replace text, save.
//! Generic over [`WordBackend`] so tests run against [`MockWordBackend`]
//! (never touches disk or COM) while production wires in the real COM
//! backend behind the `office-com` feature.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use operant_ir::RiskClass;
use parking_lot::Mutex;
use serde_json::json;

use super::OfficeError;
use crate::adapter::{Adapter, AdapterError, Idempotency, VerbSpec};

const NAMESPACE: &str = "word";

pub type DocumentId = u64;

/// What the `word` adapter needs from a Word automation backend.
/// `docs/specs/action.md`: "each releasing COM objects deterministically"
/// is [`WordBackend::close_document`]'s job for the real COM
/// implementation; the mock has nothing to release.
pub trait WordBackend: Send + Sync {
    fn open_document(&self, path: &str) -> Result<DocumentId, OfficeError>;
    fn get_text(&self, document: DocumentId) -> Result<String, OfficeError>;
    /// Replace every occurrence of `find` with `replace`. Returns the
    /// number of replacements made.
    fn replace_text(
        &self,
        document: DocumentId,
        find: &str,
        replace: &str,
    ) -> Result<u32, OfficeError>;
    /// Returns the path actually saved to.
    fn save_document(
        &self,
        document: DocumentId,
        path: Option<&str>,
    ) -> Result<String, OfficeError>;
    fn close_document(&self, document: DocumentId) -> Result<(), OfficeError>;
}

struct MockDocument {
    path: String,
    text: String,
    saved_to: Vec<String>,
}

/// In-memory [`WordBackend`]: no disk I/O, no COM. What every test in
/// this crate runs `word.*` verbs against. Since it never parses a real
/// `.docx`, [`MockWordBackend::seed`] is how a test gives `open_document`
/// something to return.
#[derive(Default)]
pub struct MockWordBackend {
    next_id: AtomicU64,
    documents: Mutex<HashMap<DocumentId, MockDocument>>,
    seeds: Mutex<HashMap<String, String>>,
}

impl MockWordBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test/demo helper: pre-populate what `open_document(path)` returns.
    pub fn seed(&self, path: &str, text: &str) {
        self.seeds.lock().insert(path.to_string(), text.to_string());
    }

    /// Every path `save_document` was called with, in call order.
    pub fn saved_paths(&self, document: DocumentId) -> Vec<String> {
        self.documents
            .lock()
            .get(&document)
            .map(|d| d.saved_to.clone())
            .unwrap_or_default()
    }
}

impl WordBackend for MockWordBackend {
    fn open_document(&self, path: &str) -> Result<DocumentId, OfficeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let text = self.seeds.lock().get(path).cloned().unwrap_or_default();
        self.documents.lock().insert(
            id,
            MockDocument {
                path: path.to_string(),
                text,
                saved_to: Vec::new(),
            },
        );
        Ok(id)
    }

    fn get_text(&self, document: DocumentId) -> Result<String, OfficeError> {
        self.documents
            .lock()
            .get(&document)
            .map(|d| d.text.clone())
            .ok_or(OfficeError::UnknownDocument(document))
    }

    fn replace_text(
        &self,
        document: DocumentId,
        find: &str,
        replace: &str,
    ) -> Result<u32, OfficeError> {
        let mut docs = self.documents.lock();
        let doc = docs
            .get_mut(&document)
            .ok_or(OfficeError::UnknownDocument(document))?;
        if find.is_empty() {
            return Ok(0);
        }
        let count = doc.text.matches(find).count() as u32;
        doc.text = doc.text.replace(find, replace);
        Ok(count)
    }

    fn save_document(
        &self,
        document: DocumentId,
        path: Option<&str>,
    ) -> Result<String, OfficeError> {
        let mut docs = self.documents.lock();
        let doc = docs
            .get_mut(&document)
            .ok_or(OfficeError::UnknownDocument(document))?;
        let target = path.map(String::from).unwrap_or_else(|| doc.path.clone());
        doc.saved_to.push(target.clone());
        Ok(target)
    }

    fn close_document(&self, document: DocumentId) -> Result<(), OfficeError> {
        self.documents
            .lock()
            .remove(&document)
            .map(|_| ())
            .ok_or(OfficeError::UnknownDocument(document))
    }
}

/// `word` namespace adapter.
pub struct WordAdapter {
    verbs: Vec<VerbSpec>,
    backend: Arc<dyn WordBackend>,
}

impl WordAdapter {
    pub fn new(backend: Arc<dyn WordBackend>) -> Self {
        Self {
            verbs: build_verbs(),
            backend,
        }
    }

    /// Convenience for tests: an adapter over a fresh [`MockWordBackend`],
    /// with a handle to that same backend kept alongside so the test can
    /// seed a document or assert on `saved_paths`.
    pub fn mock() -> (Self, Arc<MockWordBackend>) {
        let backend = Arc::new(MockWordBackend::new());
        (Self::new(backend.clone()), backend)
    }

    fn call_inner(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, OfficeError> {
        match verb {
            "open" => self.open(args),
            "get_text" => self.get_text(args),
            "replace_text" => self.replace_text(args),
            "save" => self.save(args),
            "close" => self.close(args),
            other => unreachable!(
                "AdapterRegistry only dispatches verbs WordAdapter::verbs() declared, got `{other}`"
            ),
        }
    }

    fn open(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let path = str_arg(args, "path")?;
        let id = self.backend.open_document(path)?;
        Ok(json!({ "document": id.to_string() }))
    }

    fn get_text(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let document = handle_arg(args, "document")?;
        let text = self.backend.get_text(document)?;
        Ok(json!({ "text": text }))
    }

    fn replace_text(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let document = handle_arg(args, "document")?;
        let find = str_arg(args, "find")?;
        let replace = args.get("replace").and_then(|v| v.as_str()).unwrap_or("");
        let replacements = self.backend.replace_text(document, find, replace)?;
        Ok(json!({ "replacements": replacements }))
    }

    fn save(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let document = handle_arg(args, "document")?;
        let path = args.get("path").and_then(|v| v.as_str());
        let saved_to = self.backend.save_document(document, path)?;
        Ok(json!({ "saved_to": saved_to }))
    }

    fn close(&self, args: &serde_json::Value) -> Result<serde_json::Value, OfficeError> {
        let document = handle_arg(args, "document")?;
        self.backend.close_document(document)?;
        Ok(json!({ "ok": true }))
    }
}

impl Adapter for WordAdapter {
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

fn str_arg<'a>(args: &'a serde_json::Value, key: &'static str) -> Result<&'a str, OfficeError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or(OfficeError::MissingArg(key))
}

fn handle_arg(args: &serde_json::Value, key: &'static str) -> Result<u64, OfficeError> {
    let raw = str_arg(args, key)?;
    raw.parse::<u64>()
        .map_err(|_| OfficeError::BadHandle(raw.to_string()))
}

fn build_verbs() -> Vec<VerbSpec> {
    let handle_prop = json!({ "type": "string", "minLength": 1 });
    vec![
        VerbSpec::new(
            "open",
            json!({
                "type": "object",
                "required": ["path"],
                "properties": { "path": { "type": "string", "minLength": 1 } },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "get_text",
            json!({
                "type": "object",
                "required": ["document"],
                "properties": { "document": handle_prop },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "replace_text",
            json!({
                "type": "object",
                "required": ["document", "find"],
                "properties": {
                    "document": handle_prop,
                    "find": { "type": "string", "minLength": 1 },
                    "replace": { "type": "string", "default": "" }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            // Applying the same find/replace twice finds nothing left to
            // replace the second time (assuming `replace` does not itself
            // reintroduce `find`, the common case): same end state.
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "save",
            json!({
                "type": "object",
                "required": ["document"],
                "properties": {
                    "document": handle_prop,
                    "path": { "type": "string", "minLength": 1 }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "close",
            json!({
                "type": "object",
                "required": ["document"],
                "properties": { "document": handle_prop },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Unknown,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_get_replace_save_round_trip_on_the_mock() {
        let (adapter, backend) = WordAdapter::mock();
        backend.seed(
            "letter.docx",
            "Dear [Name], your invoice INV-2026-0711 is due.",
        );

        let open = adapter
            .call("open", &json!({ "path": "letter.docx" }))
            .unwrap();
        let document = open["document"].as_str().unwrap().to_string();

        let got = adapter
            .call("get_text", &json!({ "document": document }))
            .unwrap();
        assert_eq!(
            got["text"],
            json!("Dear [Name], your invoice INV-2026-0711 is due.")
        );

        let replaced = adapter
            .call(
                "replace_text",
                &json!({ "document": document, "find": "[Name]", "replace": "Acme Co" }),
            )
            .unwrap();
        assert_eq!(replaced["replacements"], json!(1));

        let got = adapter
            .call("get_text", &json!({ "document": document }))
            .unwrap();
        assert_eq!(
            got["text"],
            json!("Dear Acme Co, your invoice INV-2026-0711 is due.")
        );

        let save = adapter
            .call("save", &json!({ "document": document }))
            .unwrap();
        assert_eq!(save["saved_to"], json!("letter.docx"));
        assert_eq!(
            backend.saved_paths(document.parse().unwrap()),
            vec!["letter.docx".to_string()]
        );
    }

    #[test]
    fn replace_text_of_something_absent_makes_zero_replacements() {
        let (adapter, backend) = WordAdapter::mock();
        backend.seed("x.docx", "no placeholders here");
        let open = adapter.call("open", &json!({ "path": "x.docx" })).unwrap();
        let document = open["document"].as_str().unwrap().to_string();
        let replaced = adapter
            .call(
                "replace_text",
                &json!({ "document": document, "find": "[Name]", "replace": "Acme" }),
            )
            .unwrap();
        assert_eq!(replaced["replacements"], json!(0));
    }

    #[test]
    fn unopened_document_handle_is_a_typed_error() {
        let (adapter, _backend) = WordAdapter::mock();
        let err = adapter
            .call("get_text", &json!({ "document": "999" }))
            .unwrap_err();
        assert!(matches!(err, AdapterError::CallFailed { .. }));
    }

    #[test]
    fn round_trips_through_action_ir_with_replace_gated_as_write_risk() {
        use crate::{AdapterRegistry, Executor, MockSynthesizer, NoopSleeper};
        use operant_ir::{Action, ActionKind, Grounding, Pace, Retry};

        let (adapter, backend) = WordAdapter::mock();
        backend.seed("template.docx", "Hello [Name]");
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(adapter));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("word"));
        params.insert("verb".into(), json!("open"));
        params.insert("args".into(), json!({ "path": "template.docx" }));
        let open_action = Action {
            v: 1,
            id: "word-open".into(),
            kind: ActionKind::AdapterCall,
            intent: None,
            target: None,
            params,
            pace: Pace::Instant,
            risk_class: RiskClass::Read,
            irreversible: false,
            grounding: Grounding::Adapter,
            timeout_ms: 5000,
            retry: Retry {
                attempts: 0,
                backoff_ms: 0,
            },
        };
        let outcome = exec.execute(&open_action, None, None).unwrap();
        let document = outcome.adapter_result.unwrap()["document"]
            .as_str()
            .unwrap()
            .to_string();

        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("word"));
        params.insert("verb".into(), json!("replace_text"));
        params.insert(
            "args".into(),
            json!({ "document": document, "find": "[Name]", "replace": "Josef" }),
        );
        let replace_action = Action {
            risk_class: RiskClass::Write,
            id: "word-replace".into(),
            params,
            ..open_action.clone()
        };
        let outcome = exec.execute(&replace_action, None, None).unwrap();
        assert_eq!(outcome.adapter_result.unwrap()["replacements"], json!(1));
    }

    #[test]
    fn namespace_and_verbs_match_the_action_ir_contract() {
        let (adapter, _backend) = WordAdapter::mock();
        assert_eq!(adapter.namespace(), "word");
        let names: Vec<_> = adapter.verbs().iter().map(|v| v.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["open", "get_text", "replace_text", "save", "close"]
        );
    }
}
