//! `operant record-ipc [--out <dir>] [--fixtures <dir>]` (dev-only, behind the
//! `dev-ipc-record` cargo feature): the IPC fixture recorder.
//!
//! It drives a REAL CLI session against the real core Bus (explore -> compile ->
//! replay -> undo) and captures three things, framed EXACTLY per
//! `contracts/ipc.md`:
//!
//! 1. the capability handshake response (`get_capabilities`),
//! 2. the command/response pairs the shell would send, and
//! 3. the real bus event stream the session produces.
//!
//! It writes `contracts/fixtures/ipc/handshake.json` and
//! `contracts/fixtures/ipc/session-explore-compile-replay-undo.jsonl` so Phase 2
//! lanes build and test against a recorded session without a live core.
//!
//! What is real here: the explore run is the real `operant_orchestrator`
//! `ExploreLoop` publishing real, typed `run.*` events to the real
//! `operant_core::Bus` (the SAME event structs a live core emits; the planner
//! and perceiver are mock, which is exactly how the default `operant explore`
//! runs headless). Compile is the real `operant_compiler::compile`. Replay is
//! the real `operant_replay::Replayer`, wrapped in the synthetic `run.*`
//! envelope the `start_replay` command is contracted to publish
//! (`contracts/ipc.md` section 5b, `docs/specs/ipc-bridge.md` section 3b). Undo
//! is the real `operant_recorder` undo journal, published by the recorder's own
//! `publish_undo_preview`.
//!
//! This binary is NEVER in a release build. It is opt-in behind `dev-ipc-record`
//! and, like `dev-agent-bridge`, is a development harness only.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use operant_action::{Executor, MockSynthesizer, NoopSleeper};
use operant_compiler::{compile, Trajectory};
use operant_core::bus::events::{
    RunCompleted, RunMode as BusRunMode, RunOutcome as BusRunOutcome, RunStarted, RunStepExecuted,
    RunStepGated, StepOutcome,
};
use operant_core::{Bus, Perceiver};
use operant_gates::EvalContext;
use operant_ir::bus::Envelope;
use operant_ir::{ActionKind, GateKind, GateResult};
use operant_orchestrator::backends::{BackendEvent, MockPlannerBackend, ModelBackend};
use operant_orchestrator::explore::{ExploreLoop, NoControl};
use operant_perception_uia::FixturePerceiver;
use operant_recorder::undo::PendingWrite;
use operant_recorder::{Recorder, RunMode, RunStatus};
use operant_replay::{CompiledWorkflow, Replayer};

/// The IPC protocol version this recorder frames to. Must match
/// `contracts/ipc.md` (the `pv` field), distinct from the bus envelope `v`.
const PROTOCOL_VERSION: u32 = 1;
const WINDOW_PROCESS: &str = "notepad.exe";
/// Exactly `cli/src/snapshot.rs`'s `DEFAULT_INVOICE_TEXT`, so the compiled
/// workflow's postcondition matches the bundled snapshot and replay passes.
const INVOICE_TEXT: &str = "Invoice 2026-07-11 total $142.50";
const GOAL: &str = "Write an invoice note in Notepad and save it";

/// Every top-level bus family the pump forwards (`contracts/ipc.md` section 6).
/// Disjoint top-level prefixes, so no envelope is delivered to two of these.
const FAMILIES: &[&str] = &[
    "run.*",
    "gate.*",
    "approval.*",
    "perception.*",
    "sidecar.*",
    "vram.*",
    "workflow.*",
    "trigger.fired",
    "schedule.*",
    "killswitch.*",
    "undo.*",
    "doctor.*",
    "metrics.*",
    "suggestion.*",
    "config.changed",
    "voice.*",
];

