//! End-to-end: compile the frozen notepad trajectory, then replay the resulting
//! `CompiledWorkflow` through a `MockSynthesizer` and check the exact ordered
//! synthesizer calls and the pre/post gate outcomes.
//!
//! The compiler runs only to produce input for the replayer; it crosses the
//! crate boundary as JSON (serialize `operant_compiler::CompiledWorkflow`,
//! deserialize `operant_replay::CompiledWorkflow`), exactly as the serde-JSON
//! contract intends.

use std::collections::BTreeMap;

use operant_action::SynthCall;
use operant_compiler::{compile, Trajectory};
use operant_gates::EvalContext;
use operant_ir::{Coords, GateResult, Role, Snapshot, WindowMatch};
use operant_replay::{CompiledWorkflow, ReplayError, Replayer};

const TRAJECTORY: &str = include_str!("../../../contracts/fixtures/trajectory_notepad.json");
const SNAPSHOT: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");

const TYPED_TEXT: &str = "Invoice 2026-07-11 total $142.50";

fn compiled() -> CompiledWorkflow {
    let traj: Trajectory = serde_json::from_str(TRAJECTORY).expect("trajectory parses");
    let comp = compile(&traj).expect("fixture compiles");
    // Round-trip through JSON so the test exercises the serde contract the
    // replayer consumes, not a shared in-process type.
    let json = serde_json::to_value(&comp.workflow).expect("serializes");
    serde_json::from_value(json).expect("deserializes into replay type")
}

fn notepad_snapshot() -> Snapshot {
    serde_json::from_str(SNAPSHOT).expect("snapshot parses")
}

/// A perception snapshot whose editor holds `value` (post-condition context).
fn snapshot_with_editor(value: &str) -> Snapshot {
    let mut snap = notepad_snapshot();
    for e in &mut snap.elements {
        if e.role == Role::Document && e.name == "Text editor" {
            e.value = Some(value.to_string());
        }
    }
    snap
}

fn pre_ctx() -> EvalContext {
    EvalContext::new().with_snapshot(notepad_snapshot())
}

fn post_ctx() -> EvalContext {
    EvalContext::new().with_snapshot(snapshot_with_editor(TYPED_TEXT))
}

fn notepad_window() -> WindowMatch {
    WindowMatch {
        process: Some("notepad.exe".to_string()),
        title_pattern: Some(".* - Notepad".to_string()),
    }
}

#[test]
fn replay_reproduces_click_type_save_and_passes_postcondition() {
    let wf = compiled();
    let replayer = Replayer::with_mock();

    let report = replayer
        .replay_compiled(&wf, &BTreeMap::new(), &pre_ctx(), &post_ctx())
        .expect("replay succeeds");

    let point = Coords {
        x: 700.0,
        y: 500.0,
        monitor: Some("MON1".to_string()),
        dpi_scale: Some(1.0),
    };
    // click editor, type templated text with inputs substituted, ctrl+s.
    // The two synthesized waits produce no synthesizer calls; the final assert
    // is evaluated as the postcondition gate, not dispatched.
    assert_eq!(
        replayer.synthesizer().calls(),
        vec![
            SynthCall::FocusWindow(notepad_window()),
            SynthCall::ClickPoint(point),
            SynthCall::FocusWindow(notepad_window()),
            SynthCall::TypeText(TYPED_TEXT.to_string()),
            SynthCall::FocusWindow(notepad_window()),
            SynthCall::Key("ctrl+s".to_string()),
        ]
    );

    assert_eq!(report.steps_executed, 5); // click, type, wait, key, wait
    assert_eq!(report.pre, vec![GateResult::Pass]);
    assert_eq!(report.post, vec![GateResult::Pass]);
}

#[test]
fn caller_inputs_override_the_manifest_defaults() {
    let wf = compiled();
    let replayer = Replayer::with_mock();

    let mut inputs = BTreeMap::new();
    inputs.insert("invoice_date".to_string(), "2027-01-02".to_string());
    inputs.insert("amount".to_string(), "9.99".to_string());

    let post =
        EvalContext::new().with_snapshot(snapshot_with_editor("Invoice 2027-01-02 total $9.99"));
    replayer
        .replay_compiled(&wf, &inputs, &pre_ctx(), &post)
        .expect("replay with overrides succeeds");

    assert!(replayer
        .synthesizer()
        .calls()
        .contains(&SynthCall::TypeText(
            "Invoice 2027-01-02 total $9.99".to_string()
        )));
}

#[test]
fn replay_performs_zero_network_operations() {
    let wf = compiled();
    let replayer = Replayer::with_mock();
    replayer
        .replay_compiled(&wf, &BTreeMap::new(), &pre_ctx(), &post_ctx())
        .expect("replay succeeds");

    // The only observable effects of a replay are local input-synthesis calls.
    // There is no socket, no backend, no adapter_call: every recorded call is a
    // local Synthesizer primitive, never a network operation.
    let calls = replayer.synthesizer().calls();
    assert_eq!(calls.len(), 6);
    for call in &calls {
        let local = matches!(
            call,
            SynthCall::FocusWindow(_)
                | SynthCall::ClickPoint(_)
                | SynthCall::TypeText(_)
                | SynthCall::Key(_)
        );
        assert!(local, "unexpected non-local synthesizer call: {call:?}");
    }
}

#[test]
fn wrong_foreground_app_halts_before_any_step() {
    let wf = compiled();
    let replayer = Replayer::with_mock();

    let mut snap = notepad_snapshot();
    snap.window.process = "chrome.exe".to_string();
    let pre = EvalContext::new().with_snapshot(snap);

    let err = replayer
        .replay_compiled(&wf, &BTreeMap::new(), &pre, &post_ctx())
        .expect_err("precondition must fail");
    assert!(matches!(err, ReplayError::Precondition { index: 0 }));
    // Halted before any synthesizer call ran.
    assert_eq!(replayer.synthesizer().calls().len(), 0);
}

#[test]
fn postcondition_fails_when_the_note_was_not_written() {
    let wf = compiled();
    let replayer = Replayer::with_mock();

    // The editor is still empty in the post context, so the assert regex fails.
    let empty_post = EvalContext::new().with_snapshot(snapshot_with_editor(""));
    let err = replayer
        .replay_compiled(&wf, &BTreeMap::new(), &pre_ctx(), &empty_post)
        .expect_err("postcondition must fail");
    assert!(matches!(err, ReplayError::Postcondition { .. }));
    // The steps still executed; the failure is detected after they ran.
    assert_eq!(replayer.synthesizer().calls().len(), 6);
}
