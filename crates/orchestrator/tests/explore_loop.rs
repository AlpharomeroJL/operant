//! Integration tests for the EXPLORE loop (C6, L7A): a scripted
//! Notepad-style task driven by `mock_planner` against `FixturePerceiver`
//! and `MockSynthesizer`, headless, no network/GPU. The happy-path script
//! mirrors the narrative in `contracts/fixtures/trajectory_notepad.json`
//! (click the editor, type a note, save) but drives it live through
//! `ExploreLoop::run` rather than reading a canned trajectory.

use operant_action::{Executor, MockSynthesizer, NoopSleeper, SynthCall};
use operant_core::bus::events::{
    HaltReason, RunCompleted, RunRedirected, RunStarted, RunStepExecuted, RunStepGated,
};
use operant_core::bus::events::RunOutcome as BusRunOutcome;
use operant_core::{Bus, Perceiver};
use operant_ir::GateResult;
use operant_orchestrator::backends::{BackendEvent, MockPlannerBackend};
use operant_orchestrator::explore::{ExploreLoop, NoControl, RunControl, ScriptedControl};
use operant_perception_uia::FixturePerceiver;
use operant_recorder::Recorder;
use serde_json::json;

const GOAL: &str = "Write an invoice note in Notepad and save it";

fn notepad_perceiver() -> Box<dyn Perceiver> {
    let raw = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
    Box::new(FixturePerceiver::from_json(raw).expect("shared notepad fixture parses"))
}

fn click_editor_action() -> serde_json::Value {
    json!({
        "id": "s1",
        "kind": "click",
        "intent": "Click the text editor",
        "target": {
            "window": { "process": "notepad.exe" },
            "selectors": [{ "kind": "automation_id", "value": "RichEditD2DPT" }]
        },
        "risk_class": "read",
        "grounding": "uia"
    })
}

fn type_note_action() -> serde_json::Value {
    json!({
        "id": "s2",
        "kind": "type",
        "intent": "Type the invoice note",
        "target": { "window": { "process": "notepad.exe" } },
        "params": { "text": "Invoice 2026-07-11 total $142.50" },
        "risk_class": "write",
        "grounding": "uia"
    })
}

fn save_action() -> serde_json::Value {
    json!({
        "id": "s3",
        "kind": "key",
        "intent": "Save the file",
        "target": { "window": { "process": "notepad.exe" } },
        "params": { "combo": "ctrl+s" },
        "risk_class": "write",
        "grounding": "uia"
    })
}

/// The scripted plan for the whole task in one `mock_planner` response:
/// click, type, save, then the sentinel `done` tool call.
fn scripted_task_planner() -> MockPlannerBackend {
    MockPlannerBackend::new(
        "mock_planner",
        vec![
            BackendEvent::ToolCall {
                id: "1".into(),
                name: "propose_action".into(),
                arguments: click_editor_action(),
            },
            BackendEvent::ToolCall {
                id: "2".into(),
                name: "propose_action".into(),
                arguments: type_note_action(),
            },
            BackendEvent::ToolCall {
                id: "3".into(),
                name: "propose_action".into(),
                arguments: save_action(),
            },
            BackendEvent::ToolCall {
                id: "4".into(),
                name: "done".into(),
                arguments: json!({}),
            },
        ],
    )
}

fn new_loop() -> ExploreLoop<MockSynthesizer> {
    let executor = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));
    ExploreLoop::new(
        notepad_perceiver(),
        Box::new(scripted_task_planner()),
        executor,
        "notepad.exe",
    )
}