pub fn run(args: &[String]) -> Result<()> {
    let Some(opts) = Opts::parse(args)? else {
        return Ok(());
    };

    std::fs::create_dir_all(&opts.work)
        .with_context(|| format!("creating work directory {}", opts.work.display()))?;
    std::fs::create_dir_all(&opts.fixtures)
        .with_context(|| format!("creating fixtures directory {}", opts.fixtures.display()))?;

    let bus = Bus::new();
    // Subscribe to every family BEFORE anything publishes, so the crossbeam
    // channels buffer the whole stream; we drain them per phase below.
    let subs: Vec<_> = FAMILIES.iter().map(|p| bus.subscribe(p)).collect();

    let recorder =
        Recorder::open(opts.work.join("recorder.sqlite3")).context("opening the run recorder")?;

    let mut frames: Vec<Value> = Vec::new();

    // ---- ready (core -> shell, unsolicited first frame) ----
    frames.push(json!({ "t": "ready", "pv": PROTOCOL_VERSION }));

    // ---- get_capabilities (the handshake) ----
    // The recorder reports its OWN build's capabilities, computed from the same
    // cfg flags the rest of the CLI uses. The committed fixture is a default
    // (mock) build, so this is the BLOCKING case (real_uia/real_input false):
    // the exact state that must force the shell's blocking screen.
    let caps = json!({
        "real_uia": cfg!(feature = "real-uia"),
        "real_input": cfg!(feature = "real-input"),
        "real_vision": false,
        "mock_planner_only": true,
        "transport_kind": "stdio",
        "version": "1.0.0",
        "git_sha": "unknown"
    });
    let handshake_req = req("cmd-1", "get_capabilities", json!({}));
    let handshake_res = res_ok("cmd-1", caps.clone());
    frames.push(handshake_req.clone());
    frames.push(handshake_res.clone());

    // ---- start_explore: the real ExploreLoop, real typed run.* events ----
    let rt = tokio::runtime::Runtime::new().context("starting the tokio runtime")?;
    let perceiver: Box<dyn Perceiver> = Box::new(fixture_perceiver(WINDOW_PROCESS)?);
    let planner: Box<dyn ModelBackend> = Box::new(scripted_mock_planner());
    let executor = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));
    let explore = ExploreLoop::new(perceiver, planner, executor, WINDOW_PROCESS);
    let mut control = NoControl;
    let summary = rt
        .block_on(explore.run(&bus, &recorder, GOAL, &mut control))
        .map_err(|e| anyhow::anyhow!("explore loop failed: {e}"))?;
    let explore_run_id = summary.run_id.clone();

    frames.push(req(
        "cmd-2",
        "start_explore",
        json!({ "goal": GOAL, "window_process": WINDOW_PROCESS }),
    ));
    frames.push(res_ok("cmd-2", json!({ "run_id": explore_run_id })));
    push_events(&mut frames, drain(&subs)?);

    // ---- compile_run: the real compiler over the recorded trajectory ----
    let run_row = recorder
        .get_run(&explore_run_id)
        .context("reading the run row")?
        .context("run row missing after the explore run")?;
    let steps = recorder
        .list_steps(&explore_run_id)
        .context("listing recorded steps")?;
    let trajectory_value = export_trajectory(&run_row, &steps);
    let traj: Trajectory = serde_json::from_value(trajectory_value)
        .context("re-parsing the exported trajectory as a compiler Trajectory")?;
    let compilation =
        compile(&traj).map_err(|e| anyhow::anyhow!("compiling the trajectory: {e}"))?;
    let wf_name = compilation.workflow.manifest.name.clone();
    let wf_version = compilation.workflow.manifest.version.clone();
    let wf_steps = compilation.workflow.actions.len();
    // Write the compiled artifacts into the throwaway work dir (not the fixtures).
    let compiled_dir = opts.work.join("compiled");
    std::fs::create_dir_all(&compiled_dir).ok();
    std::fs::write(
        compiled_dir.join("compiled.json"),
        serde_json::to_string_pretty(&compilation.workflow)?,
    )
    .ok();

    frames.push(req("cmd-3", "compile_run", json!({ "run_id": explore_run_id })));
    frames.push(res_ok(
        "cmd-3",
        json!({ "name": wf_name, "version": wf_version, "steps": wf_steps }),
    ));
    // compile_run echoes workflow.compiled (relative paths keep the fixture portable).
    bus.publish(
        "workflow.compiled",
        json!({
            "name": wf_name,
            "version": wf_version,
            "manifest_path": "compiled/manifest.json",
            "dsl_path": "compiled/workflow.ts",
            "source_run_id": explore_run_id
        }),
    );
    push_events(&mut frames, drain(&subs)?);

    // ---- start_replay: the real Replayer, wrapped in synthetic run.* ----
    let wf: CompiledWorkflow = serde_json::from_value(serde_json::to_value(&compilation.workflow)?)
        .context("re-parsing the compiled workflow as a replay CompiledWorkflow")?;
    let ctx = EvalContext::new().with_snapshot(crate::snapshot::bundled_notepad_snapshot());
    let inputs: BTreeMap<String, String> = BTreeMap::new();
    let report = Replayer::with_mock()
        .replay_compiled(&wf, &inputs, &ctx, &ctx)
        .map_err(|e| anyhow::anyhow!("replaying the compiled workflow: {e}"))?;

    // A real replay run row, then the synthetic run.* the start_replay command
    // is contracted to publish around a Replayer (which itself publishes nothing).
    let replay_run_id = recorder
        .start_run(GOAL, RunMode::Replay, None)
        .context("starting the replay run row")?;
    bus.publish_event(&RunStarted {
        run_id: replay_run_id.clone(),
        goal: GOAL.to_string(),
        mode: BusRunMode::Replay,
        workflow_name: Some(wf_name.clone()),
    })?;
    let mut replay_steps = 0u32;
    for action in &wf.actions {
        if action.kind == ActionKind::Assert {
            continue; // never dispatched; surfaced as the postcondition gate
        }
        bus.publish_event(&RunStepGated {
            run_id: replay_run_id.clone(),
            step_id: action.id.clone(),
            gate_kind: GateKind::Pre,
            result: GateResult::Pass,
            expr: None,
        })?;
        bus.publish_event(&RunStepExecuted {
            run_id: replay_run_id.clone(),
            step_id: action.id.clone(),
            outcome: StepOutcome::Ok,
            ms: 0,
            grounding: action.grounding,
        })?;
        replay_steps += 1;
    }
    bus.publish_event(&RunCompleted {
        run_id: replay_run_id.clone(),
        outcome: BusRunOutcome::Ok,
        steps: replay_steps,
        wall_ms: 0,
    })?;
    recorder
        .end_run(&replay_run_id, RunStatus::Completed)
        .context("closing the replay run row")?;

    frames.push(req(
        "cmd-4",
        "start_replay",
        json!({ "path": "compiled/compiled.json" }),
    ));
    frames.push(res_ok(
        "cmd-4",
        json!({
            "run_id": replay_run_id,
            "steps_executed": report.steps_executed,
            "pre": gate_results(&report.pre),
            "post": gate_results(&report.post)
        }),
    ));
    push_events(&mut frames, drain(&subs)?);

    // ---- preview_undo + undo_run: the real recorder undo journal ----
    // The headless mock synthesizer performs no real OS writes, so nothing was
    // journaled by the run itself. Seed the journal through the recorder's REAL
    // `journal_ahead` API (a relative path that never exists on disk, so the
    // later undo is a guarded no-op) so the REAL `publish_undo_preview` emits a
    // populated `undo.previewed` exercising the F1b items[] wire shape and an
    // irreversible entry. This models a run that created a file and sent an email.
    recorder
        .journal_ahead(
            &explore_run_id,
            &PendingWrite::CreateFile {
                path: PathBuf::from("undo_demo.txt"),
            },
        )
        .context("seeding a CreateFile undo entry")?;
    recorder
        .journal_ahead(
            &explore_run_id,
            &PendingWrite::Irreversible {
                description: "sent the invoice email to boss@example.com".to_string(),
            },
        )
        .context("seeding an Irreversible undo entry")?;

    frames.push(req("cmd-5", "preview_undo", json!({ "run_id": explore_run_id })));
    frames.push(res_ok("cmd-5", json!({ "ok": true })));
    recorder
        .publish_undo_preview(&bus, &explore_run_id)
        .context("publishing the undo preview")?;
    push_events(&mut frames, drain(&subs)?);

    frames.push(req("cmd-6", "undo_run", json!({ "run_id": explore_run_id })));
    let narration = recorder
        .undo_run(&explore_run_id)
        .context("applying the undo")?;
    frames.push(res_ok("cmd-6", json!({ "restored": narration.len() })));
    bus.publish(
        "undo.applied",
        json!({ "run_id": explore_run_id, "restored": narration.len(), "narration": narration }),
    );
    push_events(&mut frames, drain(&subs)?);

    // ---- normalize volatile ids/timings so the committed fixture is stable ----
    normalize(&mut frames);

    // ---- write the fixtures ----
    let mut jsonl = String::new();
    for f in &frames {
        jsonl.push_str(&serde_json::to_string(f)?);
        jsonl.push('\n');
    }
    let session_path = opts.fixtures.join("session-explore-compile-replay-undo.jsonl");
    std::fs::write(&session_path, &jsonl)
        .with_context(|| format!("writing {}", session_path.display()))?;

    // The handshake extract: ready + the get_capabilities exchange, normalized.
    let mut handshake_extract = vec![
        json!({ "t": "ready", "pv": PROTOCOL_VERSION }),
        handshake_req,
        handshake_res,
    ];
    normalize(&mut handshake_extract);
    let handshake_doc = json!({
        "note": "The shell to core capability handshake, framed per contracts/ipc.md section 3. This is a real capture from a default (mock) recorder build, so real_uia and real_input are false: the BLOCKING case that must force the shell's blocking screen. A real-capable core reports the same shape with the booleans true.",
        "ready": handshake_extract[0],
        "request": handshake_extract[1],
        "response": handshake_extract[2]
    });
    let handshake_path = opts.fixtures.join("handshake.json");
    std::fs::write(
        &handshake_path,
        serde_json::to_string_pretty(&handshake_doc)? + "\n",
    )
    .with_context(|| format!("writing {}", handshake_path.display()))?;

    println!("record-ipc: captured {} frames", frames.len());
    println!("  handshake -> {}", handshake_path.display());
    println!("  session   -> {}", session_path.display());
    Ok(())
}

