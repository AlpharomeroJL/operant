//! `operant list [<dir>]`: scan a directory for compiled workflow
//! manifests (`manifest.json`, the `contracts/` manifest shape) and print
//! each one's name, version, step count, and risk ceiling. Default `<dir>`
//! is the current directory.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use operant_ir::Manifest;

const MAX_DEPTH: usize = 4;

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    let dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut found = Vec::new();
    walk(&dir, 0, &mut found).with_context(|| format!("scanning {}", dir.display()))?;

    if found.is_empty() {
        println!("no workflow manifests found under {}", dir.display());
        return Ok(());
    }

    found.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));
    for (path, manifest) in &found {
        println!(
            "{}@{}  {} step(s)  risk={}  -- {}",
            manifest.name,
            manifest.version,
            manifest.step_summary.len(),
            risk_str(manifest.capabilities.risk_ceiling),
            path.display()
        );
    }
    println!();
    println!("{} workflow(s)", found.len());
    Ok(())
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, Manifest)>) -> Result<()> {
    if depth > MAX_DEPTH || !dir.is_dir() {
        return Ok(());
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // an unreadable subdirectory is skipped, not fatal
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("node_modules") {
                continue;
            }
            walk(&path, depth + 1, out)?;
        } else if path.file_name().and_then(|n| n.to_str()) == Some("manifest.json") {
            match fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<Manifest>(&raw).ok())
            {
                Some(manifest) => out.push((path, manifest)),
                None => eprintln!(
                    "operant: skipping {} (not a valid workflow manifest)",
                    path.display()
                ),
            }
        }
    }
    Ok(())
}

fn risk_str(r: operant_ir::RiskClass) -> &'static str {
    match r {
        operant_ir::RiskClass::Read => "read",
        operant_ir::RiskClass::Write => "write",
        operant_ir::RiskClass::Destructive => "destructive",
    }
}

fn print_help() {
    println!("operant list [<dir>]");
    println!();
    println!("List compiled workflow manifests found under <dir> (default: .).");
}