#[tokio::test]
async fn explore_loop_completes_a_scripted_notepad_task_recording_and_gating_every_step() {
    let bus = Bus::new();
    let recorder = Recorder::open_in_memory().expect("open in-memory recorder");
    let run_events = bus.subscribe("run.*");

    let loop_ = new_loop();
    let summary = loop_
        .run(&bus, &recorder, GOAL, &mut NoControl)
        .await
        .expect("run completes without an infrastructure error");

    assert_eq!(summary.outcome, BusRunOutcome::Ok);
    assert_eq!(summary.steps, 3);
    assert!(summary.halted.is_none());

    // Every step is recorded ...
    let steps = recorder.list_steps(&summary.run_id).expect("list steps");
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].action.id, "s1");
    assert_eq!(steps[1].action.id, "s2");
    assert_eq!(steps[2].action.id, "s3");
    for s in &steps {
        assert_eq!(s.outcome, "ok");
        assert!(s.human_correction.is_none());
    }

    let envelopes: Vec<_> = run_events.rx.try_iter().collect();

    // ... and every step passed through a gate.
    let gated: Vec<RunStepGated> = envelopes
        .iter()
        .filter(|e| e.topic == "run.step.gated")
        .map(|e| serde_json::from_value(e.payload.clone()).unwrap())
        .collect();
    assert_eq!(gated.len(), 3, "one safety-gate check per proposed step");
    for g in &gated {
        assert_eq!(g.result, GateResult::Pass);
    }

    // run.started / run.step.executed / run.completed are published.
    assert_eq!(envelopes.first().unwrap().topic, "run.started");
    let started: RunStarted =
        serde_json::from_value(envelopes.first().unwrap().payload.clone()).unwrap();
    assert_eq!(started.run_id, summary.run_id);
    assert_eq!(started.goal, GOAL);

    let executed: Vec<RunStepExecuted> = envelopes
        .iter()
        .filter(|e| e.topic == "run.step.executed")
        .map(|e| serde_json::from_value(e.payload.clone()).unwrap())
        .collect();
    assert_eq!(executed.len(), 3);
    assert_eq!(executed[0].step_id, "s1");
    assert_eq!(executed[2].step_id, "s3");

    assert_eq!(envelopes.last().unwrap().topic, "run.completed");
    let completed: RunCompleted =
        serde_json::from_value(envelopes.last().unwrap().payload.clone()).unwrap();
    assert_eq!(completed.outcome, BusRunOutcome::Ok);
    assert_eq!(completed.steps, 3);

    // Proves the click actually resolved a real point and the executor
    // actually dispatched, not just that the loop believes it did.
    let calls = loop_.executor().synthesizer().calls();
    assert!(calls.iter().any(|c| matches!(c, SynthCall::ClickPoint(_))));
    assert!(calls
        .iter()
        .any(|c| matches!(c, SynthCall::TypeText(t) if t.contains("Invoice"))));
    assert!(calls
        .iter()
        .any(|c| matches!(c, SynthCall::Key(k) if k == "ctrl+s")));
}

/// D5: an explore run's model-call count is a REAL measured value, not a
/// hardcoded zero. The scripted planner returns its whole plan in ONE
/// `complete()` call, so the loop consults the planner exactly once: one round,
/// one model call. The count equals the number of rounds and surfaces on both
/// the [`RunSummary`] and the published `run.completed` event.
#[tokio::test]
async fn explore_run_reports_a_real_nonzero_model_call_count_equal_to_rounds() {
    let bus = Bus::new();
    let recorder = Recorder::open_in_memory().expect("open in-memory recorder");
    let completed = bus.subscribe("run.completed");

    let loop_ = new_loop();
    let summary = loop_
        .run(&bus, &recorder, GOAL, &mut NoControl)
        .await
        .expect("run completes without an infrastructure error");

    // The whole scripted plan arrives in a single planner response, so the loop
    // runs exactly one round: one model call.
    assert_eq!(
        summary.model_calls, 1,
        "one planner round consulted means one model call"
    );
    assert!(
        summary.model_calls > 0,
        "an explore run consults the planner, so its model-call count is nonzero"
    );

    let env = completed
        .rx
        .try_iter()
        .last()
        .expect("run.completed published");
    let completed_ev: RunCompleted =
        serde_json::from_value(env.payload.clone()).expect("run.completed payload parses");
    assert_eq!(
        completed_ev.model_calls, summary.model_calls,
        "the run.completed event carries the same real count as the summary"
    );
    assert!(completed_ev.model_calls > 0);
}

#[tokio::test]
async fn explore_loop_captures_a_mid_run_redirect_as_a_human_correction_and_finishes() {
    let bus = Bus::new();
    let recorder = Recorder::open_in_memory().expect("open in-memory recorder");
    let run_events = bus.subscribe("run.*");

    let loop_ = new_loop();
    let mut control = ScriptedControl::new([
        None, // nothing pending before step 1 ("click")
        Some(RunControl::Redirect(
            "Do not use the menu. Press Ctrl+S instead.".to_string(),
        )), // arrives mid-run, right before step 2 ("type")
        None, // nothing pending before step 3 ("key")
    ]);

    let summary = loop_
        .run(&bus, &recorder, GOAL, &mut control)
        .await
        .expect("run completes without an infrastructure error");

    assert_eq!(summary.outcome, BusRunOutcome::Ok);
    assert_eq!(
        summary.steps, 3,
        "the redirect annotates a step, it does not skip one"
    );
    assert!(summary.halted.is_none(), "the loop resumes and finishes");

    let steps = recorder.list_steps(&summary.run_id).unwrap();
    assert_eq!(steps.len(), 3);
    assert!(
        steps[0].human_correction.is_none(),
        "the redirect had not arrived yet for step 1"
    );
    let correction = steps[1]
        .human_correction
        .clone()
        .expect("the redirect is captured on the step right after it arrives");
    assert_eq!(
        correction["instruction"],
        json!("Do not use the menu. Press Ctrl+S instead.")
    );
    assert!(
        steps[2].human_correction.is_none(),
        "the correction attaches once, not to every later step"
    );

    let envelopes: Vec<_> = run_events.rx.try_iter().collect();
    let topics: Vec<&str> = envelopes.iter().map(|e| e.topic.as_str()).collect();
    let paused_at = topics
        .iter()
        .position(|t| *t == "run.paused")
        .expect("run.paused published");
    let redirected_at = topics
        .iter()
        .position(|t| *t == "run.redirected")
        .expect("run.redirected published");
    let resumed_at = topics
        .iter()
        .position(|t| *t == "run.resumed")
        .expect("run.resumed published");
    assert!(paused_at < redirected_at, "paused fires before redirected");
    assert!(redirected_at < resumed_at, "redirected fires before resumed");

    let redirected: RunRedirected =
        serde_json::from_value(envelopes[redirected_at].payload.clone()).unwrap();
    assert_eq!(
        redirected.instruction,
        "Do not use the menu. Press Ctrl+S instead."
    );

    assert_eq!(envelopes.last().unwrap().topic, "run.completed");
}

