//! `operant explore --goal <text> --window-process <exe> [--out <dir>]`:
//! the model-driven teach verb. Runs operant-orchestrator's [`ExploreLoop`]
//! (perceive -> plan -> gate -> execute -> record) to completion, then exports
//! a compiler-ready `trajectory.json` that `operant compile` accepts.
//!
//! Backends are chosen by cfg, mirroring `commands/run.rs`:
//! - perceiver: `UiaPerceiver` under `real-uia`, else a `FixturePerceiver`
//!   over the bundled Notepad snapshot fixture (the default/mock path).
//! - synthesizer: `WindowsSynthesizer` under `real-input`, else
//!   `MockSynthesizer` (headless, deterministic).
//! - planner: the opt-in `AgentBridgeBackend` under `dev-agent-bridge` (an
//!   operator answers each turn through a directory of JSON files), else a
//!   scripted `MockPlannerBackend` running a fixed 2-3 step Notepad task.
//!
//! Unlike `run`/`dry-run`, this verb is async (the loop is) and legitimately
//! uses a planner backend. It stays offline in every build this workspace's
//! gates gate on: the default planner is the scripted mock, and the agent
//! bridge is a filesystem rendezvous, not a network call. E4's guard in
//! `run.rs` still forbids enabling exactly one of `real-uia`/`real-input`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use operant_action::{Executor, Synthesizer};
use operant_core::{Bus, Perceiver};
use operant_orchestrator::backends::ModelBackend;
use operant_orchestrator::explore::{ExploreLoop, NoControl, RunSummary};
use operant_recorder::{Recorder, RunRecord, StepRecord};
use serde_json::json;

#[cfg(not(feature = "dev-agent-bridge"))]
use operant_orchestrator::backends::{BackendEvent, MockPlannerBackend};
#[cfg(not(feature = "real-uia"))]
use operant_perception_uia::FixturePerceiver;

pub fn run(args: &[String]) -> Result<()> {
    let Some(opts) = Opts::parse(args)? else {
        return Ok(());
    };

    std::fs::create_dir_all(&opts.out)
        .with_context(|| format!("creating output directory {}", opts.out.display()))?;

    let bus = Bus::new();
    // Recorder lives under the out dir (a real SQLite file, WAL alongside), so
    // the run's rows survive for the trajectory export below and for post-hoc
    // inspection.
    let recorder =
        Recorder::open(opts.out.join("recorder.sqlite3")).context("opening the run recorder")?;

    let rt = tokio::runtime::Runtime::new().context("starting the tokio runtime")?;

    // ---- planner (cfg) ----
    #[cfg(feature = "dev-agent-bridge")]
    let planner: Box<dyn ModelBackend> = {
        use operant_orchestrator::backends::AgentBridgeBackend;
        let bridge = AgentBridgeBackend::from_env().map_err(|e| anyhow::anyhow!("{e}"))?;
        println!(
            "agent-bridge planner: rendezvous dir {}",
            bridge.dir().display()
        );
        Box::new(bridge)
    };
    #[cfg(not(feature = "dev-agent-bridge"))]
    let planner: Box<dyn ModelBackend> = Box::new(scripted_mock_planner());

    // ---- perceiver (cfg) ----
    #[cfg(feature = "real-uia")]
    let perceiver: Box<dyn Perceiver> = Box::new(operant_perception_uia::UiaPerceiver::new());
    #[cfg(not(feature = "real-uia"))]
    let perceiver: Box<dyn Perceiver> = Box::new(fixture_perceiver(&opts.window_process)?);

    // ---- synthesizer (cfg) + run ----
    // Two branches because `Executor<S>` is generic over the synthesizer type;
    // `run_loop` is generic so the run + export logic is written once.
    #[cfg(feature = "real-input")]
    let summary = {
        use operant_action::WindowsSynthesizer;
        let executor = Executor::new(WindowsSynthesizer::new());
        run_loop(&rt, perceiver, planner, executor, &opts, &bus, &recorder)?
    };
    #[cfg(not(feature = "real-input"))]
    let summary = {
        use operant_action::{MockSynthesizer, NoopSleeper};
        // NoopSleeper: no real waiting on human-paced steps or retry backoff,
        // so the headless mock run is instant and deterministic.
        let executor = Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));
        run_loop(&rt, perceiver, planner, executor, &opts, &bus, &recorder)?
    };

    // ---- export the trajectory the compiler reads ----
    let run_row = recorder
        .get_run(&summary.run_id)
        .context("reading the run row")?
        .context("run row missing after the run")?;
    let steps = recorder
        .list_steps(&summary.run_id)
        .context("listing recorded steps")?;
    let trajectory = export_trajectory(&run_row, &steps);
    let traj_path = opts.out.join("trajectory.json");
    std::fs::write(&traj_path, serde_json::to_string_pretty(&trajectory)?)
        .with_context(|| format!("writing {}", traj_path.display()))?;

    println!(
        "explored `{}`: {} step(s) recorded, outcome {:?}",
        opts.goal, summary.steps, summary.outcome
    );
    if let Some(reason) = &summary.halted {
        println!("  run halted early: {reason:?}");
    }
    println!("  trajectory -> {}", traj_path.display());
    println!(
        "  compile it: operant compile {} {}",
        traj_path.display(),
        opts.out.join("compiled").display()
    );
    Ok(())
}