// --------------------------------------------------------------------------
// Frame builders (contracts/ipc.md section 2)
// --------------------------------------------------------------------------

fn req(id: &str, cmd: &str, args: Value) -> Value {
    json!({ "t": "req", "pv": PROTOCOL_VERSION, "id": id, "cmd": cmd, "args": args })
}

fn res_ok(id: &str, result: Value) -> Value {
    json!({ "t": "res", "pv": PROTOCOL_VERSION, "id": id, "ok": true, "result": result })
}

/// Wrap a captured bus envelope as an `evt` frame. `thumb` is null throughout:
/// this is a headless/mock recorder with no pixels, so no thumbnail exists
/// (`contracts/ipc.md` section 7). The field is present to document its shape.
fn evt_frame(env: &Envelope) -> Result<Value> {
    Ok(json!({
        "t": "evt",
        "pv": PROTOCOL_VERSION,
        "env": serde_json::to_value(env)?,
        "thumb": Value::Null
    }))
}

fn push_events(frames: &mut Vec<Value>, envs: Vec<Envelope>) {
    for env in &envs {
        if let Ok(frame) = evt_frame(env) {
            frames.push(frame);
        }
    }
}

/// Drain every subscription and return the envelopes globally ordered by `seq`.
/// Families are disjoint top-level prefixes, so no envelope is double-counted.
fn drain(subs: &[operant_core::bus::Subscription]) -> Result<Vec<Envelope>> {
    let mut out: Vec<Envelope> = Vec::new();
    for sub in subs {
        out.extend(sub.rx.try_iter());
    }
    out.sort_by_key(|e| e.seq);
    Ok(out)
}

