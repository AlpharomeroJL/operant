//! NFR-6 / NFR-7 headless golden path (thesis proof): explore once with a
//! model, replay forever without one. Drives the FULL loop end to end with
//! zero network/GPU:
//!
//!   EXPLORE  operant_orchestrator::explore::ExploreLoop, driven by a
//!            scripted mock_planner (`MockPlannerBackend`) against a fixed
//!            `FixturePerceiver` snapshot and a `MockSynthesizer`, records a
//!            scripted Notepad-style task into an in-memory
//!            `operant_recorder::Recorder`.
//!
//!   COMPILE  `operant_compiler::compile_records` turns the recorded steps
//!            straight into a `CompiledWorkflow` (manifest + ordered
//!            actions). It already IS the bridge between the recorder's row
//!            shape and the compiler's pipeline, so this test does not need
//!            to hand-roll a translation from `operant_recorder::StepRecord`
//!            into `operant_compiler::Trajectory` itself; see the test body
//!            for the one bridge this file does still have to do by hand
//!            (`operant_compiler::CompiledWorkflow` -> `operant_replay::CompiledWorkflow`).
//!
//!   REPLAY   `operant_replay::Replayer`, backed by a completely FRESH
//!            `MockSynthesizer` that has never seen a single call, drives
//!            the compiled workflow and is asserted to reproduce explore's
//!            own synthesizer call sequence (same focus/click/type/key
//!            calls, same coordinates, same text) and to pass the
//!            postcondition gate the compiler derived from the trajectory's
//!            outcome-bearing assert step.
//!
//! The explore phase mirrors the proven pattern in
//! `crates/orchestrator/tests/explore_loop.rs` (click the editor, type an
//! invoice note, save); this file's own job is the wiring between the three
//! phases and the zero-model-calls property of the replay half, not
//! re-proving what each crate's own unit tests already cover.

use std::collections::BTreeMap;

use operant_action::{Executor, MockSynthesizer, NoopSleeper};
use operant_core::bus::events::RunOutcome as BusRunOutcome;
use operant_core::{Bus, Perceiver};
use operant_gates::{evaluate_gate, EvalContext};
use operant_ir::{
    Action, ActionKind, Bounds, Element, Gate, GateKind, GateResult, Grounding, NameRoleSeg,
    OnFail, Pace, Retry, RiskClass, Role, Selector, Snapshot, SnapshotSource, Target, WindowInfo,
    WindowMatch,
};
use operant_orchestrator::backends::{BackendEvent, MockPlannerBackend};
use operant_orchestrator::explore::{ExploreLoop, NoControl};
use operant_perception_uia::FixturePerceiver;
use operant_recorder::{NewStep, Recorder};
use serde_json::json;

const GOAL: &str = "Write an invoice note in Notepad and save it";
const INVOICE_TEXT: &str = "Invoice 2026-07-11 total $142.50";
const POST_REGEX: &str = r"^Invoice \d{4}-\d{2}-\d{2} total \$\d+\.\d{2}$";

