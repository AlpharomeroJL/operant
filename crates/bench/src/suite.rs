//! The real benchmark suite (C17, FR-D3): builds the fixture tasks, drives
//! each one through `operant_replay::Replayer` for 5 repetitions per task
//! per mode, and turns the raw measurements into `BenchResult` rows.
//!
//! Tasks: the fixture Notepad task (compiled from
//! `contracts/fixtures/trajectory_notepad.json`, same path
//! `crates/replay/tests/replay_notepad.rs` exercises), the fixture web task,
//! and the drift fixture post-repair task (both hand-built over
//! `contracts/fixtures/drift_renamed_button/{before,after}.json`, the
//! fixture webapp's snapshots; see that fixture's own README). Modes:
//! `replay` (compiled, zero model calls by construction) and `reinfer_mock`
//! (recorded replay latencies plus a documented simulated per-step
//! re-inference overhead, standing in for a planner that re-plans every
//! step). `reinfer_real` is out of scope here: `docs/specs/bench.md` gates
//! it behind a real backend, which this lane does not wire up.
//!
//! Per-step timing note: `Replayer::replay` dispatches a whole compiled
//! workflow as a single call and exposes no per-step timing hook, and
//! `crates/replay` is outside this lane's owned paths, so bench measures
//! wall time around each whole-workflow call and divides by the workflow's
//! dispatchable action count (every action except the trailing `assert`,
//! which is evaluated as the postcondition gate, never dispatched) to
//! approximate a per-step figure. That approximation, and the fact that
//! `reinfer_mock` never calls a real backend, is what the methods/honesty
//! section in `render_benchmarks_md` states plainly rather than leaving
//! implicit.

use std::collections::BTreeMap;
use std::time::Instant;

use operant_compiler::{compile, Trajectory};
use operant_gates::EvalContext;
use operant_ir::{
    Action, ActionKind, Capabilities, Coords, DslRef, Gate, GateKind, Grounding, Manifest, OnFail,
    Pace, Retry, RiskClass, Role, Selector, Snapshot, Target, WindowMatch,
};
use operant_replay::{CompiledWorkflow, Replayer};
use serde_json::json;

use crate::{BenchMode, BenchResult};

const NOTEPAD_TRAJECTORY: &str =
    include_str!("../../../contracts/fixtures/trajectory_notepad.json");
const NOTEPAD_SNAPSHOT: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
const NOTEPAD_TYPED_TEXT: &str = "Invoice 2026-07-11 total $142.50";

const WEBAPP_BEFORE: &str =
    include_str!("../../../contracts/fixtures/drift_renamed_button/before.json");
const WEBAPP_AFTER: &str =
    include_str!("../../../contracts/fixtures/drift_renamed_button/after.json");
const WEBAPP_CUSTOMER: &str = "Acme Co";
const WEBAPP_AMOUNT: &str = "250.00";

/// Repetitions per task per mode. `docs/specs/bench.md`: "success boolean
/// over 5 repetitions".
pub const REPETITIONS: i32 = 5;

// The CI regression threshold (p50 step under 150ms) lives in
// `threshold::MAX_P50_STEP_MS`, the one source for that number; this module
// only produces the rows it checks.

/// Simulated per-step re-inference overhead for `reinfer_mock`: the added
/// cost of asking a planner before every step instead of replaying a
/// compiled action list straight through. A documented constant, not a
/// measurement, exactly like the honesty note `render_benchmarks_md` emits
/// says: "reinfer_mock uses recorded latencies from the actual replay,
/// simulating agent-at-every-step cost without hitting a real backend."
const REINFER_STEP_OVERHEAD_MS: f64 = 6.0;

/// Illustrative tokens-per-call estimate for `reinfer_mock`'s `tokens`
/// column. Also a documented constant, not a measurement: there is no real
/// backend behind `reinfer_mock`, so there are no real tokens to count.
const REINFER_TOKENS_PER_CALL: i32 = 180;

/// One bench task: a compiled workflow plus the replay inputs and gate
/// contexts it needs.
pub struct TaskFixture {
    pub name: &'static str,
    pub workflow: CompiledWorkflow,
    pub inputs: BTreeMap<String, String>,
    pub pre: EvalContext,
    pub post: EvalContext,
}

