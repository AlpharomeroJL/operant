//! Integration test for watch-and-suggest (X5): a repeated manual task is
//! detected, offered once, and -- on acceptance -- seeds a real supervised
//! EXPLORE run through L7A's [`ExploreLoop`], headless, with no network or GPU.
//!
//! This proves the end-to-end wiring the brief calls for: the detector hands
//! off an [`ExploreSeed`] and the caller drives the existing explore loop with
//! it; the watch module never restructures that loop.

use operant_action::{Executor, MockSynthesizer, NoopSleeper};
use operant_core::bus::events::RunOutcome as BusRunOutcome;
use operant_core::{Bus, Perceiver};
use operant_ir::{Action, ActionKind, Grounding, RiskClass, Selector, Target};
use operant_orchestrator::backends::{BackendEvent, MockPlannerBackend};
use operant_orchestrator::explore::{ExploreLoop, NoControl};
use operant_orchestrator::watch::{ManualEvent, WatchConfig, Watcher};
use operant_perception_uia::FixturePerceiver;
use operant_recorder::Recorder;
use serde_json::json;

fn notepad_perceiver() -> Box<dyn Perceiver> {
    let raw = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
    Box::new(FixturePerceiver::from_json(raw).expect("shared notepad fixture parses"))
}

/// One manual step of the repeated task, targeting a stable automation id.
fn manual_step(id: &str, kind: ActionKind, automation_id: &str) -> ManualEvent {
    ManualEvent::new(Action {
        v: 1,
        id: id.to_string(),
        kind,
        intent: None,
        target: Some(Target {
            selectors: vec![Selector::AutomationId { value: automation_id.to_string() }],
            ..Default::default()
        }),
        params: serde_json::Map::new(),
        pace: Default::default(),
        risk_class: RiskClass::Write,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5000,
        retry: Default::default(),
    })
}

/// The scripted planner that carries out the seeded goal: click the editor,
/// type, save, done. Mirrors the explore-loop test's happy path.
fn scripted_task_planner() -> MockPlannerBackend {
    MockPlannerBackend::new(
        "mock_planner",
        vec![
            BackendEvent::ToolCall {
                id: "1".into(),
                name: "propose_action".into(),
                arguments: json!({
                    "id": "s1", "kind": "click", "intent": "Click the editor",
                    "target": { "window": { "process": "notepad.exe" },
                        "selectors": [{ "kind": "automation_id", "value": "RichEditD2DPT" }] },
                    "risk_class": "read", "grounding": "uia"
                }),
            },
            BackendEvent::ToolCall {
                id: "2".into(),
                name: "propose_action".into(),
                arguments: json!({
                    "id": "s2", "kind": "type", "intent": "Type the note",
                    "target": { "window": { "process": "notepad.exe" } },
                    "params": { "text": "weekly status" },
                    "risk_class": "write", "grounding": "uia"
                }),
            },
            BackendEvent::ToolCall {
                id: "3".into(),
                name: "propose_action".into(),
                arguments: json!({
                    "id": "s3", "kind": "key", "intent": "Save",
                    "target": { "window": { "process": "notepad.exe" } },
                    "params": { "combo": "ctrl+s" },
                    "risk_class": "write", "grounding": "uia"
                }),
            },
            BackendEvent::ToolCall { id: "4".into(), name: "done".into(), arguments: json!({}) },
        ],
    )
}

#[tokio::test]
async fn accepting_a_suggestion_seeds_a_supervised_explore_run() {
    let bus = Bus::new();
    let suggestions = bus.subscribe("suggestion.*");

    // The feature is opt-in; the user has turned it on.
    let mut watcher = Watcher::capped(WatchConfig { enabled: true, ..WatchConfig::default() });

    // The user performs the same three-step task four times by hand.
    let mut suggestion_id = None;
    for _ in 0..4 {
        for (id, kind, aid) in [
            ("click", ActionKind::Click, "SubjectField"),
            ("type", ActionKind::Type, "SubjectField"),
            ("save", ActionKind::Key, "Editor"),
        ] {
            if let Some(offer) = watcher.observe(&bus, &manual_step(id, kind, aid)) {
                suggestion_id = Some(offer.suggestion_id);
            }
        }
    }
    let suggestion_id = suggestion_id.expect("the repeated task produced one offer");

    // Accepting yields a seed for the supervised run.
    let seed = watcher
        .accept(&bus, &suggestion_id)
        .expect("the open offer accepts");
    assert!(!seed.goal.is_empty());
    assert_eq!(seed.steps.len(), 3);

    // Drive L7A's explore loop with the seeded goal: a real supervised run.
    let executor = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));
    let explore = ExploreLoop::new(
        notepad_perceiver(),
        Box::new(scripted_task_planner()),
        executor,
        "notepad.exe",
    );
    let recorder = Recorder::open_in_memory().expect("in-memory recorder");
    let summary = explore
        .run(&bus, &recorder, &seed.goal, &mut NoControl)
        .await
        .expect("the seeded supervised run completes without an infrastructure error");

    assert_eq!(summary.outcome, BusRunOutcome::Ok);
    assert_eq!(summary.steps, 3, "the seeded run carried out the three-step task");

    // The suggestion lifecycle went over the bus: offered then accepted.
    let topics: Vec<_> = suggestions.rx.try_iter().map(|e| e.topic).collect();
    assert!(topics.contains(&"suggestion.offered".to_string()));
    assert!(topics.contains(&"suggestion.accepted".to_string()));
}
