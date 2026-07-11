//! Manifest load guard.
//!
//! Safety gates are runtime-owned. A workflow manifest that declares a gate with
//! `kind: "safety"`, or that carries a directive attempting to disable the hard
//! invariants, FAILS TO LOAD with a typed [`SafetyError`]. This is the load-time
//! half of FR-S4: there is no in-band way for a workflow to opt out.

use operant_ir::{GateKind, Manifest};

use crate::error::SafetyError;

/// Directive keys that would try to switch safety off. Presence with a truthy
/// value is rejected.
const DISABLE_KEYS: &[&str] = &[
    "disable_safety",
    "safety_disabled",
    "disable_safety_gates",
    "disable_invariants",
    "allow_unsafe",
];

/// Parse and guard a workflow manifest from JSON text.
///
/// Returns the parsed [`Manifest`] only if it declares no safety gate and no
/// disable directive; otherwise a typed [`SafetyError`].
pub fn load_manifest(json: &str) -> Result<Manifest, SafetyError> {
    let raw: serde_json::Value = serde_json::from_str(json)?;

    // Reject explicit "disable safety" directives (these are not fields of the
    // typed manifest, so serde would otherwise silently drop them).
    for key in DISABLE_KEYS {
        if truthy(raw.get(*key)) {
            return Err(SafetyError::AttemptToDisableSafety);
        }
    }

    // Reject any gate declared with kind "safety" at the raw level (before the
    // typed parse, so a future manifest that also fails typed parsing still gets
    // the precise reason).
    if let Some(gates) = raw.get("gates").and_then(|g| g.as_array()) {
        for g in gates {
            if g.get("kind").and_then(|k| k.as_str()) == Some("safety") {
                return Err(SafetyError::SafetyGateInManifest);
            }
        }
    }

    let manifest: Manifest = serde_json::from_value(raw)?;

    // Belt and suspenders: also reject via the typed enum.
    if manifest.gates.iter().any(|g| g.kind == GateKind::Safety) {
        return Err(SafetyError::SafetyGateInManifest);
    }

    Ok(manifest)
}

/// Guard an already-parsed manifest (the typed half only).
pub fn guard_manifest(manifest: &Manifest) -> Result<(), SafetyError> {
    if manifest.gates.iter().any(|g| g.kind == GateKind::Safety) {
        return Err(SafetyError::SafetyGateInManifest);
    }
    Ok(())
}

fn truthy(v: Option<&serde_json::Value>) -> bool {
    match v {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => {
            matches!(s.to_lowercase().as_str(), "true" | "yes" | "1" | "on")
        }
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str =
        include_str!("../../../contracts/fixtures/workflow_notepad/manifest.json");

    #[test]
    fn good_manifest_loads() {
        let m = load_manifest(GOOD).expect("clean manifest loads");
        assert_eq!(m.name, "notepad-invoice-note");
        // The clean manifest declares only pre/post gates.
        assert!(m.gates.iter().all(|g| g.kind != GateKind::Safety));
    }

    #[test]
    fn manifest_with_safety_gate_fails_to_load() {
        // Take the good manifest and splice in a safety gate.
        let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
        v["gates"].as_array_mut().unwrap().push(serde_json::json!({
            "step_ref": null,
            "kind": "safety",
            "expr": { "op": "exists", "query": { "kind": "snapshot_element", "role": "button", "name": "*" } },
            "on_fail": "halt"
        }));
        let json = serde_json::to_string(&v).unwrap();
        assert!(matches!(
            load_manifest(&json),
            Err(SafetyError::SafetyGateInManifest)
        ));
    }

    #[test]
    fn manifest_that_tries_to_disable_safety_fails_to_load() {
        let mut v: serde_json::Value = serde_json::from_str(GOOD).unwrap();
        v["disable_safety"] = serde_json::json!(true);
        let json = serde_json::to_string(&v).unwrap();
        assert!(matches!(
            load_manifest(&json),
            Err(SafetyError::AttemptToDisableSafety)
        ));
    }
}
