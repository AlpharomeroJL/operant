//! KI-1 regression: replay re-resolves a click's selector chain against a
//! live `Perceiver` at run time.
//!
//! A workflow taught against one window layout bakes a `coords_last_known`
//! into each click at teach time. If the same workflow is replayed against a
//! live window whose layout has since moved, replaying that stale coordinate
//! clicks empty space. With a `Perceiver` installed, replay re-resolves the
//! selector chain against a fresh snapshot and clicks where the element IS
//! now. Replay stays model-free: a `Perceiver` is a perception backend, never
//! a model backend (enforced by the crate graph and
//! `replay_crate_is_backend_free`).

use std::collections::BTreeMap;

use operant_action::{MockSynthesizer, SynthCall};
use operant_core::perceive::{PerceptionError, Perceiver, Resolved};
use operant_gates::EvalContext;
use operant_ir::{Action, Manifest, Selector, Snapshot};
use operant_replay::{CompiledWorkflow, Replayer};
use serde_json::json;

// The coordinate compiled into the workflow at teach time...
const STALE_X: f64 = 700.0;
const STALE_Y: f64 = 514.0;
// ...and where the element actually is now, after the layout moved.
const MOVED_X: f64 = 900.0;
const MOVED_Y: f64 = 642.0;

/// A live window whose layout has MOVED since the workflow was taught: it
/// resolves any selector chain to a fixed fresh point distinct from the stale
/// `coords_last_known` baked into the compiled click.
struct MovedWindowPerceiver;

impl MovedWindowPerceiver {
    fn fresh_snapshot(process: &str) -> Snapshot {
        serde_json::from_value(json!({
            "source": "fixture",
            "window": { "process": process, "title": "Untitled - Notepad", "dpi_scale": 1.0 },
            "digest": "0000000000000000000000000000000000000000000000000000000000000000",
            "elements": []
        }))
        .expect("mock snapshot builds")
    }
}

impl Perceiver for MovedWindowPerceiver {
    fn snapshot(&self, window_process: &str) -> Result<Snapshot, PerceptionError> {
        Ok(Self::fresh_snapshot(window_process))
    }

    fn resolve(
        &self,
        _snapshot: &Snapshot,
        selectors: &[Selector],
    ) -> Result<Resolved, PerceptionError> {
        if selectors.is_empty() {
            return Err(PerceptionError::SelectorMiss);
        }
        Ok(Resolved {
            x: MOVED_X,
            y: MOVED_Y,
            monitor: Some("MON1".to_string()),
        })
    }

    fn wait_until_changed(
        &self,
        _window_process: &str,
        _prev_digest: &str,
        timeout_ms: u64,
    ) -> Result<Snapshot, PerceptionError> {
        Err(PerceptionError::Timeout(timeout_ms))
    }
}

fn gateless_manifest() -> Manifest {
    serde_json::from_value(json!({
        "name": "ki1-live-reresolve",
        "version": "1.0.0",
        "description": "",
        "step_summary": [],
        "inputs_schema": {},
        "capabilities": { "risk_ceiling": "read" },
        "dsl": { "path": "workflow.ts", "hash": "0" }
    }))
    .expect("manifest builds")
}

/// A single click carrying BOTH a selector chain and a now-stale
/// `coords_last_known`.
fn selector_click_with_stale_coords() -> Action {
    serde_json::from_value(json!({
        "id": "click-editor",
        "kind": "click",
        "intent": "Click the text editor",
        "target": {
            "window": { "process": "notepad.exe" },
            "selectors": [{ "kind": "automation_id", "value": "RichEditD2DPT" }],
            "coords_last_known": { "x": STALE_X, "y": STALE_Y, "monitor": "MON1", "dpi_scale": 1.0 }
        },
        "risk_class": "read",
        "grounding": "uia"
    }))
    .expect("click action builds")
}

fn workflow_from(action: Action) -> CompiledWorkflow {
    CompiledWorkflow {
        manifest: gateless_manifest(),
        actions: vec![action],
    }
}

fn click_points(calls: &[SynthCall]) -> Vec<(f64, f64)> {
    calls
        .iter()
        .filter_map(|c| match c {
            SynthCall::ClickPoint(coords) => Some((coords.x, coords.y)),
            _ => None,
        })
        .collect()
}

