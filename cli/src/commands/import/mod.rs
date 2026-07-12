//! `operant import <format> <args>`: import an existing test/spec into a
//! workflow skeleton the same way `compile` does (X9).
//!
//! `playwright` is the only importer today: `operant import playwright
//! <spec.ts> [<out_dir>]` parses a basic subset of a Playwright spec
//! (goto/click/fill/expect), maps it onto the `browser` namespace's Action
//! IR (`crates/action/src/adapters/browser`, L9A), and runs the result
//! through `operant_compiler::compile` (L8A), writing the same three
//! artifacts `operant compile` does: `manifest.json`, `workflow.ts`, and
//! `compiled.json`.

pub mod playwright;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    match args.first().map(String::as_str) {
        Some("playwright") => run_playwright(&args[1..]),
        Some(other) => bail!(
            "operant import: unknown importer `{other}` (only `playwright` is supported)"
        ),
        None => {
            print_help();
            Ok(())
        }
    }
}

fn run_playwright(args: &[String]) -> Result<()> {
    let spec_path = args
        .first()
        .context("usage: operant import playwright <spec.ts> [<out_dir>]")?;
    let spec_path = PathBuf::from(spec_path);
    let spec_text = fs::read_to_string(&spec_path)
        .with_context(|| format!("reading Playwright spec {}", spec_path.display()))?;
    let spec_dir = spec_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let outcome = playwright::import(&spec_text, &spec_dir)
        .with_context(|| format!("importing {}", spec_path.display()))?;
    let compilation = outcome.compilation;

    let out_dir: PathBuf = match args.get(1) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new("imported").join(&compilation.workflow.manifest.name),
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
        "imported `{}` v{} ({} steps) from {} -> {}",
        compilation.workflow.manifest.name,
        compilation.workflow.manifest.version,
        compilation.workflow.actions.len(),
        spec_path.display(),
        out_dir.display()
    );
    println!("  {}", manifest_path.display());
    println!("  {}", dsl_path.display());
    println!("  {}", compiled_path.display());

    if !outcome.todo_notes.is_empty() {
        println!(
            "  {} step(s) could not be mapped and became TODO markers:",
            outcome.todo_notes.len()
        );
        for note in &outcome.todo_notes {
            println!("    - {note}");
        }
    }

    Ok(())
}

fn print_help() {
    println!("operant import playwright <spec.ts> [<out_dir>]");
    println!();
    println!("Import a Playwright spec (goto/click/fill/expect) into a workflow skeleton:");
    println!("manifest.json, workflow.ts, and compiled.json, the same three artifacts");
    println!("`operant compile` writes. A statement this basic importer cannot map");
    println!("becomes a TODO marker step in the emitted workflow, not a dropped step");
    println!("or a crash.");
    println!("Default <out_dir> is ./imported/<workflow-name>.");
}
