//! wasm-bindgen bridge (feature `wasm`, wasm32-unknown-unknown only): the
//! JS-callable surface the docs-site playground (`site/playground/`)
//! drives. This wraps the exact same [`Replayer`] plus
//! `operant_action::adapters::browser::{FixtureBrowser, BrowserAdapter}`
//! code a native build uses; the only wasm-specific thing here is the
//! `String`/`JsValue` boundary at the edges, not the replay logic itself.
//! Off by default: see this crate's `Cargo.toml` for why enabling it
//! changes dependency resolution but never this crate's own compiled
//! behavior.
//!
//! `FixtureBrowser::attach_html` (unlike [`Browser::attach`]) never
//! touches the filesystem, so the whole call graph this module drives is
//! wasm32-unknown-unknown-portable: parse HTML text already in memory,
//! mutate an in-memory element table, dispatch through the same
//! `AdapterRegistry`/`Executor` code path as any native adapter call.

use std::collections::BTreeMap;
use std::sync::Arc;

use operant_action::adapters::browser::{BrowserAdapter, FixtureBrowser};
use operant_action::{AdapterRegistry, MockSynthesizer};
use operant_gates::EvalContext;
use operant_ir::GateResult;
use wasm_bindgen::prelude::*;

use crate::{CompiledWorkflow, Replayer};

/// Replay `workflow_json` (a serialized [`CompiledWorkflow`]) against
/// `page_html` (the fixture webapp's markup) through the real `browser`
/// namespace adapter: a fresh [`FixtureBrowser`] attached to `page_html`
/// stands in for a live CDP-attached tab, so every `adapter_call` step
/// (`namespace: "browser"`) dispatches through
/// [`operant_action::AdapterRegistry`] exactly as it would natively.
///
/// Returns a JSON object `{ "steps_executed": number, "pre_pass": bool,
/// "post_pass": bool }` on success. On any replay error (a failing gate, an
/// unregistered adapter, an assert step that fails for real against the
/// fixture page) returns `Err` with the error's `Display` text, so the
/// caller can show it rather than silently treating a broken replay as a
/// pass.
#[wasm_bindgen]
pub fn replay_fixture(workflow_json: &str, page_html: &str) -> Result<String, JsValue> {
    let workflow: CompiledWorkflow =
        serde_json::from_str(workflow_json).map_err(|e| JsValue::from_str(&e.to_string()))?;

    let browser = Arc::new(FixtureBrowser::new());
    browser
        .attach_html(page_html)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let mut adapters = AdapterRegistry::new();
    adapters.register(Box::new(BrowserAdapter::new(browser)));

    let replayer = Replayer::with_adapters(MockSynthesizer::new(), adapters);
    let ctx = EvalContext::new();
    let report = replayer
        .replay_compiled(&workflow, &BTreeMap::new(), &ctx, &ctx)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let all_pass = |results: &[GateResult]| results.iter().all(|r| *r == GateResult::Pass);
    let out = serde_json::json!({
        "steps_executed": report.steps_executed,
        "pre_pass": all_pass(&report.pre),
        "post_pass": all_pass(&report.post),
    });
    Ok(out.to_string())
}