// ---- notepad task: compile the frozen trajectory fixture --------------------

fn notepad_workflow() -> CompiledWorkflow {
    let traj: Trajectory =
        serde_json::from_str(NOTEPAD_TRAJECTORY).expect("fixture trajectory parses");
    let comp = compile(&traj).expect("fixture trajectory compiles");
    // Round-trip through JSON so this exercises the same serde contract
    // production code crosses (compiler -> replay), not a shared in-process
    // type; mirrors crates/replay/tests/replay_notepad.rs.
    let json = serde_json::to_value(&comp.workflow).expect("compiled workflow serializes");
    serde_json::from_value(json).expect("compiled workflow deserializes into replay's shape")
}

fn notepad_snapshot() -> Snapshot {
    serde_json::from_str(NOTEPAD_SNAPSHOT).expect("notepad snapshot fixture parses")
}

fn notepad_snapshot_with_editor(value: &str) -> Snapshot {
    let mut snap = notepad_snapshot();
    for e in &mut snap.elements {
        if e.role == Role::Document && e.name == "Text editor" {
            e.value = Some(value.to_string());
        }
    }
    snap
}

/// The fixture Notepad task: click, type the invoice note, save.
pub fn notepad_task() -> TaskFixture {
    TaskFixture {
        name: "notepad",
        workflow: notepad_workflow(),
        inputs: BTreeMap::new(),
        pre: EvalContext::new().with_snapshot(notepad_snapshot()),
        post: EvalContext::new().with_snapshot(notepad_snapshot_with_editor(NOTEPAD_TYPED_TEXT)),
    }
}

/// The same task with a deliberately broken postcondition: the post
/// snapshot's editor is left empty, so the compiled workflow's postcondition
/// assert fails on every repetition. Used by the negative test proving the
/// CI regression threshold actually catches a regression rather than always
/// passing; mirrors `crates/replay/tests/replay_notepad.rs`'s own
/// `postcondition_fails_when_the_note_was_not_written`.
pub fn notepad_task_with_broken_postcondition() -> TaskFixture {
    TaskFixture {
        post: EvalContext::new().with_snapshot(notepad_snapshot_with_editor("")),
        ..notepad_task()
    }
}

// ---- web task and drift-repaired task: hand-built over the webapp fixture ---
//
// Neither has a pre-compiled workflow fixture (contracts/fixtures/README.md
// lists a "Compiled workflow" fixture only for notepad); the fixture webapp
// itself is "consumed by browser adapter, e2e, demo mode, capture", not the
// compiler. Built directly as operant_ir types instead, the same way
// e2e/golden-path/tests/golden_path.rs hand-builds its postcondition assert
// action rather than inventing a new trajectory fixture file (this lane owns
// crates/bench and BENCHMARKS.md only, not contracts/fixtures).

fn webapp_snapshot(raw: &str) -> Snapshot {
    serde_json::from_str(raw).expect("webapp snapshot fixture parses")
}

/// A copy of a webapp snapshot with the Customer and Amount fields filled
/// in, standing in for "the state after the run" the same way
/// `notepad_snapshot_with_editor` does for Notepad: snapshots are static
/// fixtures, not simulated, so the post-condition context is a distinct
/// snapshot object built to reflect what the fields hold after the type
/// actions run.
fn webapp_snapshot_with_values(raw: &str, customer: &str, amount: &str) -> Snapshot {
    let mut snap = webapp_snapshot(raw);
    for e in &mut snap.elements {
        match e.name.as_str() {
            "Customer" => e.value = Some(customer.to_string()),
            "Amount" => e.value = Some(amount.to_string()),
            _ => {}
        }
    }
    snap
}

fn webapp_window() -> WindowMatch {
    WindowMatch {
        process: Some("fixture-webapp".to_string()),
        title_pattern: None,
    }
}