// ---- the fixture world: one static Notepad snapshot ------------------------
//
// A single fixed snapshot, reused for perception during EXPLORE and for the
// pre/post gate contexts during REPLAY. Its "Text editor" element already
// carries the invoice text: FixturePerceiver never simulates typing (it
// answers every `snapshot()` call from fixed data, see
// crates/perception-uia/src/fixture.rs), so this is the fixture-mode
// equivalent of "the state after the run" -- the same simplification
// crates/orchestrator/tests/explore_loop.rs's own `login_snapshot()` helper
// makes for its safety-gate test.
//
// Because the snapshot never changes, every step's before/after digest is
// identical, so the compiler's wait-insertion pass (pass 4) correctly
// synthesizes zero `wait` steps for this run. `contracts/fixtures/trajectory_notepad.json`
// exercises that pass with a hand-authored digest change; it is already
// covered by crates/compiler's own unit tests, so it is not duplicated here.
fn notepad_snapshot() -> Snapshot {
    Snapshot {
        v: 1,
        source: SnapshotSource::Fixture,
        window: WindowInfo {
            hwnd: None,
            process: "notepad.exe".to_string(),
            title: "Untitled - Notepad".to_string(),
            monitor: Some("MON1".to_string()),
            dpi_scale: 1.0,
        },
        digest: "d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0".to_string(),
        truncated: false,
        captured_ms: Some(41),
        elements: vec![
            Element {
                idx: 0,
                parent: None,
                role: Role::Window,
                name: "Untitled - Notepad".to_string(),
                value: None,
                automation_id: None,
                bounds: Some(Bounds {
                    x: 100.0,
                    y: 100.0,
                    w: 1200.0,
                    h: 800.0,
                    monitor: Some("MON1".to_string()),
                }),
                enabled: true,
                offscreen: false,
                is_password: false,
                patterns: vec![],
                selectors: vec![],
            },
            Element {
                idx: 1,
                parent: Some(0),
                role: Role::Document,
                name: "Text editor".to_string(),
                value: Some(INVOICE_TEXT.to_string()),
                automation_id: Some("RichEditD2DPT".to_string()),
                bounds: Some(Bounds {
                    x: 100.0,
                    y: 156.0,
                    w: 1200.0,
                    h: 716.0,
                    monitor: Some("MON1".to_string()),
                }),
                enabled: true,
                offscreen: false,
                is_password: false,
                patterns: vec!["value".to_string(), "text".to_string()],
                selectors: vec![
                    Selector::AutomationId {
                        value: "RichEditD2DPT".to_string(),
                    },
                    Selector::NameRolePath {
                        path: vec![
                            NameRoleSeg {
                                role: "window".to_string(),
                                name: "Untitled - Notepad".to_string(),
                            },
                            NameRoleSeg {
                                role: "document".to_string(),
                                name: "Text editor".to_string(),
                            },
                        ],
                    },
                ],
            },
        ],
    }
}

fn notepad_perceiver() -> Box<dyn Perceiver> {
    Box::new(FixturePerceiver::single(notepad_snapshot()))
}

// ---- the scripted mock_planner task ----------------------------------------
//
// Click the editor, type the invoice note, save. Same narrative as
// crates/orchestrator/tests/explore_loop.rs, with one addition: the click's
// `coords_last_known` is filled in up front. ExploreLoop records the Action
// IR exactly as the planner proposed it; it does NOT write the point
// perception resolved back into the recorded step (see
// `replay_needs_the_coords_a_live_perceiver_would_have_resolved` below for
// why that matters). The value here is not arbitrary: it is the exact
// center-of-bounds `crates/perception-uia/src/resolve.rs`'s own
// `resolve_in_snapshot` computes for the "Text editor" element's bounds
// above ((100 + 1200/2), (156 + 716/2)), so the point EXPLORE resolves live
// and the point REPLAY replays from a cached value are the same point.