#[test]
fn replay_reresolves_the_selector_chain_against_a_live_perceiver_not_stale_coords() {
    let wf = workflow_from(selector_click_with_stale_coords());
    let replayer = Replayer::with_mock().with_perceiver(Box::new(MovedWindowPerceiver));
    let ctx = EvalContext::new();

    let report = replayer
        .replay_compiled(&wf, &BTreeMap::new(), &ctx, &ctx)
        .expect("replay runs");
    assert_eq!(report.steps_executed, 1);

    let points = click_points(&replayer.synthesizer().calls());
    assert_eq!(
        points,
        vec![(MOVED_X, MOVED_Y)],
        "replay must click where the live Perceiver resolves the selector NOW, not the stale coord"
    );
    assert!(
        !points.contains(&(STALE_X, STALE_Y)),
        "the stale compiled coordinate must not be clicked once a selector re-resolves live"
    );
}

#[test]
fn wired_run_path_construction_reresolves_not_stale_coords() {
    // Reproduce the EXACT construction the CLI's real run path uses
    // (cli/src/commands/run.rs, gated behind `real-uia` + `real-input`):
    //
    //     Replayer::new(WindowsSynthesizer::new())
    //         .with_perceiver(Box::new(UiaPerceiver::new()))
    //
    // then drive it through `replay_compiled`. Headless, the two backends that
    // need a live desktop stand in for their real counterparts: a
    // `MockSynthesizer` (which records the point actually clicked) for
    // `WindowsSynthesizer`, and `MovedWindowPerceiver` (a fixture whose layout
    // has MOVED since teach time) for `UiaPerceiver`. The construction SHAPE
    // and the entry method are the run path's own, so this proves the wired
    // run path lands the click on the RE-RESOLVED target rather than the stale
    // coordinate compiled in at teach time. It does not (and cannot headlessly)
    // exercise the real Windows/UIA backends -- that live 5/5 desktop
    // confirmation is the orchestrator's to run.
    let wf = workflow_from(selector_click_with_stale_coords());
    let replayer =
        Replayer::new(MockSynthesizer::new()).with_perceiver(Box::new(MovedWindowPerceiver));
    let ctx = EvalContext::new();

    let report = replayer
        .replay_compiled(&wf, &BTreeMap::new(), &ctx, &ctx)
        .expect("replay runs");
    assert_eq!(report.steps_executed, 1);

    let points = click_points(&replayer.synthesizer().calls());
    assert_eq!(
        points,
        vec![(MOVED_X, MOVED_Y)],
        "the wired run-path construction must click where the live Perceiver resolves the \
         selector NOW, not the stale compiled coordinate"
    );
    assert!(
        !points.contains(&(STALE_X, STALE_Y)),
        "the stale teach-time coordinate must never be clicked once the run path installs a Perceiver"
    );
}

#[test]
fn without_a_perceiver_replay_falls_back_to_the_compiled_coords() {
    // The deterministic path the golden test relies on: no perceiver => the
    // compiled coords_last_known is replayed verbatim.
    let wf = workflow_from(selector_click_with_stale_coords());
    let replayer = Replayer::with_mock();
    let ctx = EvalContext::new();

    replayer
        .replay_compiled(&wf, &BTreeMap::new(), &ctx, &ctx)
        .expect("replay runs");

    assert_eq!(
        click_points(&replayer.synthesizer().calls()),
        vec![(STALE_X, STALE_Y)],
        "with no perceiver, replay clicks the cached coordinate exactly as before"
    );
}

#[test]
fn with_a_perceiver_but_no_selector_replay_still_uses_the_cached_coordinate() {
    // "fall back to coords only when no selector is available": a click that
    // carries only a cached coordinate is replayed from it even when a
    // Perceiver is installed, because there is no selector chain to re-resolve.
    let mut action = selector_click_with_stale_coords();
    action
        .target
        .as_mut()
        .unwrap()
        .selectors
        .clear();
    let wf = workflow_from(action);

    let replayer = Replayer::with_mock().with_perceiver(Box::new(MovedWindowPerceiver));
    let ctx = EvalContext::new();
    replayer
        .replay_compiled(&wf, &BTreeMap::new(), &ctx, &ctx)
        .expect("replay runs");

    assert_eq!(
        click_points(&replayer.synthesizer().calls()),
        vec![(STALE_X, STALE_Y)],
        "with no selector to re-resolve, replay uses the cached coordinate"
    );
}
