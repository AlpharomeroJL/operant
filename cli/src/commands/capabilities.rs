//! `operant capabilities`: print THIS build's capability blob (the
//! `get_capabilities` handshake result, `contracts/ipc.md` section 3) to stdout
//! as JSON, then exit 0.
//!
//! This is the one-shot introspection path the release gate uses
//! (`release/scripts/check-release-artifact.mjs`, `just check-release-artifact`):
//! it reads a built core binary's OWN reported capabilities and refuses to ship
//! a mock artifact as a product. The four automation booleans follow build cfg
//! and are constant for a process lifetime (`contracts/ipc.md` section 3), so a
//! one-shot read of them is a faithful description of what the binary will do.
//!
//! The booleans are computed from the SAME cfg flags the rest of the CLI keys
//! off (`cli/src/commands/run.rs`'s E4 rule, `cli/src/commands/record_ipc.rs`'s
//! handshake), so this output cannot disagree with what the binary actually does
//! at run time. That is the whole point: capability is read from the compiled
//! artifact, never asserted in a doc. A default (mock) build reports
//! `real_uia=false`/`real_input=false` and FAILS the gate; a release build
//! (`--features real-uia,real-input,real-transport`) reports them `true` and
//! PASSES. See `release/BUILD-MATRIX.md`.

use anyhow::Result;
use serde_json::{json, Value};

/// The build version string, matching `operant --version` (`cli/src/main.rs`).
const VERSION: &str = "1.0.0";

/// This build's capability object, byte-shaped per `contracts/ipc.md` section 3
/// (the `get_capabilities` result). Every field is derived from compile-time
/// cfg or a fixed constant, so it describes the artifact, not an intention.
pub fn capabilities() -> Value {
    json!({
        // Live UIA perception is linked only under `real-uia`
        // (operant-perception-uia's UiaPerceiver). Mock builds report false.
        "real_uia": cfg!(feature = "real-uia"),
        // Real Windows input (operant-action's WindowsSynthesizer) is linked only
        // under `real-input`. Mock builds report false.
        "real_input": cfg!(feature = "real-input"),
        // No vision grounder sidecar is linked into the core binary, so pixel
        // grounding is never available from the CLI core itself.
        "real_vision": false,
        // The only compiled-in planner is the scripted mock unless the dev-only
        // agent bridge is built in. A release build never enables it, so a
        // shipped core reports true here; that is honest for the CLI core, whose
        // real-model teaching path is wired by the shell at runtime, not by a
        // compile feature (`contracts/ipc.md` section 3, mock_planner_only).
        "mock_planner_only": !cfg!(feature = "dev-agent-bridge"),
        // The transport this core speaks over the sidecar stdio pipe.
        "transport_kind": "stdio",
        "version": VERSION,
        // Not stamped in this build; matches the recorded handshake fixture.
        "git_sha": "unknown"
    })
}

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    // Print the capability result object as pretty JSON on a clean stdout, so a
    // caller can pipe `operant capabilities` straight into the release gate.
    println!("{}", serde_json::to_string_pretty(&capabilities())?);
    Ok(())
}

fn print_help() {
    println!("operant capabilities");
    println!();
    println!("Print this build's capability blob (the get_capabilities handshake");
    println!("result, contracts/ipc.md section 3) as JSON to stdout, then exit 0.");
    println!();
    println!("The release gate (just check-release-artifact) reads real_uia and");
    println!("real_input from this output and refuses a mock artifact (either false).");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_blob_has_the_contract_shape() {
        let caps = capabilities();
        let obj = caps.as_object().expect("capabilities is a JSON object");
        // Every field the contract (section 3) fixes must be present.
        for field in [
            "real_uia",
            "real_input",
            "real_vision",
            "mock_planner_only",
            "transport_kind",
            "version",
            "git_sha",
        ] {
            assert!(obj.contains_key(field), "missing capability field `{field}`");
        }
        assert!(caps["real_uia"].is_boolean());
        assert!(caps["real_input"].is_boolean());
        assert_eq!(caps["transport_kind"], "stdio");
        assert_eq!(caps["version"], VERSION);
    }

    #[test]
    fn automation_booleans_follow_build_cfg() {
        // The booleans this build reports must equal its own compile features,
        // so the gate is reading the artifact, not a hardcoded answer. In the
        // default (mock) test build both are false, which is the blocked case.
        let caps = capabilities();
        assert_eq!(caps["real_uia"], cfg!(feature = "real-uia"));
        assert_eq!(caps["real_input"], cfg!(feature = "real-input"));
    }
}
