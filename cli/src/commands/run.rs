//! `operant run <compiled.json>`: replay a compiled workflow via
//! `operant_replay::Replayer` (C14/FR-O4). The default build -- and every
//! test, `just golden`, and `just ci` -- replays against a mock synthesizer,
//! headless and deterministic. A REAL build (compiled with both the
//! `real-uia` and `real-input` features) instead drives the real Windows
//! synthesizer AND installs a live UIA `Perceiver`, so a click re-resolves
//! its selector chain against the live desktop at run time (KI-1) rather than
//! replaying the coordinate cached at teach time.
//!
//! Every side effect still goes through `operant-replay`'s injected
//! `Synthesizer`, and replay stays model- and network-free in BOTH builds: a
//! `Perceiver` is a PERCEPTION backend, never a model backend, so nothing
//! here can reach a model or a network call (`crates/replay`'s own crate-graph
//! guarantee, `crates/replay/src/lib.rs`'s `replay_crate_is_backend_free` test).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use operant_action::Synthesizer;
use operant_gates::EvalContext;
use operant_ir::GateResult;
use operant_replay::{CompiledWorkflow, Replayer};

use crate::snapshot;

pub fn run(args: &[String]) -> Result<()> {
    let opts = Opts::parse(
        args,
        "operant run <compiled.json> [--inputs k=v,k2=v2] [--snapshot <snapshot.json>]",
    )?;
    let Some(opts) = opts else { return Ok(()) };

    let workflow = load_compiled(&opts.workflow_path)?;
    let inputs = parse_inputs(opts.inputs.as_deref());
    let gate_snapshot = match opts.snapshot_path {
        Some(path) => snapshot::load_snapshot(&PathBuf::from(path))?,
        None => snapshot::bundled_notepad_snapshot(),
    };
    let ctx = EvalContext::new().with_snapshot(gate_snapshot);

    // Construct the replayer. A REAL run (compiled with BOTH `real-uia` and
    // `real-input`) drives real Windows input AND installs a live UIA
    // Perceiver, so a click re-resolves its selector chain against the live
    // desktop at run time (KI-1) rather than replaying the coordinate cached
    // at teach time. Every other build -- the default, `just golden`, `just
    // ci`, every test -- keeps the deterministic, model-free mock path.
    // Replay stays backend-free either way: a Perceiver is a PERCEPTION
    // backend, never a model backend (crates/replay is proven backend-free by
    // its own crate graph and `replay_crate_is_backend_free`).
    #[cfg(all(feature = "real-uia", feature = "real-input"))]
    {
        use operant_action::WindowsSynthesizer;
        use operant_perception_uia::UiaPerceiver;
        let replayer = Replayer::new(WindowsSynthesizer::new())
            .with_perceiver(Box::new(UiaPerceiver::new()));
        report_replay(&replayer, &workflow, &inputs, &ctx)?;
    }
    #[cfg(not(all(feature = "real-uia", feature = "real-input")))]
    {
        let replayer = Replayer::with_mock();
        report_replay(&replayer, &workflow, &inputs, &ctx)?;
    }
    Ok(())
}

/// Replay a compiled workflow through `replayer` and print the step count and
/// pre/post gate results. Generic over the [`Synthesizer`] so the identical
/// reporting path serves both the default mock replayer and, in a `real-uia`
/// + `real-input` build, the real Windows-backed one whose click steps
/// re-resolve against the live desktop.
fn report_replay<S: Synthesizer>(
    replayer: &Replayer<S>,
    workflow: &CompiledWorkflow,
    inputs: &BTreeMap<String, String>,
    ctx: &EvalContext,
) -> Result<()> {
    let report = replayer.replay_compiled(workflow, inputs, ctx, ctx)?;
    println!(
        "replayed `{}` v{}: {} step(s) executed",
        workflow.manifest.name, workflow.manifest.version, report.steps_executed
    );
    println!("  pre-conditions:  {}", fmt_results(&report.pre));
    println!("  post-conditions: {}", fmt_results(&report.post));
    Ok(())
}

/// Shared by `run` and `dry-run`: `<workflow> [--inputs ...] [--snapshot ...]`.
pub(crate) struct Opts {
    pub workflow_path: String,
    pub inputs: Option<String>,
    pub snapshot_path: Option<String>,
}

impl Opts {
    pub(crate) fn parse(args: &[String], usage: &str) -> Result<Option<Self>> {
        let mut positional = Vec::new();
        let mut inputs = None;
        let mut snapshot_path = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => {
                    println!("{usage}");
                    return Ok(None);
                }
                "--inputs" => {
                    i += 1;
                    inputs = Some(args.get(i).cloned().context("--inputs needs a value")?);
                }
                "--snapshot" => {
                    i += 1;
                    snapshot_path = Some(args.get(i).cloned().context("--snapshot needs a value")?);
                }
                other => positional.push(other.to_string()),
            }
            i += 1;
        }
        let workflow_path = positional.into_iter().next().context(usage.to_string())?;
        Ok(Some(Self {
            workflow_path,
            inputs,
            snapshot_path,
        }))
    }
}

pub(crate) fn load_compiled(path: &str) -> Result<CompiledWorkflow> {
    let raw = fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing compiled workflow {path}"))
}

/// Parse `--inputs k=v,k2=v2` into workflow input bindings. Overrides the
/// compiled workflow's own `inputs_schema` defaults; unset keys keep
/// whatever default the workflow declares.
pub(crate) fn parse_inputs(arg: Option<&str>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(arg) = arg else { return out };
    for pair in arg.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        if let Some((k, v)) = pair.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

pub(crate) fn fmt_results(results: &[GateResult]) -> String {
    if results.is_empty() {
        return "(none)".to_string();
    }
    let all_pass = results.iter().all(|r| *r == GateResult::Pass);
    let detail: Vec<&str> = results
        .iter()
        .map(|r| match r {
            GateResult::Pass => "pass",
            GateResult::Fail => "fail",
        })
        .collect();
    format!(
        "{} [{}]",
        if all_pass { "PASS" } else { "FAIL" },
        detail.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_inputs_splits_pairs_and_trims_whitespace() {
        let got = parse_inputs(Some("invoice_date=2026-08-01, amount = 9.99"));
        assert_eq!(
            got.get("invoice_date").map(String::as_str),
            Some("2026-08-01")
        );
        assert_eq!(got.get("amount").map(String::as_str), Some("9.99"));
    }

    #[test]
    fn parse_inputs_of_none_is_empty() {
        assert!(parse_inputs(None).is_empty());
    }

    #[test]
    fn fmt_results_reports_pass_only_when_every_gate_passed() {
        assert_eq!(
            fmt_results(&[GateResult::Pass, GateResult::Pass]),
            "PASS [pass, pass]"
        );
        assert_eq!(
            fmt_results(&[GateResult::Pass, GateResult::Fail]),
            "FAIL [pass, fail]"
        );
        assert_eq!(fmt_results(&[]), "(none)");
    }
}