fn gate_results(results: &[GateResult]) -> Value {
    Value::Array(
        results
            .iter()
            .map(|r| match r {
                GateResult::Pass => json!("pass"),
                GateResult::Fail => json!("fail"),
            })
            .collect(),
    )
}

// --------------------------------------------------------------------------
// Determinism normalization
// --------------------------------------------------------------------------

/// Replace recorder-generated ids (`run_<hex>_<hex>`, `step_<hex>_<hex>`, per
/// `crates/recorder/src/ids.rs`) with stable tokens and zero volatile timings
/// (`ms`, `wall_ms`), so the committed fixture is byte-stable and reviewable
/// across regenerations. Everything else is the raw capture. First-seen order
/// is `seq` order, which is deterministic for this mock session.
fn normalize(frames: &mut [Value]) {
    let mut idmap: BTreeMap<String, String> = BTreeMap::new();
    let mut run_n = 0u32;
    let mut step_n = 0u32;
    for f in frames.iter_mut() {
        normalize_value(f, &mut idmap, &mut run_n, &mut step_n);
    }
}

fn normalize_value(
    v: &mut Value,
    idmap: &mut BTreeMap<String, String>,
    run_n: &mut u32,
    step_n: &mut u32,
) {
    match v {
        Value::String(s) => {
            if let Some(tok) = token_for(s, idmap, run_n, step_n) {
                *s = tok;
            }
        }
        Value::Object(m) => {
            for (k, val) in m.iter_mut() {
                if (k == "ms" || k == "wall_ms") && val.is_number() {
                    *val = json!(0);
                } else {
                    normalize_value(val, idmap, run_n, step_n);
                }
            }
        }
        Value::Array(a) => {
            for val in a.iter_mut() {
                normalize_value(val, idmap, run_n, step_n);
            }
        }
        _ => {}
    }
}

