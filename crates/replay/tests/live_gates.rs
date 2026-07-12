//! E3: with live gate snapshots enabled, pre/post gates evaluate against
//! perception captured LIVE around the run, not the caller-supplied context.
//!
//! The property that matters: a PASS proves the LIVE desktop reached the
//! asserted state. To show it, these tests hand the replayer a STALE post
//! context whose editor is empty (which would fail the post gate) and a live
//! `Perceiver` whose desktop actually holds the invoice. With the opt-in on,
//! the post gate passes because it reads the live desktop, not the stale
//! context; with the opt-in off, installing a perceiver changes nothing and
//! the stale context still governs the gate (so every existing test that
//! installs a perceiver for click re-resolution keeps its exact behavior).

use std::collections::BTreeMap;

use operant_action::MockSynthesizer;
use operant_compiler::{compile, Trajectory};
use operant_core::perceive::{PerceptionError, Perceiver, Resolved};
use operant_gates::EvalContext;
use operant_ir::{GateResult, Role, Selector, Snapshot};
use operant_replay::{CompiledWorkflow, ReplayError, Replayer};

const TRAJECTORY: &str = include_str!("../../../contracts/fixtures/trajectory_notepad.json");
const SNAPSHOT: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
const INVOICE: &str = "Invoice 2026-07-11 total $142.50";

fn compiled() -> CompiledWorkflow {
    let traj: Trajectory = serde_json::from_str(TRAJECTORY).expect("trajectory parses");
    let comp = compile(&traj).expect("fixture compiles");
    let json = serde_json::to_value(&comp.workflow).expect("serializes");
    serde_json::from_value(json).expect("deserializes into replay type")
}

fn notepad_snapshot_with_editor(value: &str) -> Snapshot {
    let mut snap: Snapshot = serde_json::from_str(SNAPSHOT).expect("snapshot parses");
    for e in &mut snap.elements {
        if e.role == Role::Document && e.name == "Text editor" {
            e.value = Some(value.to_string());
        }
    }
    snap
}

/// A live Notepad whose editor already holds the invoice: it stands in for a
/// desktop the run has driven to the asserted state. Its `snapshot` is the live
/// screen the gates read when the opt-in is on; `resolve` lands a click for the
/// workflow's one selector-bearing step so the run reaches its post gate.
struct LiveNotepad;

impl Perceiver for LiveNotepad {
    fn snapshot(&self, _window_process: &str) -> Result<Snapshot, PerceptionError> {
        Ok(notepad_snapshot_with_editor(INVOICE))
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
            x: 700.0,
            y: 500.0,
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

#[test]
fn live_gate_snapshots_override_a_stale_post_context() {
    let wf = compiled();
    let replayer = Replayer::new(MockSynthesizer::new())
        .with_perceiver(Box::new(LiveNotepad))
        .with_live_gate_snapshots();

    // A deliberately stale context: its editor is EMPTY, so if the gates read
    // it (rather than the live desktop) the post condition would FAIL.
    let stale = EvalContext::new().with_snapshot(notepad_snapshot_with_editor(""));

    let report = replayer
        .replay_compiled(&wf, &BTreeMap::new(), &stale, &stale)
        .expect("replay succeeds against the live desktop");

    // Both gates passed because they read the LIVE Notepad (invoice present),
    // not the stale empty context that was passed in.
    assert_eq!(report.pre, vec![GateResult::Pass]);
    assert_eq!(
        report.post,
        vec![GateResult::Pass],
        "the post gate must read the live desktop the run produced, not the stale context"
    );
}

#[test]
fn without_the_opt_in_a_perceiver_does_not_change_gate_evaluation() {
    // Installing a Perceiver (for click re-resolution) but NOT opting into live
    // gate snapshots must leave gate evaluation exactly as it was: the passed
    // contexts govern. This is the guarantee that keeps every existing
    // perceiver-installing test deterministic.
    let wf = compiled();
    let replayer = Replayer::new(MockSynthesizer::new()).with_perceiver(Box::new(LiveNotepad));

    let good_pre = EvalContext::new().with_snapshot(notepad_snapshot_with_editor(""));
    let stale_empty_post = EvalContext::new().with_snapshot(notepad_snapshot_with_editor(""));

    let err = replayer
        .replay_compiled(&wf, &BTreeMap::new(), &good_pre, &stale_empty_post)
        .expect_err("the stale empty post context must still fail the post gate");
    assert!(
        matches!(err, ReplayError::Postcondition { .. }),
        "without the opt-in, the passed (stale) post context governs the gate, got {err:?}"
    );
}
