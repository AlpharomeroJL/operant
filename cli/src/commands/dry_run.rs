//! `operant dry-run <compiled.json>`: show what `run` would do and
//! validate that it could, without dispatching a single synthesizer call
//! -- not even against the mock. Always checks every `click` step carries
//! a resolvable `coords_last_known` (headless replay has no live perceiver
//! to resolve one at run time; an unresolved click is
//! `operant_replay::ReplayError::Unresolved` at `run` time). Evaluates
//! preconditions too, when a snapshot is available.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use operant_gates::{evaluate_gate, EvalContext};
use operant_ir::{ActionKind, GateKind, Manifest, RiskClass};

use crate::commands::run::{fmt_results, load_compiled, parse_inputs, Opts};
use crate::snapshot;

pub fn run(args: &[String]) -> Result<()> {
    let opts = Opts::parse(
        args,
        "operant dry-run <compiled.json> [--inputs k=v,k2=v2] [--snapshot <snapshot.json>]",
    )?;
    let Some(opts) = opts else { return Ok(()) };

    let workflow = load_compiled(&opts.workflow_path)?;
    let inputs = parse_inputs(opts.inputs.as_deref());
    let bindings = merge_defaults(&workflow.manifest, &inputs);

    println!(
        "`{}` v{} -- {} step(s), risk ceiling: {}",
        workflow.manifest.name,
        workflow.manifest.version,
        workflow.actions.len(),
        risk_str(workflow.manifest.capabilities.risk_ceiling)
    );
    println!();

    if !bindings.is_empty() {
        println!("Inputs:");
        for (k, v) in &bindings {
            println!("  {k} = {v}");
        }
        println!();
    }

    println!("Would run:");
    for (i, step) in workflow.manifest.step_summary.iter().enumerate() {
        println!("  {}. {}", i + 1, step);
    }

    let mut problems = Vec::new();
    for action in &workflow.actions {
        if action.kind == ActionKind::Click {
            let resolved = action
                .target
                .as_ref()
                .and_then(|t| t.coords_last_known.as_ref());
            if resolved.is_none() {
                problems.push(format!(
                    "step `{}` ({}) has no cached coordinates to replay",
                    action.id,
                    action.intent.as_deref().unwrap_or("<no intent>")
                ));
            }
        }
    }

    let mut pre_checked = false;
    if let Some(path) = &opts.snapshot_path {
        let snap = snapshot::load_snapshot(&PathBuf::from(path))?;
        let ctx = EvalContext::new().with_snapshot(snap);
        let pre_results: Result<Vec<_>, _> = workflow
            .manifest
            .gates
            .iter()
            .filter(|g| g.kind == GateKind::Pre)
            .map(|g| evaluate_gate(g, &ctx))
            .collect();
        let pre_results =
            pre_results.map_err(|e| anyhow::anyhow!("evaluating precondition gates: {e}"))?;
        println!();
        println!("Preconditions: {}", fmt_results(&pre_results));
        pre_checked = true;
        if pre_results
            .iter()
            .any(|r| *r != operant_ir::GateResult::Pass)
        {
            problems.push("a precondition gate would fail".to_string());
        }
    }

    println!();
    if problems.is_empty() {
        println!(
            "OK: this workflow is ready to replay headless.{}",
            if pre_checked {
                ""
            } else {
                " (pass --snapshot to also check preconditions)"
            }
        );
        Ok(())
    } else {
        for p in &problems {
            eprintln!("problem: {p}");
        }
        anyhow::bail!(
            "{} problem(s) found; `operant run` would not complete cleanly",
            problems.len()
        );
    }
}

fn merge_defaults(
    manifest: &Manifest,
    inputs: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(props) = manifest
        .inputs_schema
        .get("properties")
        .and_then(|v| v.as_object())
    {
        for (name, schema) in props {
            if let Some(default) = schema.get("default").and_then(|v| v.as_str()) {
                out.insert(name.clone(), default.to_string());
            }
        }
    }
    for (k, v) in inputs {
        out.insert(k.clone(), v.clone());
    }
    out
}

fn risk_str(r: RiskClass) -> &'static str {
    match r {
        RiskClass::Read => "read",
        RiskClass::Write => "write",
        RiskClass::Destructive => "destructive",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_defaults_overlays_caller_inputs_on_schema_defaults() {
        let manifest_json = json!({
            "v": 1, "name": "x", "version": "1.0.0", "description": "d",
            "step_summary": [], "inputs_schema": {
                "type": "object",
                "properties": { "a": { "type": "string", "default": "one" }, "b": { "type": "string", "default": "two" } }
            },
            "capabilities": { "apps": [], "paths": [], "network": false, "risk_ceiling": "read" },
            "dsl": { "path": "workflow.ts", "hash": "h" }
        });
        let manifest: Manifest = serde_json::from_value(manifest_json).unwrap();
        let mut inputs = BTreeMap::new();
        inputs.insert("b".to_string(), "override".to_string());
        let merged = merge_defaults(&manifest, &inputs);
        assert_eq!(merged.get("a").map(String::as_str), Some("one"));
        assert_eq!(merged.get("b").map(String::as_str), Some("override"));
    }
}