/// Build and run the loop, returning its summary. Generic over the synthesizer
/// so the two cfg branches above share one body.
fn run_loop<S: Synthesizer>(
    rt: &tokio::runtime::Runtime,
    perceiver: Box<dyn Perceiver>,
    planner: Box<dyn ModelBackend>,
    executor: Executor<S>,
    opts: &Opts,
    bus: &Bus,
    recorder: &Recorder,
) -> Result<RunSummary> {
    let explore = ExploreLoop::new(perceiver, planner, executor, opts.window_process.clone());
    let mut control = NoControl;
    rt.block_on(explore.run(bus, recorder, &opts.goal, &mut control))
        .context("the explore loop failed with an infrastructure error")
}

/// Turn the recorded run and its steps into the trajectory JSON shape
/// `operant compile` reads (`contracts/fixtures/trajectory_notepad.json`).
/// The recorder row shape and the trajectory shape agree field for field
/// (see `crates/compiler/src/trajectory.rs`); the extra provenance fields
/// carried here (grounding, started/ended, model_config) are ignored by the
/// compiler's `Trajectory` deserialization.
fn export_trajectory(run: &RunRecord, steps: &[StepRecord]) -> serde_json::Value {
    let steps_json: Vec<serde_json::Value> = steps
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
        "description": format!("Exported by `operant explore` from run {}", run.id),
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

/// The default/mock perceiver: the bundled Notepad snapshot fixture, with its
/// window process rewritten to whatever `--window-process` named so the loop's
/// perception and safety guard both resolve against it. Only the Notepad
/// element tree is modeled, so a mock run is only meaningful for a Notepad-shaped
/// task; a real teach run uses `--features real-uia` and the live `UiaPerceiver`.
#[cfg(not(feature = "real-uia"))]
fn fixture_perceiver(window_process: &str) -> Result<FixturePerceiver> {
    const RAW: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");
    let mut snap: operant_ir::Snapshot =
        serde_json::from_str(RAW).context("parsing the bundled fixture snapshot")?;
    snap.window.process = window_process.to_string();
    Ok(FixturePerceiver::single(snap))
}

/// The scripted default planner: click the editor, type an invoice note, save,
/// then `done`. A fixed 2-3 step Notepad task (the same narrative the golden
/// path e2e proves compiles and replays), ignoring the request content exactly
/// as `MockPlannerBackend` does. The real planner is the `AgentBridgeBackend`
/// under `dev-agent-bridge`; this is the headless/demo stand-in.
#[cfg(not(feature = "dev-agent-bridge"))]
fn scripted_mock_planner() -> MockPlannerBackend {
    // Center-of-bounds of the fixture's "Text editor" element ((100+1200/2),
    // (156+716/2)), i.e. the exact point the FixturePerceiver resolves the
    // click to, baked in so mock REPLAY (which has no live perceiver) clicks
    // the same point.
    let click = json!({
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
    });
    let type_note = json!({
        "id": "s2",
        "kind": "type",
        "intent": "Type the invoice note",
        "target": { "window": { "process": "notepad.exe" } },
        "params": { "text": "Invoice 2026-07-11 total $142.50" },
        "risk_class": "write",
        "grounding": "uia"
    });
    let save = json!({
        "id": "s3",
        "kind": "key",
        "intent": "Save the file",
        "target": { "window": { "process": "notepad.exe" } },
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

struct Opts {
    goal: String,
    window_process: String,
    out: PathBuf,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Option<Self>> {
        let mut goal = None;
        let mut window_process = None;
        let mut out = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => {
                    print_help();
                    return Ok(None);
                }
                "--goal" => {
                    i += 1;
                    goal = Some(args.get(i).cloned().context("--goal needs a value")?);
                }
                "--window-process" => {
                    i += 1;
                    window_process =
                        Some(args.get(i).cloned().context("--window-process needs a value")?);
                }
                "--out" => {
                    i += 1;
                    out = Some(PathBuf::from(
                        args.get(i).cloned().context("--out needs a value")?,
                    ));
                }
                other => anyhow::bail!("operant explore: unexpected argument `{other}`"),
            }
            i += 1;
        }
        let goal = goal.context("operant explore requires --goal <text>")?;
        let window_process =
            window_process.context("operant explore requires --window-process <exe>")?;
        let out = out.unwrap_or_else(|| PathBuf::from("out").join("explore"));
        Ok(Some(Self {
            goal,
            window_process,
            out,
        }))
    }
}

fn print_help() {
    println!("operant explore --goal <text> --window-process <exe> [--out <dir>]");
    println!();
    println!("Run the model-driven teach loop to completion and export a compiler-ready");
    println!("trajectory.json (plus a recorder.sqlite3) under <dir> (default ./out/explore).");
    println!();
    println!("Backends are selected at build time:");
    println!("  default            scripted mock planner, fixture perceiver, mock synth (headless)");
    println!("  --features real-uia,real-input    live UIA perception + real Windows input");
    println!("  --features dev-agent-bridge        an operator answers each turn via");
    println!("                                     OPERANT_AGENT_BRIDGE_DIR (see docs/evidence/agent-bridge-protocol.md)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_recorder::{RunMode, RunStatus};

    #[test]
    fn parse_requires_goal_and_window_process() {
        assert!(Opts::parse(&["--goal".into(), "g".into()]).is_err());
        let ok = Opts::parse(&[
            "--goal".into(),
            "g".into(),
            "--window-process".into(),
            "notepad.exe".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(ok.goal, "g");
        assert_eq!(ok.window_process, "notepad.exe");
        assert_eq!(ok.out, PathBuf::from("out").join("explore"));
    }

    #[test]
    fn export_trajectory_has_run_and_steps() {
        let run = RunRecord {
            id: "run_1".to_string(),
            goal: "do a thing".to_string(),
            mode: RunMode::Explore,
            started: 1000,
            ended: Some(2000),
            status: RunStatus::Completed,
            model_config: None,
        };
        let action: operant_ir::Action = serde_json::from_value(json!({
            "id": "s1", "kind": "key", "params": {"combo": "ctrl+s"},
            "risk_class": "write", "grounding": "uia"
        }))
        .unwrap();
        let step = StepRecord {
            id: "step_1".to_string(),
            run_id: "run_1".to_string(),
            seq: 1,
            action,
            grounding: operant_ir::Grounding::Uia,
            snapshot_digest_before: Some("d0".to_string()),
            snapshot_digest_after: Some("d1".to_string()),
            outcome: "ok".to_string(),
            ms: 10,
            note: None,
            human_correction: None,
            outcome_bearing: false,
            created_at: 0,
        };
        let traj = export_trajectory(&run, &[step]);
        assert_eq!(traj["v"], 1);
        assert_eq!(traj["run"]["id"], "run_1");
        assert_eq!(traj["run"]["goal"], "do a thing");
        assert_eq!(traj["run"]["mode"], "explore");
        assert_eq!(traj["run"]["status"], "completed");
        assert_eq!(traj["steps"][0]["seq"], 1);
        assert_eq!(traj["steps"][0]["action"]["id"], "s1");
        assert_eq!(traj["steps"][0]["snapshot_digest_before"], "d0");

        // The exported JSON must deserialize as the compiler's own input type.
        let reparsed: operant_compiler::Trajectory =
            serde_json::from_value(traj).expect("exported trajectory parses as a compiler Trajectory");
        assert_eq!(reparsed.run.id, "run_1");
        assert_eq!(reparsed.steps.len(), 1);
    }
}