/// If `s` is a recorder-generated id, return its stable token (assigning a new
/// one on first sight). Otherwise `None`.
fn token_for(
    s: &str,
    idmap: &mut BTreeMap<String, String>,
    run_n: &mut u32,
    step_n: &mut u32,
) -> Option<String> {
    if let Some(existing) = idmap.get(s) {
        return Some(existing.clone());
    }
    let prefix = if s.starts_with("run_") {
        "run"
    } else if s.starts_with("step_") {
        "step"
    } else {
        return None;
    };
    let rest = &s[prefix.len() + 1..];
    let parts: Vec<&str> = rest.split('_').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    if !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit())) {
        return None;
    }
    let token = if prefix == "run" {
        let t = format!("run_{run_n}");
        *run_n += 1;
        t
    } else {
        let t = format!("step_{step_n}");
        *step_n += 1;
        t
    };
    idmap.insert(s.to_string(), token.clone());
    Some(token)
}

// --------------------------------------------------------------------------
// Mock explore assembly (mirrors cli/src/commands/explore.rs's default path)
// --------------------------------------------------------------------------

/// The bundled Notepad snapshot as a single-frame fixture perceiver, its window
/// process rewritten so the loop's perception and safety guard resolve against
/// `WINDOW_PROCESS`. Identical to `explore.rs`'s default perceiver.
fn fixture_perceiver(window_process: &str) -> Result<FixturePerceiver> {
    const RAW: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
    let mut snap: operant_ir::Snapshot =
        serde_json::from_str(RAW).context("parsing the bundled fixture snapshot")?;
    snap.window.process = window_process.to_string();
    Ok(FixturePerceiver::single(snap))
}

