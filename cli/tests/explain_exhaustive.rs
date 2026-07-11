//! Exhaustive test: prove that EVERY fixture and cookbook workflow
//! explains through `operant explain` without glossary violations.
//!
//! This test:
//! 1. Finds all fixture manifest.json files in contracts/fixtures/**/*.json
//! 2. For each, runs `operant explain` and validates the output
//! 3. Checks that the rendered output contains no internal glossary terms
//!
//! Cookbook workflows (cookbook/*.ts) are validated via a complementary
//! Node-based test in scripts/ and doctest.mjs, which imports and validates
//! them directly. This Rust test focuses on the CLI's explain command.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Glossary: internal terms that must NOT appear in explain output
/// (user-facing terms only). Mirrors contracts/microcopy_glossary.json.
fn glossary_internal_terms() -> Vec<&'static str> {
    vec![
        "trajectory",
        "compile",
        "grounding",
        "DSL",
        "manifest",
        "MCP",
        "invariant",
        "gate",
        "precondition",
        "postcondition",
        "selector",
        "anchor",
        "replay",
        "explore",
        "drift",
        "re-ground",
        "sidecar",
        "backend",
        "inference",
        "token",
        "VRAM",
        "API key",
        "OAuth",
        "PKCE",
        "capability grant",
        "risk class",
        "dry-run",
        "audit chain",
        "hash",
        "CDP",
        "UIA",
        "OCR",
        "daemon",
        "regex",
        "cron",
        "stdout",
        "stderr",
    ]
}

/// Find all fixture JSON files that have a manifest (either as a top-level
/// object with a "name" key, or as part of a compiled.json with manifest+actions)
fn find_fixture_manifests() -> Vec<PathBuf> {
    let mut manifests = Vec::new();

    // Try multiple possible locations since tests can run from different dirs
    let possible_dirs = vec![
        PathBuf::from("contracts/fixtures"),
        PathBuf::from("../contracts/fixtures"),
        PathBuf::from("../../contracts/fixtures"),
    ];

    let fixtures_dir = possible_dirs
        .iter()
        .find(|dir| dir.exists())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("contracts/fixtures"));

    if !fixtures_dir.exists() {
        eprintln!("Warning: fixtures directory not found at {:?}", fixtures_dir);
        return manifests;
    }

    // Walk fixtures directory
    if let Ok(entries) = fs::read_dir(&fixtures_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                // Skip non-manifest files
                if path.file_name().map_or(false, |name| {
                    let n = name.to_string_lossy();
                    n == "manifest.json" || n == "compiled.json"
                }) {
                    manifests.push(path);
                }
            } else if path.is_dir() {
                // Check subdirectories for manifest.json
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub_entry in sub_entries.flatten() {
                        let sub_path = sub_entry.path();
                        if sub_path.file_name().map_or(false, |name| {
                            name.to_string_lossy() == "manifest.json"
                        }) {
                            manifests.push(sub_path);
                        }
                    }
                }
            }
        }
    }

    manifests.sort();
    manifests
}

/// Run `operant explain` on a file and return the output
fn run_explain(manifest_path: &PathBuf) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(&["run", "--", "explain"])
        .arg(manifest_path)
        .current_dir(".")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run operant explain: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "operant explain failed for {:?}: {}",
            manifest_path, stderr
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Check if text contains any glossary internal terms (case-insensitive word boundary match)
fn has_glossary_violations(text: &str, terms: &[&str]) -> Vec<String> {
    let mut violations = Vec::new();

    for term in terms {
        // Word-boundary, case-insensitive regex (simplified: just check word boundaries)
        let pattern_lower = term.to_lowercase();
        let text_lower = text.to_lowercase();

        // Simple word-boundary check: look for the term surrounded by non-alphanumeric
        for (i, _) in text_lower.match_indices(&pattern_lower) {
            let before_ok = i == 0 || !text_lower.chars().nth(i - 1).unwrap().is_alphanumeric();
            let after_idx = i + pattern_lower.len();
            let after_ok = after_idx >= text_lower.len()
                || !text_lower.chars().nth(after_idx).unwrap().is_alphanumeric();

            if before_ok && after_ok {
                violations.push(format!(
                    "Found internal term '{}' in output",
                    term
                ));
                break; // Only report once per term
            }
        }
    }

    violations
}

#[test]
fn test_explain_exhaustive_fixtures() {
    let glossary = glossary_internal_terms();
    let manifests = find_fixture_manifests();

    if manifests.is_empty() {
        eprintln!("Warning: No fixture manifests found");
        return;
    }

    println!("Testing {} fixture manifest(s)...", manifests.len());

    let mut all_violations = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    for manifest_path in &manifests {
        print!("  Testing {:?}... ", manifest_path);

        match run_explain(manifest_path) {
            Ok(output) => {
                let violations = has_glossary_violations(&output, &glossary);
                if violations.is_empty() {
                    println!("OK");
                    passed += 1;
                } else {
                    println!("GLOSSARY VIOLATIONS");
                    failed += 1;
                    all_violations.push((manifest_path.clone(), violations));
                }
            }
            Err(e) => {
                println!("ERROR: {}", e);
                failed += 1;
                all_violations.push((
                    manifest_path.clone(),
                    vec![format!("explain failed: {}", e)],
                ));
            }
        }
    }

    println!("\nResults: {} passed, {} failed", passed, failed);

    if !all_violations.is_empty() {
        eprintln!("\nGlossary violations found:");
        for (path, violations) in all_violations {
            eprintln!("  {:?}", path);
            for violation in violations {
                eprintln!("    - {}", violation);
            }
        }
        panic!("Exhaustive test found glossary violations");
    }
}

#[test]
fn test_explain_fixture_coverage() {
    // Verify we're testing known fixtures
    let manifests = find_fixture_manifests();
    assert!(
        !manifests.is_empty(),
        "No fixture manifests found in contracts/fixtures"
    );

    // Log what we found
    println!("Found {} fixture(s):", manifests.len());
    for m in &manifests {
        println!("  - {}", m.display());
    }

    // Verify we have at least the known fixtures
    let paths_str: Vec<String> = manifests.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    assert!(
        paths_str.iter().any(|p| p.contains("workflow_notepad")),
        "Expected to find workflow_notepad fixture"
    );
}

#[test]
fn test_explain_cookbook_workflows() {
    // Test that cookbook workflows can be rendered/explained
    // Invokes scripts/test_cookbook_explain.mjs which imports and validates
    // every cookbook workflow via renderWorkflow
    println!("Testing cookbook workflows via Node...");

    // Find the script in multiple possible locations
    let possible_scripts = vec![
        PathBuf::from("scripts/test_cookbook_explain.mjs"),
        PathBuf::from("../scripts/test_cookbook_explain.mjs"),
        PathBuf::from("../../scripts/test_cookbook_explain.mjs"),
    ];

    let script = possible_scripts
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("scripts/test_cookbook_explain.mjs"));

    if !script.exists() {
        eprintln!("Warning: test_cookbook_explain.mjs not found, skipping cookbook tests");
        return;
    }

    let output = Command::new("node")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run test_cookbook_explain.mjs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("{}", stdout);
    if !output.status.success() {
        eprintln!("{}", stderr);
        panic!("Cookbook explain test failed");
    }
}