fn click_editor_action() -> serde_json::Value {
    json!({
        "id": "s1",
        "kind": "click",
        "intent": "Click the text editor",
        "target": {
            "window": { "process": "notepad.exe" },
            "selectors": [
                { "kind": "automation_id", "value": "RichEditD2DPT" },
                { "kind": "name_role_path", "path": [
                    { "role": "window", "name": "Untitled - Notepad" },
                    { "role": "document", "name": "Text editor" }
                ] }
            ],
            "coords_last_known": { "x": 700.0, "y": 514.0, "monitor": "MON1", "dpi_scale": 1.0 }
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
        "params": { "text": INVOICE_TEXT },
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

/// The whole task as one `mock_planner` response: click, type, save, then
/// the sentinel `done` tool call. `MockPlannerBackend` replays this fixed
/// script on every `complete()` call regardless of the request content
/// (`crates/orchestrator/src/backends/mock_backends.rs`), so ExploreLoop's
/// single round consumes it as one propose-action batch
/// (`crates/orchestrator/src/explore/mod.rs`'s own module doc: "a single
/// `complete()` call may return a batch of several `propose_action` calls").
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

/// The outcome-bearing postcondition step. Built directly as `operant_ir::Action`
/// (there is no planner script for it -- see the bridge note in the test
/// body for why) rather than through the mock_planner's JSON, mirroring
/// `contracts/fixtures/trajectory_notepad.json`'s own final step: an
/// anchored regex match against the "Text editor" element's value.
fn postcondition_assert_action() -> Action {
    let mut params = serde_json::Map::new();
    params.insert(
        "expr".to_string(),
        json!({
            "op": "matches",
            "query": { "kind": "snapshot_element_value", "role": "document", "name": "Text editor" },
            "regex": POST_REGEX,
        }),
    );
    Action {
        v: 1,
        id: "s4".to_string(),
        kind: ActionKind::Assert,
        intent: Some("Check that the note was written".to_string()),
        target: Some(Target {
            window: Some(WindowMatch {
                process: Some("notepad.exe".to_string()),
                title_pattern: None,
            }),
            selectors: vec![],
            anchor: None,
            coords_last_known: None,
        }),
        params,
        pace: Pace::Instant,
        risk_class: RiskClass::Read,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5000,
        retry: Retry {
            attempts: 0,
            backoff_ms: 0,
        },
    }
}

fn new_explore_loop() -> ExploreLoop<MockSynthesizer> {
    let executor = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));
    ExploreLoop::new(
        notepad_perceiver(),
        Box::new(scripted_task_planner()),
        executor,
        "notepad.exe",
    )
}

// ---- structural half of the zero-model-replay proof ------------------------

/// The thesis's structural guarantee: `operant-replay` cannot make a model
/// or network call during REPLAY because it does not link anything that
/// could. Mirrors `crates/replay/src/lib.rs`'s own `replay_crate_is_backend_free`
/// unit test and `scripts/check_airgap.mjs`'s CI gate, but run here
/// independently, from OUTSIDE the replay crate, over the exact manifest the
/// workspace builds (`include_str!` is resolved by rustc at compile time
/// relative to this file, so this reads the real file on disk, not a copy).
#[test]
fn replay_crate_has_no_model_or_network_dependency() {
    let toml = include_str!("../../../crates/replay/Cargo.toml");

    // Sanity: this really is operant-replay's own manifest, wired up the way
    // this test assumes, not an empty or unrelated file.
    assert!(toml.contains("name = \"operant-replay\""));
    assert!(toml.contains("operant-ir"));
    assert!(toml.contains("operant-action"));
    assert!(toml.contains("operant-gates"));

    let (runtime, _dev) = toml
        .split_once("[dev-dependencies]")
        .unwrap_or((toml, ""));
    for banned in [
        "operant-orchestrator",
        "operant-scheduler",
        "operant-registry",
        "reqwest",
        "tokio",
        "hyper",
        "reticle",
    ] {
        assert!(
            !runtime.contains(banned),
            "replay runtime deps must not include `{banned}`; zero-model replay is enforced \
             by the crate graph, not a runtime flag"
        );
    }
}

// ---- the golden path itself -------------------------------------------------

#[tokio::test]
async fn golden_path_explores_compiles_and_replays_with_zero_model_calls() {
    // ---- EXPLORE: ExploreLoop + mock_planner + FixturePerceiver + MockSynthesizer ----
    let bus = Bus::new();
    let recorder = Recorder::open_in_memory().expect("open in-memory recorder");

    let explore_loop = new_explore_loop();
    let summary = explore_loop
        .run(&bus, &recorder, GOAL, &mut NoControl)
        .await
        .expect("explore run completes without an infrastructure error");

    assert_eq!(
        summary.outcome,
        BusRunOutcome::Ok,
        "the scripted task reaches the planner's own done signal"
    );
    assert_eq!(summary.steps, 3);
    assert!(summary.halted.is_none());

    // The action layer's own record of what happened: proof the click
    // really resolved through perception and the executor really
    // dispatched, not just that the loop believes it did. One focus + one
    // kind-specific call per step (crates/action/src/executor.rs's
    // `dispatch_once` focuses the target window, unconditionally on kind,
    // before doing anything else), three steps.
    let explore_calls = explore_loop.executor().synthesizer().calls();
    assert_eq!(explore_calls.len(), 6);

    // Bridge (see this test's module doc): ExploreLoop's Executor refuses to
    // dispatch `assert` by design -- "assert is evaluated against a
    // perception snapshot by the gate engine (C9, operant-gates), not the
    // action layer" (crates/action/src/executor.rs's `dispatch_once`). A
    // live 3-step ExploreLoop run can therefore never itself produce the
    // outcome-bearing assert step contracts/fixtures/trajectory_notepad.json's
    // shape (and the compiler's pass 4) expect. That step is appended to
    // the SAME run directly through the recorder here, exactly mirroring
    // how that fixture's own final step was produced -- and evaluated for
    // real against the fixture snapshot first, through the same
    // operant_gates::evaluate_gate the compiled postcondition will run
    // through at replay time, so the "ok" outcome recorded below is earned,
    // not assumed.
    let snapshot = notepad_snapshot();
    let assert_action = postcondition_assert_action();
    let expr = assert_action
        .params
        .get("expr")
        .expect("the assert action carries its predicate")
        .clone();
    let post_gate = Gate {
        step_ref: None,
        kind: GateKind::Post,
        expr,
        on_fail: OnFail::Halt,
    };
    let verdict = evaluate_gate(&post_gate, &EvalContext::new().with_snapshot(snapshot.clone()))
        .expect("well-formed predicate");
    assert_eq!(
        verdict,
        GateResult::Pass,
        "the fixture snapshot really does satisfy the postcondition"
    );

    recorder
        .record_step(
            &summary.run_id,
            NewStep::new(4, assert_action, Grounding::Uia, "ok", 60)
                .with_digests(Some(snapshot.digest.clone()), Some(snapshot.digest.clone()))
                .outcome_bearing(true),
        )
        .expect("record the postcondition step");

    // ---- COMPILE: operant_compiler::compile_records straight from the recorder ----
    let run_row = recorder
        .get_run(&summary.run_id)
        .expect("query the run row")
        .expect("run row present");
    let steps = recorder
        .list_steps(&summary.run_id)
        .expect("list recorded steps");
    assert_eq!(steps.len(), 4, "click, type, key, and the appended assert");

    let compilation = operant_compiler::compile_records(&run_row.goal, &summary.run_id, &steps)
        .expect("the recorded trajectory compiles");

    let kinds: Vec<ActionKind> = compilation
        .workflow
        .actions
        .iter()
        .map(|a| a.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            ActionKind::Click,
            ActionKind::Type,
            ActionKind::Key,
            ActionKind::Assert,
        ],
        "no synthesized waits: the fixture snapshot is static, so no step's before/after digest \
         ever differs and pass 4 has nothing to insert one for"
    );
    assert_eq!(
        compilation.workflow.manifest.capabilities.apps,
        vec!["notepad.exe".to_string()]
    );
    assert_eq!(
        compilation.workflow.manifest.capabilities.risk_ceiling,
        RiskClass::Write
    );
    assert_eq!(
        compilation.workflow.manifest.gates.len(),
        2,
        "one pre gate (foreground process == notepad.exe) and one post gate (the assert)"
    );

    // Bridge (see this test's module doc): operant_compiler::CompiledWorkflow
    // and operant_replay::CompiledWorkflow are deliberately separate Rust
    // types -- crates/replay/src/lib.rs's own module doc: "defined here so
    // the replay crate need not depend on the compiler (which would drag in
    // the recorder and break the backend-free crate graph)" -- kept
    // JSON-compatible on purpose. Cross that boundary the same way a real
    // deployment would (a compiled workflow persisted to disk on the
    // machine that explored, then loaded on a machine that only ever
    // replays): serialize what COMPILE produced, deserialize it into
    // replay's own copy of the shape.
    let persisted =
        serde_json::to_string(&compilation.workflow).expect("compiled workflow serializes");
    let replay_workflow: operant_replay::CompiledWorkflow = serde_json::from_str(&persisted)
        .expect("compiled workflow deserializes into the replay crate's own dependency-free shape");

    // ---- REPLAY: operant_replay::Replayer against a completely FRESH MockSynthesizer ----
    let replayer = operant_replay::Replayer::with_mock();
    assert_eq!(
        replayer.synthesizer().call_count(),
        0,
        "a fresh synthesizer that has never seen a single call: replay starts from nothing, \
         not from explore's own executor"
    );

    let gate_ctx = EvalContext::new().with_snapshot(snapshot.clone());
    let report = replayer
        .replay_compiled(&replay_workflow, &BTreeMap::new(), &gate_ctx, &gate_ctx)
        .expect("replay reproduces the compiled workflow with zero model or network calls");

    assert_eq!(
        report.steps_executed, 3,
        "click, type, key; assert is never dispatched, it is the postcondition gate \
         (operant_replay::Replayer::replay skips ActionKind::Assert on purpose)"
    );
    assert_eq!(report.pre, vec![GateResult::Pass]);
    assert_eq!(
        report.post,
        vec![GateResult::Pass],
        "the postcondition the compiler derived from the trajectory's assert step holds on \
         replay too"
    );

    // The thesis proof itself: replay reproduces explore's exact
    // synthesizer call sequence -- same focus/click/type/key calls, same
    // resolved coordinates, same typed text -- with no planner, no
    // ModelBackend, and no MockPlannerBackend anywhere in this second half
    // of the test (structurally impossible to reach one: nothing above this
    // point imports operant_orchestrator, and `replay_crate_has_no_model_or_network_dependency`
    // proves operant-replay itself cannot link one either).
    let replay_calls = replayer.synthesizer().calls();
    assert_eq!(
        replay_calls, explore_calls,
        "replay reproduces explore's exact synthesizer call sequence with zero model calls: \
         explore once with a model, replay forever without one"
    );
}

/// Isolates the coordinate-bridging fact called out in `click_editor_action`'s
/// own comment: EXPLORE resolves a click's screen point live, through
/// perception, every time; REPLAY has no perception dependency at all and
/// can only click a `coords_last_known` baked into the compiled action at
/// compile time. That value survives compilation unchanged (pass 3,
/// `selectorize`, only reorders `target.selectors`; it never touches
/// `target.coords_last_known`), so it has to be correct where the planner
/// (here, the scripted mock_planner) first proposes the step, or replay's
/// click has nothing to click. This test pins that down directly, rather
/// than leaving it as an implicit precondition of the click matching in the
/// golden path above.
#[test]
fn scripted_click_coords_match_what_the_fixture_perceiver_actually_resolves() {
    let snapshot = notepad_snapshot();
    let action: Action = serde_json::from_value(click_editor_action())
        .expect("the scripted click action parses as Action IR");
    let scripted = action
        .target
        .as_ref()
        .and_then(|t| t.coords_last_known.as_ref())
        .expect("the scripted click carries a coords_last_known hint");

    let perceiver = FixturePerceiver::single(snapshot.clone());
    let live = perceiver
        .resolve(&snapshot, &action.target.as_ref().unwrap().selectors)
        .expect("the automation_id selector resolves against the fixture snapshot");

    assert_eq!(scripted.x, live.x);
    assert_eq!(scripted.y, live.y);
    assert_eq!(scripted.monitor, live.monitor);
}
