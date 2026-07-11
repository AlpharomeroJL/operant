//! `browser` namespace adapter (C5, `docs/ARCHITECTURE.md`): "CDP attach
//! to Chrome/Edge/Electron/WebView2. DOM plus accessibility tree emitted
//! as Perception Snapshots; DOM actions emitted as Action IR, so web
//! steps record, compile, and replay identically to native steps."
//!
//! - [`Browser`]: the trait every backend implements (`attach`,
//!   `snapshot`, `act`), the same "trait behind `Arc<dyn ...>`, a mock or
//!   fixture backend always built, a real backend behind a cargo
//!   feature" shape [`super::office`] and [`super::email`] already use
//!   for their own backends.
//! - [`fixture`]: [`fixture::FixtureBrowser`], the "no real browser"
//!   backend every test in this module runs against. Reads a fixture
//!   webapp HTML file directly (`contracts/fixtures/webapp/*.html`) and
//!   answers `Browser` calls off the parsed element table; no OS,
//!   network, or JS-engine calls involved. Always built, mirroring
//!   `operant-perception-uia::fixture::FixturePerceiver`'s precedent for
//!   C2 (`crates/perception-uia/src/fixture.rs`): "Always built... so
//!   every lane... gets a working Perceiver headless."
//! - [`html`]: the small, purpose-built markup scan [`fixture`] parses
//!   the two checked-in fixture pages with. No general HTML/DOM parser
//!   dependency, matching this crate's existing precedent for
//!   fixture-scale parsing (`super::email::parse`'s hand-rolled RFC 5322
//!   reader, `super::ocr`'s hand-rolled PNG/PDF readers).
//! - [`digest`]: the BLAKE3-over-elements-minus-bounds snapshot digest,
//!   duplicated from `operant-perception-uia::digest` (see that module's
//!   doc comment for why this crate does not just depend on it).
//! - [`cdp`]: [`cdp::CdpBrowser`], a minimal real backend behind the
//!   `cdp` cargo feature: attaches to a live Chrome/Edge DevTools target
//!   over the DevTools Protocol's WebSocket channel. Deliberately thin
//!   per this lane's brief ("fine to leave minimal"): attach only, full
//!   DOM/Input domain wiring is a FOLLOWUP. Not exercised by `cargo test`
//!   (no real browser in CI), the same posture `super::office::com`
//!   documents for its own feature-gated real backend.
//! - [`BrowserAdapter`]: registers the `browser` namespace with
//!   `attach`/`snapshot`/`click`/`type`/`assert` verbs, generic over
//!   `Arc<dyn Browser>` exactly like [`super::email::EmailAdapter`] is
//!   generic over `Arc<dyn MailStore>`/`Arc<dyn Mailer>`.

pub mod digest;
pub mod fixture;
pub mod html;

#[cfg(feature = "cdp")]
pub mod cdp;

use std::sync::Arc;

use operant_ir::snapshot::Snapshot;
use operant_ir::{Action, ActionKind, RiskClass, Selector};
use serde_json::json;
use thiserror::Error;

use crate::adapter::{Adapter, AdapterError, Idempotency, VerbSpec};

pub use fixture::FixtureBrowser;

#[cfg(feature = "cdp")]
pub use cdp::CdpBrowser;

