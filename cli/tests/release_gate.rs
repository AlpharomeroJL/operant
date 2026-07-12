//! B15 release gate: prove `release/scripts/check-release-artifact.mjs` fires
//! BOTH ways. A real-feature capability blob (real_uia/real_input true) PASSES
//! (exit 0); a mock capability blob (either false) FAILS (exit nonzero). This is
//! the "a deliberately mock-built artifact fails the release gate" requirement
//! (release/BUILD-MATRIX.md).
//!
//! The gate decision is exercised through the actual gate script (via `--caps`),
//! so this test guards the shipped gate, not a reimplementation of it. It needs
//! no built binary and makes no network calls, so it stays inside the
//! deterministic `cargo test` / `just verify` gate. `node` is a declared
//! toolchain dependency (release/REPRODUCIBLE.md; `just ci` runs it already).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The gate script, located from this crate's manifest dir so the test does not
/// depend on the process working directory (cargo runs tests from the crate dir).
fn gate_script() -> PathBuf {
    // CARGO_MANIFEST_DIR is <repo>/cli; the repo root is its parent.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let repo_root = Path::new(manifest_dir)
        .parent()
        .expect("cli crate has a parent repo root");
    let script = repo_root
        .join("release")
        .join("scripts")
        .join("check-release-artifact.mjs");
    assert!(
        script.exists(),
        "gate script not found at {}",
        script.display()
    );
    script
}

/// Write a capability blob to a unique temp file and return its path.
fn write_blob(tag: &str, json: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "operant_release_gate_{}_{}.json",
        std::process::id(),
        tag
    ));
    std::fs::write(&path, json).expect("writing the capability blob");
    path
}

/// Run the gate script against a capability file. Returns the process exit code
/// (an `i32`; `None` only if the process was killed by a signal, unexpected here).
fn run_gate(caps_path: &Path) -> Option<i32> {
    let script = gate_script();
    let output = Command::new("node")
        .arg(&script)
        .arg("--caps")
        .arg(caps_path)
        .output()
        .expect("`node` must be on PATH to run the release-gate test");
    // Surface the gate's own messages when a case does not behave as expected.
    eprintln!(
        "gate stdout: {}",
        String::from_utf8_lossy(&output.stdout).trim()
    );
    eprintln!(
        "gate stderr: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    output.status.code()
}

/// A real, shippable core reports both automation booleans true. This is the
/// shape a `--features real-uia,real-input,real-transport` build emits
/// (cli/src/commands/capabilities.rs), and the case the gate must PASS.
const REAL_BLOB: &str = r#"{
  "real_uia": true,
  "real_input": true,
  "real_vision": false,
  "mock_planner_only": true,
  "transport_kind": "stdio",
  "version": "1.0.0",
  "git_sha": "unknown"
}"#;

/// A mock core reports both automation booleans false. These are exactly the
/// seven fields of `contracts/fixtures/ipc/handshake.json`'s `response.result`
/// (the real capture from a default/mock recorder build), the case the gate
/// must FAIL.
const MOCK_BLOB: &str = r#"{
  "real_uia": false,
  "real_input": false,
  "real_vision": false,
  "mock_planner_only": true,
  "transport_kind": "stdio",
  "version": "1.0.0",
  "git_sha": "unknown"
}"#;

/// A half-mock core (only one real feature) must also FAIL: the E4 rule requires
/// BOTH features together, and one alone silently degrades to the mock path
/// (cli/src/commands/run.rs).
const HALF_BLOB: &str = r#"{
  "real_uia": true,
  "real_input": false,
  "real_vision": false,
  "mock_planner_only": true,
  "transport_kind": "stdio",
  "version": "1.0.0",
  "git_sha": "unknown"
}"#;

#[test]
fn real_capability_blob_passes_the_gate() {
    let path = write_blob("real", REAL_BLOB);
    let code = run_gate(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        code,
        Some(0),
        "a real-feature capability blob must PASS the release gate (exit 0)"
    );
}

#[test]
fn mock_capability_blob_fails_the_gate() {
    let path = write_blob("mock", MOCK_BLOB);
    let code = run_gate(&path);
    let _ = std::fs::remove_file(&path);
    // The core requirement: a mock artifact FAILS with a nonzero exit.
    assert_ne!(
        code,
        Some(0),
        "a mock capability blob must FAIL the release gate (nonzero exit)"
    );
    assert_eq!(
        code,
        Some(1),
        "the gate should fail with exit code 1 on a mock artifact"
    );
}

#[test]
fn half_real_capability_blob_fails_the_gate() {
    let path = write_blob("half", HALF_BLOB);
    let code = run_gate(&path);
    let _ = std::fs::remove_file(&path);
    assert_ne!(
        code,
        Some(0),
        "only one real feature must still FAIL the gate (E4: both are required)"
    );
}
