//! Kill switch (C20 / FR-S5, `docs/specs/guardian.md`) behavior, exercised
//! against `Executor<MockSynthesizer>` with no OS dependency.
//!
//! These tests all touch `operant_action::killswitch`'s process-wide
//! atomic, so this file (compiled by cargo as its own test binary, unlike
//! the crate's `#[cfg(test)]` unit tests) serializes them through
//! [`isolated`] rather than letting cargo's default multi-threaded test
//! runner interleave two tests that engage/reset the same global.

use std::panic::{self, UnwindSafe};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use operant_action::killswitch;
use operant_action::{ActionError, Executor, MockSynthesizer, NoopSleeper, SynthCall};
use operant_core::Bus;
use operant_ir::{
    Action, ActionKind, Coords, Grounding, Pace, Retry, RiskClass, Target, WindowMatch,
};
use serde_json::json;

static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Serializes access to the kill switch's global atomic across this
/// file's tests, and guarantees a clean (`reset`) baseline both before
/// and after each one, even if the closure panics on a failed assertion.
/// Without this, two tests in this binary could run on different
/// threads, race `engage`/`reset`, and either spuriously freeze an
/// unrelated test's dispatch or spuriously leave one engaged for the
/// next test to trip over.
fn isolated<T>(f: impl FnOnce() -> T + UnwindSafe) -> T {
    let _guard = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    killswitch::reset();
    let result = panic::catch_unwind(f);
    killswitch::reset();
    result.unwrap_or_else(|e| panic::resume_unwind(e))
}

fn base_action(id: &str, kind: ActionKind) -> Action {
    Action {
        v: 1,
        id: id.to_string(),
        kind,
        intent: None,
        target: None,
        params: serde_json::Map::new(),
        pace: Pace::Instant,
        risk_class: RiskClass::Read,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5_000,
        retry: Retry {
            attempts: 0,
            backoff_ms: 1,
        },
    }
}

fn key_action(id: &str, combo: &str) -> Action {
    let mut action = base_action(id, ActionKind::Key);
    action.params.insert("combo".into(), json!(combo));
    action
}

/// A `click` action targeting a window, so a successful dispatch costs
/// two synthesizer calls (`FocusWindow` then `ClickPoint`): exactly the
/// shape needed to prove a freeze mid-action leaves neither call behind.
fn windowed_click_action(id: &str) -> (Action, Coords) {
    let mut action = base_action(id, ActionKind::Click);
    action.target = Some(Target {
        window: Some(WindowMatch {
            process: Some("notepad.exe".into()),
            title_pattern: None,
        }),
        ..Default::default()
    });
    let point = Coords {
        x: 12.0,
        y: 34.0,
        monitor: None,
        dpi_scale: None,
    };
    (action, point)
}

/// Engages the switch while a background thread is mid-run (several
/// steps into a longer sequence, dispatching on its own thread), and
/// asserts both that the very next dispatch attempt observes `Frozen`
/// and that engage-to-frozen latency stays under the 100 ms bar in
/// `docs/specs/guardian.md`.
#[test]
fn engage_freezes_the_next_dispatch_within_100ms_of_a_mid_run_trigger() {
    isolated(|| {
        let bus = Bus::new();
        let exec = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));

        let worker = thread::spawn(move || {
            let mut frozen_at: Option<Instant> = None;
            for i in 0..60 {
                let action = key_action(&format!("step-{i}"), "ctrl+s");
                match exec.execute(&action, None, None) {
                    Ok(_) => {}
                    Err(ActionError::Frozen { .. }) => {
                        frozen_at = Some(Instant::now());
                        break;
                    }
                    Err(e) => panic!("unexpected dispatch error mid-run: {e:?}"),
                }
                thread::sleep(Duration::from_millis(2));
            }
            (exec, frozen_at)
        });

        // Let the worker get several steps into its run before tripping
        // the switch, so this is a genuine mid-run trigger rather than a
        // race against the very first dispatch.
        thread::sleep(Duration::from_millis(15));
        let engage_started = Instant::now();
        killswitch::engage(&bus);

        let (exec, frozen_at) = worker.join().expect("worker thread did not panic");
        let frozen_at =
            frozen_at.expect("the run must observe Frozen on some step before exhausting all 60");
        let elapsed = frozen_at.duration_since(engage_started);
        assert!(
            elapsed < Duration::from_millis(100),
            "engage-to-frozen took {elapsed:?}, want under 100ms"
        );

        // At least one step must have actually dispatched before the
        // freeze landed, confirming the run was genuinely interrupted
        // mid-flight rather than already frozen before it started.
        assert!(
            exec.synthesizer().call_count() > 0,
            "expected at least one successful step before the freeze"
        );
    });
}

/// Engaging the switch and then attempting a dispatch performs the
/// modifier release-all sweep, and nothing else.
#[test]
fn engaging_before_a_dispatch_performs_the_release_all_sweep() {
    isolated(|| {
        let bus = Bus::new();
        let exec = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));

        killswitch::engage(&bus);

        let action = key_action("step-1", "ctrl+s");
        let err = exec.execute(&action, None, None).unwrap_err();
        assert!(matches!(err, ActionError::Frozen { ref action_id } if action_id == "step-1"));

        assert_eq!(
            exec.synthesizer().calls(),
            vec![SynthCall::ReleaseAllModifiers],
            "a frozen dispatch must perform the sweep and nothing else"
        );
    });
}

/// A run refused mid-step (after some earlier step already dispatched
/// normally) leaves a consistent state: an action that would normally
/// cost two synthesizer calls (focus, then click) leaves neither behind
/// once frozen, only the sweep.
#[test]
fn frozen_run_leaves_no_partial_synthesizer_calls() {
    isolated(|| {
        let bus = Bus::new();
        let exec = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));

        let warm_up = key_action("step-0", "ctrl+s");
        exec.execute(&warm_up, None, None)
            .expect("warm-up step dispatches normally before the freeze");
        assert_eq!(exec.synthesizer().call_count(), 1);

        killswitch::engage(&bus);

        let (click, point) = windowed_click_action("step-1");
        let err = exec.execute(&click, Some(&point), None).unwrap_err();
        assert!(matches!(err, ActionError::Frozen { ref action_id } if action_id == "step-1"));

        // Exactly the warm-up key call plus the sweep: no `FocusWindow`
        // and no `ClickPoint`, even though this click action normally
        // needs both.
        assert_eq!(
            exec.synthesizer().calls(),
            vec![
                SynthCall::Key("ctrl+s".into()),
                SynthCall::ReleaseAllModifiers,
            ],
            "no partial focus/click calls may leak through after a freeze"
        );
    });
}

/// `reset()` is the explicit human resume: it clears the freeze and
/// ordinary dispatch resumes on the very next attempt.
#[test]
fn reset_reenables_dispatch() {
    isolated(|| {
        let bus = Bus::new();
        let exec = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));

        killswitch::engage(&bus);
        assert!(killswitch::is_engaged());
        let frozen = exec.execute(&key_action("step-1", "ctrl+s"), None, None);
        assert!(matches!(frozen, Err(ActionError::Frozen { .. })));

        killswitch::reset();
        assert!(!killswitch::is_engaged());

        let outcome = exec
            .execute(&key_action("step-2", "ctrl+s"), None, None)
            .expect("dispatch must resume once the switch is reset");
        assert_eq!(outcome.attempts, 1);
        assert_eq!(
            exec.synthesizer().calls(),
            vec![
                SynthCall::ReleaseAllModifiers,  // the frozen attempt's sweep
                SynthCall::Key("ctrl+s".into()), // step-2, after reset
            ]
        );
    });
}