/// Click the editor, type the invoice note, save, then `done`: the same scripted
/// mock planner `explore.rs` uses by default, and the golden-path narrative.
/// The invoice text matches `snapshot.rs`, so the compiled postcondition passes
/// against the bundled snapshot at replay.
fn scripted_mock_planner() -> MockPlannerBackend {
    let click = json!({
        "id": "s1",
        "kind": "click",
        "intent": "Click the text editor",
        "target": {
            "window": { "process": WINDOW_PROCESS },
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
    });
    let type_note = json!({
        "id": "s2",
        "kind": "type",
        "intent": "Type the invoice note",
        "target": { "window": { "process": WINDOW_PROCESS } },
        "params": { "text": INVOICE_TEXT },
        "risk_class": "write",
        "grounding": "uia"
    });
    let save = json!({
        "id": "s3",
        "kind": "key",
        "intent": "Save the file",
        "target": { "window": { "process": WINDOW_PROCESS } },
        "params": { "combo": "ctrl+s" },
        "risk_class": "write",
        "grounding": "uia"
    });
    MockPlannerBackend::new(
        "mock_planner",
        vec![
            BackendEvent::ToolCall {
                id: "1".to_string(),
                name: "propose_action".to_string(),
                arguments: click,
            },
            BackendEvent::ToolCall {
                id: "2".to_string(),
                name: "propose_action".to_string(),
                arguments: type_note,
            },
            BackendEvent::ToolCall {
                id: "3".to_string(),
                name: "propose_action".to_string(),
                arguments: save,
            },
            BackendEvent::ToolCall {
                id: "4".to_string(),
                name: "done".to_string(),
                arguments: json!({}),
            },
        ],
    )
}

/// Turn the recorded run and its steps into the compiler's trajectory JSON,
/// identical to `explore.rs`'s `export_trajectory`.
fn export_trajectory(
    run: &operant_recorder::RunRecord,
    steps: &[operant_recorder::StepRecord],
) -> Value {
    let steps_json: Vec<Value> = steps
        .iter()
        .map(|s| {
            let mut step = json!({
                "seq": s.seq,
                "action": s.action,
                "grounding": s.grounding,
                "outcome": s.outcome,
                "ms": s.ms,
                "outcome_bearing": s.outcome_bearing,
            });
            let obj = step.as_object_mut().expect("json object");
            if let Some(b) = &s.snapshot_digest_before {
                obj.insert("snapshot_digest_before".to_string(), json!(b));
            }
            if let Some(a) = &s.snapshot_digest_after {
                obj.insert("snapshot_digest_after".to_string(), json!(a));
            }
            if let Some(n) = &s.note {
                obj.insert("note".to_string(), json!(n));
            }
            if let Some(hc) = &s.human_correction {
                obj.insert("human_correction".to_string(), hc.clone());
            }
            step
        })
        .collect();

    json!({
        "v": 1,
        "description": format!("Recorded by `operant record-ipc` from run {}", run.id),
        "run": {
            "id": run.id,
            "goal": run.goal,
            "mode": run.mode,
            "status": run.status,
            "started": run.started,
            "ended": run.ended,
            "model_config": run.model_config,
        },
        "steps": steps_json,
    })
}

// --------------------------------------------------------------------------
// Args
// --------------------------------------------------------------------------

struct Opts {
    work: PathBuf,
    fixtures: PathBuf,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Option<Self>> {
        let mut work = None;
        let mut fixtures = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => {
                    print_help();
                    return Ok(None);
                }
                "--out" => {
                    i += 1;
                    work = Some(PathBuf::from(
                        args.get(i).cloned().context("--out needs a value")?,
                    ));
                }
                "--fixtures" => {
                    i += 1;
                    fixtures = Some(PathBuf::from(
                        args.get(i).cloned().context("--fixtures needs a value")?,
                    ));
                }
                other => anyhow::bail!("operant record-ipc: unexpected argument `{other}`"),
            }
            i += 1;
        }
        Ok(Some(Self {
            work: work.unwrap_or_else(|| PathBuf::from("out").join("record-ipc")),
            fixtures: fixtures
                .unwrap_or_else(|| PathBuf::from("contracts").join("fixtures").join("ipc")),
        }))
    }
}

fn print_help() {
    println!("operant record-ipc [--out <work-dir>] [--fixtures <dir>]");
    println!();
    println!("Record a real explore -> compile -> replay -> undo session against the core");
    println!("Bus and write the IPC fixtures (handshake.json + session .jsonl) framed per");
    println!("contracts/ipc.md. Dev-only; built behind the `dev-ipc-record` feature.");
    println!();
    println!("  --out       throwaway work dir for the recorder db + compiled artifacts");
    println!("              (default ./out/record-ipc)");
    println!("  --fixtures  where the fixtures are written (default contracts/fixtures/ipc)");
}