#[tokio::test]
async fn explicit_pause_blocks_until_resume_then_continues_without_a_correction() {
    let bus = Bus::new();
    let recorder = Recorder::open_in_memory().unwrap();
    let run_events = bus.subscribe("run.*");

    let loop_ = new_loop();
    let mut control = ScriptedControl::new([
        None,
        Some(RunControl::Pause),
        Some(RunControl::Resume),
        None,
    ]);

    let summary = loop_
        .run(&bus, &recorder, GOAL, &mut control)
        .await
        .expect("run completes");
    assert_eq!(summary.outcome, BusRunOutcome::Ok);
    assert_eq!(summary.steps, 3);

    let steps = recorder.list_steps(&summary.run_id).unwrap();
    assert!(
        steps.iter().all(|s| s.human_correction.is_none()),
        "a bare pause+resume carries no correction"
    );

    let envelopes: Vec<_> = run_events.rx.try_iter().collect();
    let topics: Vec<&str> = envelopes.iter().map(|e| e.topic.as_str()).collect();
    assert!(topics.contains(&"run.paused"));
    assert!(topics.contains(&"run.resumed"));
    assert!(!topics.contains(&"run.redirected"));
}

// ---- the safety gate actually blocks something -----------------------------

fn login_snapshot() -> operant_ir::Snapshot {
    serde_json::from_value(json!({
        "v": 1,
        "source": "fixture",
        "window": { "process": "login.exe", "title": "Sign in", "dpi_scale": 1.0 },
        "digest": "loginwindow00000000000000000000000000000000000000000000000000",
        "elements": [
            {
                "idx": 0, "parent": null, "role": "window", "name": "Sign in",
                "bounds": { "x": 0, "y": 0, "w": 400, "h": 300 }, "is_password": false
            },
            {
                "idx": 1, "parent": 0, "role": "edit", "name": "Password",
                "automation_id": "pwd",
                "bounds": { "x": 10, "y": 50, "w": 200, "h": 24 },
                "is_password": true
            }
        ]
    }))
    .expect("synthetic login snapshot parses")
}

fn type_password_action() -> serde_json::Value {
    json!({
        "id": "s1",
        "kind": "type",
        "target": {
            "window": { "process": "login.exe" },
            "selectors": [{ "kind": "automation_id", "value": "pwd" }]
        },
        "params": { "text": "hunter2" },
        "risk_class": "write",
        "grounding": "uia"
    })
}

#[tokio::test]
async fn the_safety_gate_blocks_a_credential_field_and_halts_the_run() {
    let bus = Bus::new();
    let recorder = Recorder::open_in_memory().unwrap();
    let run_events = bus.subscribe("run.*");

    let planner = MockPlannerBackend::new(
        "mock_planner",
        vec![BackendEvent::ToolCall {
            id: "1".into(),
            name: "propose_action".into(),
            arguments: type_password_action(),
        }],
    );
    let executor = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));
    let loop_ = ExploreLoop::new(
        Box::new(FixturePerceiver::single(login_snapshot())),
        Box::new(planner),
        executor,
        "login.exe",
    );

    let summary = loop_
        .run(&bus, &recorder, "Sign in", &mut NoControl)
        .await
        .unwrap();

    assert_eq!(summary.outcome, BusRunOutcome::Failed);
    assert_eq!(summary.halted, Some(HaltReason::Gate));
    assert_eq!(
        loop_.executor().synthesizer().call_count(),
        0,
        "the password field is never actually typed into"
    );

    // The blocked attempt is still recorded, for the audit trail.
    let steps = recorder.list_steps(&summary.run_id).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].outcome, "blocked");

    let envelopes: Vec<_> = run_events.rx.try_iter().collect();
    assert!(envelopes.iter().any(|e| e.topic == "run.halted"));
    let gated: RunStepGated = envelopes
        .iter()
        .find(|e| e.topic == "run.step.gated")
        .map(|e| serde_json::from_value(e.payload.clone()).unwrap())
        .unwrap();
    assert_eq!(gated.result, GateResult::Fail);
}