fn click_action(
    id: &str,
    intent: &str,
    automation_id: &str,
    x: f64,
    y: f64,
    risk: RiskClass,
) -> Action {
    Action {
        v: 1,
        id: id.to_string(),
        kind: ActionKind::Click,
        intent: Some(intent.to_string()),
        target: Some(Target {
            window: Some(webapp_window()),
            selectors: vec![Selector::AutomationId {
                value: automation_id.to_string(),
            }],
            anchor: None,
            // Replay has no perception dependency; it can only click a
            // coords_last_known baked in at "compile" time (here: bounds
            // center from the fixture snapshot). See
            // crates/replay/src/lib.rs's own module doc.
            coords_last_known: Some(Coords {
                x,
                y,
                monitor: Some("MON1".to_string()),
                dpi_scale: Some(1.0),
            }),
        }),
        params: serde_json::Map::new(),
        pace: Pace::Instant,
        risk_class: risk,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5000,
        retry: Retry::default(),
    }
}

fn type_action(id: &str, intent: &str, text: &str) -> Action {
    let mut params = serde_json::Map::new();
    params.insert("text".to_string(), json!(text));
    Action {
        v: 1,
        id: id.to_string(),
        kind: ActionKind::Type,
        intent: Some(intent.to_string()),
        target: Some(Target {
            window: Some(webapp_window()),
            selectors: vec![],
            anchor: None,
            coords_last_known: None,
        }),
        params,
        pace: Pace::Instant,
        risk_class: RiskClass::Write,
        irreversible: false,
        grounding: Grounding::Uia,
        timeout_ms: 5000,
        retry: Retry::default(),
    }
}

