//! `operant explain <compiled.json|manifest.json>`: render a workflow to
//! plain English using the existing `@operant/sdk/render` renderer
//! (`sdk/ts/src/render`, owned by U4A). This file does not reimplement
//! that renderer in Rust; it calls it through a thin Node adapter script,
//! `cli/scripts/explain.mjs`, which does nothing but import
//! `renderWorkflow` and hand back its result as JSON. All formatting
//! happens here, in Rust.
//!
//! Requires `node` on `PATH`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    let path = args
        .first()
        .context("usage: operant explain <compiled.json|manifest.json>")?;
    let rendered = render_workflow_json(path)?;
    print_rendered(&rendered);
    Ok(())
}

/// Read a `compiled.json` or bare `manifest.json` at `path` and render it to
/// the `{title, summary, grant, inputs, steps}` plain-English JSON via
/// `@operant/sdk/render`. Shared by the `explain` verb and the
/// `explain_workflow` IPC command (`contracts/ipc.md` section 5c), which
/// returns this value verbatim as its result.
pub(crate) fn render_workflow_json(path: &str) -> Result<Value> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    let doc: Value =
        serde_json::from_str(&raw).with_context(|| format!("parsing {path} as JSON"))?;

    // Accept either a `compiled.json` ({ manifest, actions }) or a bare
    // manifest.json (no actions: the numbered-steps section is then empty,
    // but the summary/grant/inputs sections still render).
    let (manifest, steps) = match (doc.get("manifest"), doc.get("actions")) {
        (Some(m), Some(a)) => (m.clone(), a.clone()),
        _ => (doc.clone(), Value::Array(vec![])),
    };

    render_via_node(&manifest, &steps)
}

fn render_via_node(manifest: &Value, steps: &Value) -> Result<Value> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("explain.mjs");
    let payload = serde_json::json!({ "manifest": manifest, "steps": steps }).to_string();

    let mut child = Command::new("node")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "launching `node {}` -- is Node.js installed and on PATH?",
                script.display()
            )
        })?;

    child
        .stdin
        .take()
        .expect("stdin was requested as piped")
        .write_all(payload.as_bytes())
        .context("writing to the explain.mjs subprocess")?;

    let output = child
        .wait_with_output()
        .context("waiting for the explain.mjs subprocess")?;
    if !output.status.success() {
        bail!(
            "explain.mjs failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("explain.mjs did not print valid JSON")
}

fn print_rendered(rendered: &Value) {
    let title = rendered
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Workflow");
    let summary = rendered
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("");
    let grant = rendered.get("grant").and_then(Value::as_str).unwrap_or("");

    println!("{title}");
    if !summary.is_empty() && summary != title {
        println!("{summary}");
    }
    println!();
    println!("{grant}");

    if let Some(inputs) = rendered.get("inputs").and_then(Value::as_array) {
        if !inputs.is_empty() {
            println!();
            println!("Inputs:");
            for input in inputs {
                let label = input.get("label").and_then(Value::as_str).unwrap_or("");
                let value = input.get("value").and_then(Value::as_str).unwrap_or("");
                println!("  - {label}: {value}");
            }
        }
    }

    if let Some(steps) = rendered.get("steps").and_then(Value::as_array) {
        if !steps.is_empty() {
            println!();
            println!("Steps:");
            for step in steps {
                let n = step.get("n").and_then(Value::as_u64).unwrap_or(0);
                let sentence = step.get("sentence").and_then(Value::as_str).unwrap_or("");
                let irreversible = step
                    .get("irreversible")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let marker = if irreversible {
                    "  (cannot be undone)"
                } else {
                    ""
                };
                println!("  {n}. {sentence}{marker}");
            }
        }
    }
}

fn print_help() {
    println!("operant explain <compiled.json|manifest.json>");
    println!();
    println!("Render a workflow's plain-English summary, permissions, inputs, and");
    println!("numbered steps via @operant/sdk/render. Requires Node.js on PATH.");
}
