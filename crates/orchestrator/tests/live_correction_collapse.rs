//! KI-2 regression: a correction injected LIVE mid-run must fold into the
//! compiled workflow exactly the way the hand-authored
//! `contracts/fixtures/trajectory_notepad.json` correction does.
//!
//! The fixture records `human_correction.supersedes_seq` on the corrected
//! step and the compiler's normalize pass drops the superseded step. This
//! test drives the real EXPLORE loop through a mid-run redirect and then
//! compiles the recorded run with `operant_compiler::compile_records`,
//! asserting the misstep collapses away just like the fixture. Before the
//! field was reconciled the loop recorded `at_seq` (which the compiler never
//! reads), so the live correction annotated but never collapsed.

use operant_action::{Executor, MockSynthesizer, NoopSleeper};
use operant_core::bus::events::RunOutcome as BusRunOutcome;
use operant_core::{Bus, Perceiver};
use operant_ir::ActionKind;
use operant_orchestrator::backends::{BackendEvent, MockPlannerBackend};
use operant_orchestrator::explore::{ExploreLoop, RunControl, ScriptedControl};
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

/// The misstep the human corrects away: the wrong keystroke (open Print),
/// recorded "ok" by the loop because the loop cannot know it is wrong -- only
/// the human, watching, does.
fn misstep_action() -> serde_json::Value {
    json!({
        "id": "s2",
        "kind": "key",
        "intent": "Open the print dialog (the misstep)",
        "target": { "window": { "process": "notepad.exe" } },
        "params": { "combo": "ctrl+p" },
        "risk_class": "write",
        "grounding": "uia"
    })
}

/// The corrected step the redirect lands on: save with Ctrl+S.
fn corrected_save_action() -> serde_json::Value {
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

fn scripted_planner() -> MockPlannerBackend {
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
                arguments: misstep_action(),
            },
            BackendEvent::ToolCall {
                id: "3".into(),
                name: "propose_action".into(),
                arguments: corrected_save_action(),
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
        Box::new(scripted_planner()),
        executor,
        "notepad.exe",
    )
}

#[tokio::test]
async fn a_live_redirect_collapses_in_the_compiler_like_the_fixture_correction() {
    let bus = Bus::new();
    let recorder = Recorder::open_in_memory().expect("open in-memory recorder");

    // The redirect arrives right before the corrected save (step 3), exactly
    // as a human would step in after seeing the wrong keystroke land.
    let mut control = ScriptedControl::new([
        None, // before step 1 (click)
        None, // before step 2 (the misstep)
        Some(RunControl::Redirect(
            "Do not open the print dialog. Press Ctrl+S instead.".to_string(),
        )), // before step 3 (the corrected save)
    ]);

    let loop_ = new_loop();
    let summary = loop_
        .run(&bus, &recorder, GOAL, &mut control)
        .await
        .expect("run completes without an infrastructure error");

    assert_eq!(summary.outcome, BusRunOutcome::Ok);
    assert_eq!(summary.steps, 3, "click, misstep, corrected save all recorded");

    // ---- the live correction is recorded on the corrected step, naming the
    // step it supersedes (the misstep at seq 2), the SAME field the fixture
    // uses and the compiler reads ----
    let steps = recorder.list_steps(&summary.run_id).unwrap();
    assert_eq!(steps.len(), 3);
    assert!(steps[0].human_correction.is_none());
    assert!(steps[1].human_correction.is_none());
    let correction = steps[2]
        .human_correction
        .clone()
        .expect("the redirect is captured on the corrected step");
    assert_eq!(
        correction["instruction"],
        json!("Do not open the print dialog. Press Ctrl+S instead.")
    );
    assert_eq!(
        correction["supersedes_seq"], 2,
        "the live correction must name the misstep it supersedes, the field the compiler collapses on"
    );

    // ---- compile the recorded run straight through the real recorder->compiler
    // path and confirm the misstep collapsed away, identical to the fixture ----
    let compilation = operant_compiler::compile_records(GOAL, &summary.run_id, &steps)
        .expect("the recorded trajectory compiles");
    let actions = &compilation.workflow.actions;

    let combos: Vec<&str> = actions
        .iter()
        .filter(|a| a.kind == ActionKind::Key)
        .filter_map(|a| a.params.get("combo").and_then(|v| v.as_str()))
        .collect();
    assert!(
        !combos.contains(&"ctrl+p"),
        "the superseded misstep (ctrl+p) must be dropped by normalize, got {combos:?}"
    );
    assert!(
        combos.contains(&"ctrl+s"),
        "the corrected save (ctrl+s) must survive, got {combos:?}"
    );

    // The click that opened the run survives; only the misstep collapses.
    assert!(actions.iter().any(|a| a.kind == ActionKind::Click));
    assert!(
        !actions
            .iter()
            .any(|a| a.intent.as_deref() == Some("Open the print dialog (the misstep)")),
        "no trace of the superseded misstep survives compilation"
    );
}