const NAMESPACE: &str = "browser";

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("io error reading `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("no page attached; call `attach` first")]
    NotAttached,
    #[error("selector did not resolve to any element in the current page")]
    SelectorMiss,
    #[error("browser act does not support action kind `{0:?}`; only click, type, and assert are supported")]
    UnsupportedKind(ActionKind),
    #[error("the matched element does not support click (missing the `invoke` pattern)")]
    NotInvokable,
    #[error("assertion failed: {0}")]
    AssertionFailed(String),
    #[error("missing required argument `{0}`")]
    MissingArg(&'static str),
    #[error("cdp error: {0}")]
    Cdp(String),
    #[error("not implemented in this minimal backend: {0}")]
    Unsupported(&'static str),
    #[error("(de)serializing a browser adapter payload: {0}")]
    Serde(#[from] serde_json::Error),
}

/// One DOM interaction to perform and record. Browser steps "emit as
/// Action IR with css selectors" (`docs/ARCHITECTURE.md` C5): `selector`
/// is what [`Browser::act`] resolves against the current page, and the
/// [`Action`] it returns carries a css-first `target.selectors` so replay
/// grounds the same way recording did.
#[derive(Debug, Clone)]
pub struct BrowserAct {
    pub id: String,
    pub kind: ActionKind,
    pub selector: Selector,
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// Attach, snapshot, act: the C5 browser backend surface.
/// [`FixtureBrowser`] (always built) answers these off a parsed fixture
/// HTML file; [`CdpBrowser`] (behind the `cdp` feature) answers them
/// against a live Chrome/Edge tab.
pub trait Browser: Send + Sync {
    /// Attach to a page. Target shape is backend-defined:
    /// [`FixtureBrowser`] takes a filesystem path to an HTML fixture;
    /// [`CdpBrowser`] takes a `ws://` DevTools target URL.
    fn attach(&self, target: &str) -> Result<(), BrowserError>;

    /// Full normalized snapshot of the currently attached page,
    /// `source: "browser"` (`contracts/perception_snapshot.schema.json`).
    fn snapshot(&self) -> Result<Snapshot, BrowserError>;

    /// Resolve `act.selector` against the current page and perform
    /// `act.kind` (click/type/assert), returning the Action IR record of
    /// what happened so it can be recorded, compiled, and replayed
    /// identically to a native UIA step.
    fn act(&self, act: &BrowserAct) -> Result<Action, BrowserError>;
}

/// `browser` namespace adapter: registers `attach`/`snapshot`/`click`/
/// `type`/`assert` verbs against an injected [`Browser`] backend, exactly
/// the "trait behind `Arc<dyn ...>`" shape [`super::email::EmailAdapter`]
/// uses for [`super::email::MailStore`]/[`super::email::Mailer`].
pub struct BrowserAdapter {
    verbs: Vec<VerbSpec>,
    browser: Arc<dyn Browser>,
}

impl BrowserAdapter {
    pub fn new(browser: Arc<dyn Browser>) -> Self {
        Self {
            verbs: build_verbs(),
            browser,
        }
    }

    /// Convenience for tests and fixture-driven demos: a fresh,
    /// unattached [`FixtureBrowser`]. Tests that need to inspect the
    /// backend directly (e.g. to snapshot it outside the adapter's JSON
    /// envelope) should build their own `Arc<FixtureBrowser>` and pass it
    /// to [`BrowserAdapter::new`] instead, so they keep a handle to it.
    pub fn with_fixture() -> Self {
        Self::new(Arc::new(FixtureBrowser::new()))
    }

    fn call_inner(
        &self,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, BrowserError> {
        match verb {
            "attach" => self.attach(args),
            "snapshot" => self.do_snapshot(),
            "click" => self.act_verb(ActionKind::Click, "click", args),
            "type" => self.act_verb(ActionKind::Type, "type", args),
            "assert" => self.act_verb(ActionKind::Assert, "assert", args),
            other => unreachable!(
                "AdapterRegistry only dispatches verbs BrowserAdapter::verbs() declared, got `{other}`"
            ),
        }
    }

    fn attach(&self, args: &serde_json::Value) -> Result<serde_json::Value, BrowserError> {
        let target = str_arg(args, "target").ok_or(BrowserError::MissingArg("target"))?;
        self.browser.attach(&target)?;
        Ok(json!({ "attached": true, "target": target }))
    }

    fn do_snapshot(&self) -> Result<serde_json::Value, BrowserError> {
        let snap = self.browser.snapshot()?;
        Ok(serde_json::to_value(&snap)?)
    }

    fn act_verb(
        &self,
        kind: ActionKind,
        verb: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, BrowserError> {
        let selector = str_arg(args, "selector").ok_or(BrowserError::MissingArg("selector"))?;
        let id = str_arg(args, "id").unwrap_or_else(|| format!("browser-{verb}"));

        let mut params = serde_json::Map::new();
        for key in ["text", "expect_name", "expect_value"] {
            if let Some(v) = args.get(key) {
                params.insert(key.to_string(), v.clone());
            }
        }

        let act = BrowserAct {
            id,
            kind,
            selector: Selector::Css { value: selector },
            params,
        };
        let action = self.browser.act(&act)?;
        Ok(serde_json::to_value(&action)?)
    }
}

impl Adapter for BrowserAdapter {
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

fn build_verbs() -> Vec<VerbSpec> {
    vec![
        VerbSpec::new(
            "attach",
            json!({
                "type": "object",
                "required": ["target"],
                "properties": { "target": { "type": "string", "minLength": 1 } },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "snapshot",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "click",
            json!({
                "type": "object",
                "required": ["selector"],
                "properties": {
                    "selector": { "type": "string", "minLength": 1 },
                    "id": { "type": "string" }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            // Retrying a click may re-submit a form, matching
            // crate::adapter's own doc example for this enum variant
            // (email.send): a duplicated side effect, never auto-retried.
            Idempotency::NotIdempotent,
        ),
        VerbSpec::new(
            "type",
            json!({
                "type": "object",
                "required": ["selector", "text"],
                "properties": {
                    "selector": { "type": "string", "minLength": 1 },
                    "text": { "type": "string" },
                    "id": { "type": "string" }
                },
                "additionalProperties": false
            }),
            RiskClass::Write,
            // Setting a field's value is a replace, not an append:
            // retrying with the same text is safe.
            Idempotency::Idempotent,
        ),
        VerbSpec::new(
            "assert",
            json!({
                "type": "object",
                "required": ["selector"],
                "properties": {
                    "selector": { "type": "string", "minLength": 1 },
                    "expect_name": { "type": "string" },
                    "expect_value": { "type": "string" },
                    "id": { "type": "string" }
                },
                "additionalProperties": false
            }),
            RiskClass::Read,
            Idempotency::Idempotent,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdapterRegistry, Executor, MockSynthesizer, NoopSleeper};
    use operant_ir::{Grounding, Pace, Retry, Target};

    fn adapter_call_action(
        id: &str,
        verb: &str,
        args: serde_json::Value,
        risk: RiskClass,
    ) -> Action {
        let mut params = serde_json::Map::new();
        params.insert("namespace".into(), json!("browser"));
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
            irreversible: false,
            grounding: Grounding::Adapter,
            timeout_ms: 5000,
            retry: Retry {
                attempts: 0,
                backoff_ms: 0,
            },
        }
    }

    fn fixture_index_path() -> String {
        FixtureBrowser::fixtures_dir()
            .join("index.html")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn namespace_and_verbs_match_the_action_ir_contract() {
        let adapter = BrowserAdapter::with_fixture();
        assert_eq!(adapter.namespace(), "browser");
        let names: Vec<_> = adapter.verbs().iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["attach", "snapshot", "click", "type", "assert"]);
    }

    #[test]
    fn validate_rejects_a_click_missing_its_selector() {
        let mut reg = AdapterRegistry::new();
        reg.register(Box::new(BrowserAdapter::with_fixture()));
        let err = reg.validate("browser", "click", &json!({})).unwrap_err();
        assert!(matches!(err, AdapterError::SchemaValidation { .. }));
    }

    #[test]
    fn attach_then_snapshot_round_trip_through_action_ir() {
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(BrowserAdapter::with_fixture()));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let attach = adapter_call_action(
            "attach-1",
            "attach",
            json!({ "target": fixture_index_path() }),
            RiskClass::Read,
        );
        exec.execute(&attach, None, None).unwrap();

        let snap = adapter_call_action("snap-1", "snapshot", json!({}), RiskClass::Read);
        let outcome = exec.execute(&snap, None, None).unwrap();
        let result = outcome.adapter_result.unwrap();
        assert_eq!(result["source"], json!("browser"));
        assert_eq!(result["elements"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn click_type_assert_adapter_calls_round_trip_and_click_is_write_risk() {
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(BrowserAdapter::with_fixture()));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let attach = adapter_call_action(
            "attach-1",
            "attach",
            json!({ "target": fixture_index_path() }),
            RiskClass::Read,
        );
        exec.execute(&attach, None, None).unwrap();

        let type_action = adapter_call_action(
            "type-1",
            "type",
            json!({ "selector": "#customer", "text": "Acme Corp" }),
            RiskClass::Write,
        );
        // Write risk needs no approval, unlike destructive (email.send).
        let outcome = exec.execute(&type_action, None, None).unwrap();
        let action_ir = outcome.adapter_result.unwrap();
        assert_eq!(action_ir["kind"], json!("type"));
        assert_eq!(action_ir["target"]["selectors"][0]["kind"], json!("css"));
        assert_eq!(
            action_ir["target"]["selectors"][0]["value"],
            json!("#customer")
        );

        let click = adapter_call_action(
            "click-1",
            "click",
            json!({ "selector": "#save-btn" }),
            RiskClass::Write,
        );
        let outcome = exec.execute(&click, None, None).unwrap();
        assert_eq!(outcome.adapter_result.unwrap()["kind"], json!("click"));

        let assert_call = adapter_call_action(
            "assert-1",
            "assert",
            json!({ "selector": "#save-btn", "expect_name": "Save invoice" }),
            RiskClass::Read,
        );
        let outcome = exec.execute(&assert_call, None, None).unwrap();
        assert_eq!(outcome.adapter_result.unwrap()["kind"], json!("assert"));
    }

    #[test]
    fn assert_mismatch_surfaces_as_a_call_failed_error() {
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(BrowserAdapter::with_fixture()));
        let exec = Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper));

        let attach = adapter_call_action(
            "attach-1",
            "attach",
            json!({ "target": fixture_index_path() }),
            RiskClass::Read,
        );
        exec.execute(&attach, None, None).unwrap();

        let assert_call = adapter_call_action(
            "assert-1",
            "assert",
            json!({ "selector": "#save-btn", "expect_name": "Not The Real Name" }),
            RiskClass::Read,
        );
        let err = exec.execute(&assert_call, None, None).unwrap_err();
        assert!(matches!(
            err,
            crate::ActionError::Adapter(AdapterError::CallFailed { .. })
        ));
    }

    // Guard against a silent regression in `target` construction: every
    // browser-emitted Action IR step must carry a `Target` (never `None`),
    // or replay would have nothing to resolve.
    #[test]
    fn emitted_actions_always_carry_a_target() {
        let fx = FixtureBrowser::new();
        fx.attach(&fixture_index_path()).unwrap();
        let act = BrowserAct {
            id: "t1".into(),
            kind: ActionKind::Click,
            selector: Selector::Css {
                value: "#save-btn".into(),
            },
            params: serde_json::Map::new(),
        };
        let action: Action = fx.act(&act).unwrap();
        let target: &Target = action.target.as_ref().expect("target present");
        assert!(!target.selectors.is_empty());
    }
}
