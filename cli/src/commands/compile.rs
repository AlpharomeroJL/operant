//! `operant compile <trajectory.json> [<out_dir>]`: trajectory -> compiled
//! workflow via `operant_compiler::compile` (C14/FR-O4).
//!
//! Writes three artifacts: `manifest.json` (the `contracts/` manifest
//! shape), `workflow.ts` (the readable DSL over `@operant/sdk`), and
//! `compiled.json` (`{ manifest, actions }`, the wire shape
//! `operant_compiler::CompiledWorkflow` already serializes to and what
//! `operant run`/`dry-run`/`explain` and the MCP server consume; not a
//! `contracts/` shape of its own, just this build's replay-ready form).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use operant_compiler::{compile, Trajectory};

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let traj_path = args
        .first()
        .context("usage: operant compile <trajectory.json> [<out_dir>]")?;
    let raw =
        fs::read_to_string(traj_path).with_context(|| format!("reading trajectory {traj_path}"))?;
    let traj: Trajectory =
        serde_json::from_str(&raw).with_context(|| format!("parsing trajectory {traj_path}"))?;
    let compilation = compile(&traj).context("compiling trajectory")?;

    let out_dir: PathBuf = match args.get(1) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new("compiled").join(&compilation.workflow.manifest.name),
    };
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("creating output directory {}", out_dir.display()))?;

    let manifest_path = out_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&compilation.workflow.manifest)?,
    )
    .with_context(|| format!("writing {}", manifest_path.display()))?;

    let dsl_path = out_dir.join("workflow.ts");
    fs::write(&dsl_path, &compilation.dsl_source)
        .with_context(|| format!("writing {}", dsl_path.display()))?;

    let compiled_path = out_dir.join("compiled.json");
    fs::write(
        &compiled_path,
        serde_json::to_string_pretty(&compilation.workflow)?,
    )
    .with_context(|| format!("writing {}", compiled_path.display()))?;

    println!(
        "compiled `{}` v{} ({} steps) -> {}",
        compilation.workflow.manifest.name,
        compilation.workflow.manifest.version,
        compilation.workflow.actions.len(),
        out_dir.display()
    );
    println!("  {}", manifest_path.display());
    println!("  {}", dsl_path.display());
    println!("  {}", compiled_path.display());
    Ok(())
}

fn print_help() {
    println!("operant compile <trajectory.json> [<out_dir>]");
    println!();
    println!("Compile a recorded trajectory into a workflow: manifest.json, workflow.ts,");
    println!("and compiled.json (manifest + actions; what run/dry-run/explain consume).");
    println!("Default <out_dir> is ./compiled/<workflow-name>.");
}
