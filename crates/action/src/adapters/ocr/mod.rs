//! `ocr` namespace adapter: on-device text plus word-box extraction from
//! PDF and PNG (`docs/specs/action.md`: "OCR/PDF: extract text plus word
//! bounding boxes from images and PDFs, on device"). One verb, `extract`,
//! dispatches to [`pdf::extract`] or the [`png`]/[`glyphs`] pipeline by
//! file extension (or an explicit `kind` override).
//!
//! - [`pdf`]: minimal content-stream text extractor.
//! - [`png`]: minimal PNG decoder (the `ocr` cargo feature's `miniz_oxide`
//!   dependency lives here).
//! - [`glyphs`]: bitmap glyph segmentation plus a small built-in
//!   template classifier for the decoded image.

pub mod glyphs;
pub mod pdf;
pub mod png;

use std::path::Path;

use operant_ir::RiskClass;
use serde_json::json;
use thiserror::Error;

use crate::adapter::{Adapter, AdapterError, Idempotency, VerbSpec};

const NAMESPACE: &str = "ocr";

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("io error on `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("missing required argument `{0}`")]
    MissingArg(&'static str),
    #[error("unknown kind `{0}`, expected \"pdf\", \"image\", or \"auto\"")]
    UnknownKind(String),
    #[error("cannot infer kind from `{0}`; pass `kind` explicitly")]
    CannotInferKind(String),
    #[error("failed to decode image: {0}")]
    Decode(String),
}

enum Kind {
    Pdf,
    Image,
}

/// `ocr` namespace adapter. Stateless: every call reads `path` fresh.
#[derive(Default)]
pub struct OcrAdapter {
    verbs: Vec<VerbSpec>,
}

impl OcrAdapter {
    pub fn new() -> Self {
        Self {
            verbs: build_verbs(),
        }
    }

    fn call_inner(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, OcrError> {
        match verb {
            "extract" => self.extract(args),
            other => unreachable!(
                "AdapterRegistry only dispatches verbs OcrAdapter::verbs() declared, got `{other}`"
            ),
        }
    }

    fn extract(&self, args: &serde_json::Value) -> Result<serde_json::Value, OcrError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or(OcrError::MissingArg("path"))?;
        let kind_arg = args.get("kind").and_then(|v| v.as_str()).unwrap_or("auto");

        let bytes = std::fs::read(path).map_err(|source| OcrError::Io {
            path: path.to_string(),
            source,
        })?;

        let kind = match kind_arg {
            "pdf" => Kind::Pdf,
            "image" => Kind::Image,
            "auto" => infer_kind(path, &bytes)?,
            other => return Err(OcrError::UnknownKind(other.to_string())),
        };

        match kind {
            Kind::Pdf => {
                let extraction = pdf::extract(&bytes);
                let words: Vec<_> = extraction
                    .words
                    .iter()
                    .map(|w| word_json(&w.text, w.x, w.y, w.x + w.width, w.y + w.height))
                    .collect();
                Ok(json!({
                    "path": path,
                    "kind": "pdf",
                    "text": extraction.text,
                    "words": words
                }))
            }
            Kind::Image => {
                let img = png::decode(&bytes).map_err(|e| OcrError::Decode(e.to_string()))?;
                let extraction = glyphs::read_text(&img);
                let words: Vec<_> = extraction
                    .words
                    .iter()
                    .map(|w| word_json(&w.text, w.x0 as f64, w.y0 as f64, w.x1 as f64, w.y1 as f64))
                    .collect();
                Ok(json!({
                    "path": path,
                    "kind": "image",
                    "text": extraction.text,
                    "words": words
                }))
            }
        }
    }
}

impl Adapter for OcrAdapter {
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

/// `x0,y0` to `x1,y1`: PDF words carry PDF user-space coordinates (origin
/// bottom-left, y grows up); image words carry pixel coordinates (origin
/// top-left, y grows down). Callers already know which from the result's
/// top-level `kind`; unifying the axis convention itself would misrepresent
/// one of the two formats, so this only unifies the JSON field names.
fn word_json(text: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> serde_json::Value {
    json!({ "text": text, "x0": x0, "y0": y0, "x1": x1, "y1": y1 })
}

fn infer_kind(path: &str, bytes: &[u8]) -> Result<Kind, OcrError> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("pdf") => Ok(Kind::Pdf),
        Some("png") | Some("jpg") | Some("jpeg") | Some("bmp") | Some("gif") => Ok(Kind::Image),
        _ => {
            if bytes.starts_with(b"%PDF") {
                Ok(Kind::Pdf)
            } else if bytes.len() >= 8
                && bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
            {
                Ok(Kind::Image)
            } else {
                Err(OcrError::CannotInferKind(path.to_string()))
            }
        }
    }
}

fn build_verbs() -> Vec<VerbSpec> {
    vec![VerbSpec::new(
        "extract",
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "minLength": 1 },
                "kind": { "type": "string", "enum": ["auto", "pdf", "image"], "default": "auto" }
            },
            "additionalProperties": false
        }),
        RiskClass::Read,
        Idempotency::Idempotent,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdapterRegistry, Executor, MockSynthesizer, NoopSleeper};
    use operant_ir::{Action, ActionKind, Grounding, Pace, Retry};

    fn fixture(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/docs")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn extract_action(id: &str, args: serde_json::Value) -> Action {
        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("ocr"));
        params.insert("verb".into(), json!("extract"));
        params.insert("args".into(), args);
        Action {
            v: 1,
            id: id.into(),
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
        }
    }

    fn executor() -> Executor<MockSynthesizer> {
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(OcrAdapter::new()));
        Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper))
    }

    #[test]
    fn pdf_fixture_round_trips_through_action_ir_and_finds_the_invoice_tokens() {
        let exec = executor();
        let action = extract_action("pdf-1", json!({ "path": fixture("sample.pdf") }));
        let outcome = exec.execute(&action, None, None).unwrap();
        let result = outcome.adapter_result.unwrap();
        assert_eq!(result["kind"], json!("pdf"));
        let text = result["text"].as_str().unwrap();
        assert!(text.contains("INV-2026-0711"), "text was: {text:?}");
        assert!(text.contains("142.50"), "text was: {text:?}");
        assert!(!result["words"].as_array().unwrap().is_empty());
    }

    #[test]
    fn png_fixture_round_trips_through_action_ir_and_finds_the_invoice_tokens() {
        let exec = executor();
        let action = extract_action("png-1", json!({ "path": fixture("sample.png") }));
        let outcome = exec.execute(&action, None, None).unwrap();
        let result = outcome.adapter_result.unwrap();
        assert_eq!(result["kind"], json!("image"));
        let text = result["text"].as_str().unwrap();
        assert!(text.contains("INV-2026-0711"), "text was: {text:?}");
        assert!(text.contains("142.50"), "text was: {text:?}");
        assert!(!result["words"].as_array().unwrap().is_empty());
    }

    #[test]
    fn kind_can_be_forced_explicitly_instead_of_inferred() {
        let exec = executor();
        let action = extract_action(
            "png-forced",
            json!({ "path": fixture("sample.png"), "kind": "image" }),
        );
        let outcome = exec.execute(&action, None, None).unwrap();
        assert_eq!(outcome.adapter_result.unwrap()["kind"], json!("image"));
    }

    #[test]
    fn schema_rejects_an_unknown_kind_via_the_registry() {
        let mut reg = AdapterRegistry::new();
        reg.register(Box::new(OcrAdapter::new()));
        let err = reg
            .validate("ocr", "extract", &json!({ "path": "x", "kind": "tiff" }))
            .unwrap_err();
        assert!(matches!(err, AdapterError::SchemaValidation { .. }));
    }

    #[test]
    fn missing_file_is_a_typed_call_error_not_a_panic() {
        let exec = executor();
        let action = extract_action("missing", json!({ "path": fixture("does-not-exist.pdf") }));
        let err = exec.execute(&action, None, None).unwrap_err();
        assert!(matches!(
            err,
            crate::ActionError::Adapter(AdapterError::CallFailed { .. })
        ));
    }
}