fn assert_matches(id: &str, intent: &str, role: &str, name: &str, regex: &str) -> Action {
    let mut params = serde_json::Map::new();
    params.insert(
        "expr".to_string(),
        json!({
            "op": "matches",
            "query": { "kind": "snapshot_element_value", "role": role, "name": name },
            "regex": regex,
        }),
    );
    Action {
        v: 1,
        id: id.to_string(),
        kind: ActionKind::Assert,
        intent: Some(intent.to_string()),
        target: Some(Target {
            window: Some(webapp_window()),
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

fn webapp_pre_gate() -> Gate {
    Gate {
        step_ref: None,
        kind: GateKind::Pre,
        expr: json!({
            "op": "equals",
            "left": { "kind": "snapshot_window_process" },
            "right": { "kind": "literal", "value": "fixture-webapp" }
        }),
        on_fail: OnFail::Halt,
    }
}

fn webapp_capabilities(app_id: &str) -> Capabilities {
    Capabilities {
        apps: vec![app_id.to_string()],
        paths: vec![],
        network: false,
        risk_ceiling: RiskClass::Write,
    }
}

/// The fixture web task: fill the Customer and Amount fields in the fixture
/// invoice webapp and click Save invoice. Grounded in
/// `contracts/fixtures/drift_renamed_button/before.json`, the pre-drift
/// snapshot of `contracts/fixtures/webapp/index.html`.
pub fn web_task() -> TaskFixture {
    let actions = vec![
        click_action(
            "w1",
            "Click the Customer field",
            "customer",
            190.0,
            136.0,
            RiskClass::Read,
        ),
        type_action("w2", "Type the customer name", WEBAPP_CUSTOMER),
        click_action(
            "w3",
            "Click the Amount field",
            "amount",
            190.0,
            186.0,
            RiskClass::Read,
        ),
        type_action("w4", "Type the invoice amount", WEBAPP_AMOUNT),
        click_action(
            "w5",
            "Click Save invoice",
            "save-btn",
            110.0,
            238.0,
            RiskClass::Write,
        ),
        assert_matches(
            "w6",
            "Check that the amount was written",
            "edit",
            "Amount",
            r"^\d+\.\d{2}$",
        ),
    ];

    let manifest = Manifest {
        v: 1,
        name: "fixture-web-invoice".to_string(),
        version: "1.0.0".to_string(),
        description: "Fills the fixture invoice webapp and saves it.".to_string(),
        step_summary: vec![
            "Click the Customer field".to_string(),
            "Type the customer name".to_string(),
            "Click the Amount field".to_string(),
            "Type the invoice amount".to_string(),
            "Click Save invoice".to_string(),
            "Check that the amount was written".to_string(),
        ],
        inputs_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        capabilities: webapp_capabilities("fixture-webapp"),
        gates: vec![
            webapp_pre_gate(),
            Gate {
                step_ref: None,
                kind: GateKind::Post,
                expr: json!({
                    "op": "matches",
                    "query": { "kind": "snapshot_element_value", "role": "edit", "name": "Amount" },
                    "regex": r"^\d+\.\d{2}$",
                }),
                on_fail: OnFail::Halt,
            },
        ],
        min_operant_version: Some("1.0.0".to_string()),
        source_run_id: None,
        dsl: DslRef {
            path: "bench/fixture-web-invoice.ts".to_string(),
            hash: "e".repeat(64),
        },
        signature: None,
    };

    TaskFixture {
        name: "web",
        workflow: CompiledWorkflow { manifest, actions },
        inputs: BTreeMap::new(),
        pre: EvalContext::new().with_snapshot(webapp_snapshot(WEBAPP_BEFORE)),
        post: EvalContext::new().with_snapshot(webapp_snapshot_with_values(
            WEBAPP_BEFORE,
            WEBAPP_CUSTOMER,
            WEBAPP_AMOUNT,
        )),
    }
}

/// The drift fixture, post-repair: the workflow's selectors already target
/// the renamed button (`store-btn` / "Store invoice"), replayed against
/// `after.json`, the drifted state
/// (`contracts/fixtures/drift_renamed_button/README.md`: "Expected repair: a
/// patch replacing the button's selectors... with (store-btn / name 'Store
/// invoice')"). This exercises replay against an already-repaired action
/// list; L9B does not implement the repair pass itself (`crates/compiler`
/// notes "L8B adds drift repair").  The precondition gate (window process)
/// still holds in `after.json`, per that same README, which is what makes
/// the original failure drift-eligible rather than a wrong-state halt.
pub fn drift_repaired_task() -> TaskFixture {
    let actions = vec![
        click_action(
            "d1",
            "Click the Customer field",
            "customer",
            190.0,
            136.0,
            RiskClass::Read,
        ),
        type_action("d2", "Type the customer name", WEBAPP_CUSTOMER),
        click_action(
            "d3",
            "Click Store invoice (post-repair selector)",
            "store-btn",
            110.0,
            238.0,
            RiskClass::Write,
        ),
        assert_matches(
            "d4",
            "Check that the customer name was written",
            "edit",
            "Customer",
            "^Acme Co$",
        ),
    ];

    let manifest = Manifest {
        v: 1,
        name: "fixture-web-invoice-drift-repaired".to_string(),
        version: "1.0.1".to_string(),
        description: "Invoice workflow repaired to target the renamed Store invoice button, \
            replayed against the drifted fixture."
            .to_string(),
        step_summary: vec![
            "Click the Customer field".to_string(),
            "Type the customer name".to_string(),
            "Click Store invoice".to_string(),
            "Check that the customer name was written".to_string(),
        ],
        inputs_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        capabilities: webapp_capabilities("fixture-webapp"),
        gates: vec![
            webapp_pre_gate(),
            Gate {
                step_ref: None,
                kind: GateKind::Post,
                expr: json!({
                    "op": "matches",
                    "query": { "kind": "snapshot_element_value", "role": "edit", "name": "Customer" },
                    "regex": "^Acme Co$",
                }),
                on_fail: OnFail::Halt,
            },
        ],
        min_operant_version: Some("1.0.0".to_string()),
        source_run_id: None,
        dsl: DslRef {
            path: "bench/fixture-web-invoice-drift-repaired.ts".to_string(),
            hash: "f".repeat(64),
        },
        signature: None,
    };

    TaskFixture {
        name: "drift_repaired",
        workflow: CompiledWorkflow { manifest, actions },
        inputs: BTreeMap::new(),
        pre: EvalContext::new().with_snapshot(webapp_snapshot(WEBAPP_AFTER)),
        post: EvalContext::new().with_snapshot(webapp_snapshot_with_values(
            WEBAPP_AFTER,
            WEBAPP_CUSTOMER,
            WEBAPP_AMOUNT,
        )),
    }
}

// ---- measurement -------------------------------------------------------------

/// One repetition's raw measurement.
struct RepMeasurement {
    ok: bool,
    wall_ms: f64,
}

/// Every action except the trailing `assert`, which `Replayer::replay` never
/// dispatches (it is evaluated as the postcondition gate instead). Mirrors
/// that skip rule exactly so the divisor used for per-step latency always
/// matches what actually ran.
fn dispatchable_action_count(workflow: &CompiledWorkflow) -> usize {
    workflow
        .actions
        .iter()
        .filter(|a| a.kind != ActionKind::Assert)
        .count()
        .max(1)
}

/// Run one repetition of `fixture` through a fresh `Replayer::with_mock()`
/// (a completely fresh MockSynthesizer per repetition, never reused, mirrors
/// `e2e/golden-path/tests/golden_path.rs`'s own reasoning for starting
/// replay "from nothing"), timing the whole call.
fn measure_replay_repetition(fixture: &TaskFixture) -> RepMeasurement {
    let replayer = Replayer::with_mock();
    let start = Instant::now();
    let result = replayer.replay_compiled(
        &fixture.workflow,
        &fixture.inputs,
        &fixture.pre,
        &fixture.post,
    );
    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    RepMeasurement {
        ok: result.is_ok(),
        wall_ms,
    }
}

fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let n = sorted_ms.len();
    let rank = (p / 100.0 * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    sorted_ms[idx]
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(|a, b| a.partial_cmp(b).expect("latency samples are never NaN"));
    v
}

/// Both bench rows (`replay` and `reinfer_mock`) produced from one task run.
pub struct TaskRun {
    pub replay: BenchResult,
    pub reinfer_mock: BenchResult,
}

/// Run `fixture` for [`REPETITIONS`] repetitions and produce both the
/// `replay` and `reinfer_mock` bench rows for it.
pub fn run_task(suite: &str, fixture: &TaskFixture) -> TaskRun {
    let steps = dispatchable_action_count(&fixture.workflow);

    let reps: Vec<RepMeasurement> = (0..REPETITIONS)
        .map(|_| measure_replay_repetition(fixture))
        .collect();
    let successes = reps.iter().filter(|r| r.ok).count() as i32;

    let replay_step_samples = sorted(reps.iter().map(|r| r.wall_ms / steps as f64).collect());
    let replay_total_wall_ms: f64 = reps.iter().map(|r| r.wall_ms).sum();

    let replay = BenchResult {
        v: 1,
        suite: suite.to_string(),
        task: fixture.name.to_string(),
        mode: BenchMode::Replay,
        repetitions: REPETITIONS,
        successes,
        p50_step_ms: percentile(&replay_step_samples, 50.0),
        p95_step_ms: percentile(&replay_step_samples, 95.0),
        total_wall_ms: replay_total_wall_ms,
        model_calls: 0,
        tokens: 0,
        notes: Some(format!(
            "{steps} dispatchable step(s) per repetition; zero model calls by construction"
        )),
        ts: None,
    };

    // reinfer_mock derives its cost from replay's own recorded latencies
    // plus a documented simulated per-step overhead: same trajectory and
    // gates as replay (so the same successes), different cost accounting.
    // It never independently re-plans or calls a real backend; see the
    // honesty note render_benchmarks_md emits.
    let overhead_per_rep_ms = REINFER_STEP_OVERHEAD_MS * steps as f64;
    let reinfer_step_samples = sorted(
        reps.iter()
            .map(|r| (r.wall_ms + overhead_per_rep_ms) / steps as f64)
            .collect(),
    );
    let reinfer_total_wall_ms: f64 = reps.iter().map(|r| r.wall_ms + overhead_per_rep_ms).sum();
    let reinfer_model_calls = steps as i32 * REPETITIONS;

    let reinfer_mock = BenchResult {
        v: 1,
        suite: suite.to_string(),
        task: fixture.name.to_string(),
        mode: BenchMode::ReinferMock,
        repetitions: REPETITIONS,
        successes,
        p50_step_ms: percentile(&reinfer_step_samples, 50.0),
        p95_step_ms: percentile(&reinfer_step_samples, 95.0),
        total_wall_ms: reinfer_total_wall_ms,
        model_calls: reinfer_model_calls,
        tokens: reinfer_model_calls * REINFER_TOKENS_PER_CALL,
        notes: Some(format!(
            "recorded replay latency plus {REINFER_STEP_OVERHEAD_MS:.1}ms simulated re-inference \
             overhead per step; no real backend called"
        )),
        ts: None,
    };

    TaskRun {
        replay,
        reinfer_mock,
    }
}

/// The three fixture tasks `docs/specs/bench.md` names: notepad, web, and
/// the drift fixture post-repair.
pub fn fixture_tasks() -> Vec<TaskFixture> {
    vec![notepad_task(), web_task(), drift_repaired_task()]
}

/// Run the full suite (every fixture task, both modes, 5 repetitions each)
/// and return the flat row list `render_benchmarks_md` consumes.
pub fn run_suite() -> Vec<BenchResult> {
    let mut out = Vec::new();
    for fixture in &fixture_tasks() {
        let run = run_task("fixture", fixture);
        out.push(run.replay);
        out.push(run.reinfer_mock);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::GateResult;

    #[test]
    fn notepad_task_replays_clean() {
        let fixture = notepad_task();
        let replayer = Replayer::with_mock();
        let report = replayer
            .replay_compiled(
                &fixture.workflow,
                &fixture.inputs,
                &fixture.pre,
                &fixture.post,
            )
            .expect("notepad fixture replays");
        assert_eq!(report.post, vec![GateResult::Pass]);
    }

    #[test]
    fn web_task_replays_clean() {
        let fixture = web_task();
        let replayer = Replayer::with_mock();
        let report = replayer
            .replay_compiled(
                &fixture.workflow,
                &fixture.inputs,
                &fixture.pre,
                &fixture.post,
            )
            .expect("web fixture replays");
        assert_eq!(report.pre, vec![GateResult::Pass]);
        assert_eq!(report.post, vec![GateResult::Pass]);
        assert_eq!(dispatchable_action_count(&fixture.workflow), 5);
    }

    #[test]
    fn drift_repaired_task_replays_clean_against_the_drifted_snapshot() {
        let fixture = drift_repaired_task();
        let replayer = Replayer::with_mock();
        let report = replayer
            .replay_compiled(
                &fixture.workflow,
                &fixture.inputs,
                &fixture.pre,
                &fixture.post,
            )
            .expect("repaired workflow replays against the drifted fixture");
        assert_eq!(report.pre, vec![GateResult::Pass]);
        assert_eq!(report.post, vec![GateResult::Pass]);
    }

    #[test]
    fn broken_postcondition_fails_every_repetition() {
        let fixture = notepad_task_with_broken_postcondition();
        let replayer = Replayer::with_mock();
        let err = replayer
            .replay_compiled(
                &fixture.workflow,
                &fixture.inputs,
                &fixture.pre,
                &fixture.post,
            )
            .expect_err("empty editor value must fail the postcondition");
        assert!(matches!(
            err,
            operant_replay::ReplayError::Postcondition { .. }
        ));
    }

    #[test]
    fn run_task_reports_five_of_five_successes_on_an_unchanged_fixture() {
        let fixture = notepad_task();
        let run = run_task("fixture", &fixture);
        assert_eq!(run.replay.repetitions, REPETITIONS);
        assert_eq!(run.replay.successes, REPETITIONS);
        assert_eq!(
            run.replay.model_calls, 0,
            "replay makes zero model calls by construction"
        );
        assert_eq!(run.replay.tokens, 0);
        assert_eq!(run.reinfer_mock.successes, REPETITIONS);
        assert!(
            run.reinfer_mock.model_calls > 0,
            "reinfer_mock re-plans every step"
        );
        assert!(run.reinfer_mock.tokens > 0);
        // reinfer_mock's simulated overhead means it is never cheaper than
        // the recorded replay latency it derives from.
        assert!(run.reinfer_mock.p50_step_ms >= run.replay.p50_step_ms);
    }

    #[test]
    fn run_task_reports_zero_of_five_successes_on_the_broken_fixture() {
        let fixture = notepad_task_with_broken_postcondition();
        let run = run_task("fixture", &fixture);
        assert_eq!(run.replay.successes, 0);
        assert_eq!(run.reinfer_mock.successes, 0);
    }

    #[test]
    fn run_suite_produces_six_rows() {
        let results = run_suite();
        assert_eq!(results.len(), 6, "3 tasks x 2 modes");
        let tasks: std::collections::BTreeSet<_> =
            results.iter().map(|r| r.task.as_str()).collect();
        assert_eq!(
            tasks,
            std::collections::BTreeSet::from(["notepad", "web", "drift_repaired"])
        );
    }
}
