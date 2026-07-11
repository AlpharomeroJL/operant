//! Replays `contracts/fixtures/trajectory_notepad.json` (the canonical
//! explore trajectory fixture) through `Executor<MockSynthesizer>` and
//! checks the exact sequence of synthesizer calls it produces.
//!
//! The fixture is the *compiler's* input shape (each step wraps an Action
//! IR step with recorder metadata: snapshot digests, outcome, timing).
//! This test only needs the inner `action` object per step, which is
//! exactly `operant_ir::Action`; resolving a step's target into a screen
//! point is perception's job (C2/C3), so the test supplies the point a
//! resolver would have produced (the fixture's own `coords_last_known`
//! when present, otherwise a stand-in).

use operant_action::{ActionError, Executor, MockSynthesizer, NoopSleeper, SynthCall};
use operant_ir::{Action, ActionKind, Coords};

const FIXTURE: &str = include_str!("../../../contracts/fixtures/trajectory_notepad.json");

fn fixture_actions() -> Vec<Action> {
    let doc: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
    doc["steps"]
        .as_array()
        .expect("fixture has a `steps` array")
        .iter()
        .map(|step| {
            serde_json::from_value(step["action"].clone())
                .expect("step.action is a valid Action IR")
        })
        .collect()
}

#[test]
fn fixture_has_the_five_documented_steps() {
    let actions = fixture_actions();
    assert_eq!(
        actions.iter().map(|a| a.kind).collect::<Vec<_>>(),
        vec![
            ActionKind::Click,
            ActionKind::Type,
            ActionKind::Click,
            ActionKind::Key,
            ActionKind::Assert,
        ]
    );
}

#[test]
fn trajectory_replays_the_expected_synthesizer_calls() {
    let actions = fixture_actions();
    let exec = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));

    let window = actions[0]
        .target
        .as_ref()
        .and_then(|t| t.window.clone())
        .expect("step 1 targets a window");

    // Step 1: click. The fixture's own `coords_last_known` stands in for
    // whatever perception would have resolved.
    let step1_point = actions[0]
        .target
        .as_ref()
        .and_then(|t| t.coords_last_known.clone())
        .expect("step 1 carries coords_last_known");
    let outcome = exec
        .execute(&actions[0], Some(&step1_point), None)
        .expect("step 1 executes");
    assert_eq!(outcome.attempts, 1);

    // Step 2: type. No resolved point needed.
    let outcome = exec
        .execute(&actions[1], None, None)
        .expect("step 2 executes");
    assert_eq!(outcome.attempts, 1);

    // Step 3: click again (the fixture's raw trajectory records this as
    // `retry_superseded`, a compiler-pass concept this crate does not
    // interpret; it is still a plain click Action IR step at this layer).
    // It carries no coords_last_known, so supply a stand-in resolved point.
    let step3_point = Coords {
        x: 50.0,
        y: 60.0,
        monitor: None,
        dpi_scale: None,
    };
    let outcome = exec
        .execute(&actions[2], Some(&step3_point), None)
        .expect("step 3 executes");
    assert_eq!(outcome.attempts, 1);

    // Step 4: key (ctrl+s).
    let outcome = exec
        .execute(&actions[3], None, None)
        .expect("step 4 executes");
    assert_eq!(outcome.attempts, 1);

    // Step 5: assert. Evaluating a gate expression against a perception
    // snapshot is the gate engine's job (C9, operant-gates), not this
    // executor's; it refuses the kind up front, typed, with zero
    // synthesizer side effects.
    let err = exec.execute(&actions[4], None, None).unwrap_err();
    assert!(matches!(
        err,
        ActionError::Unsupported {
            kind: ActionKind::Assert
        }
    ));

    assert_eq!(
        exec.synthesizer().calls(),
        vec![
            SynthCall::FocusWindow(window.clone()),
            SynthCall::ClickPoint(step1_point),
            SynthCall::FocusWindow(window.clone()),
            SynthCall::TypeText("Invoice 2026-07-11 total $142.50".into()),
            SynthCall::FocusWindow(window.clone()),
            SynthCall::ClickPoint(step3_point),
            SynthCall::FocusWindow(window),
            SynthCall::Key("ctrl+s".into()),
        ],
        "assert (step 5) must not add any synthesizer calls"
    );
}
