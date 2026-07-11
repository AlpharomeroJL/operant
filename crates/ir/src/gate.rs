//! Gate expression AST. Mirrors the predicate language in `contracts/gates`.
//! Deliberately data, never strings-of-code.

use serde::{Deserialize, Serialize};

/// A gate binding: which step, which phase, the predicate, and the failure policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    #[serde(default)]
    pub step_ref: Option<String>,
    pub kind: GateKind,
    pub expr: serde_json::Value,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GateKind {
    Pre,
    Post,
    Safety,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnFail {
    #[default]
    Halt,
    RegroundOnce,
}

/// Result of evaluating a gate predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GateResult {
    Pass,
    Fail,
}

/// The operators the engine must implement (parsed from `expr.op`).
/// This is the authoritative operator list; `crates/gates` matches on it.
pub const GATE_OPS: &[&str] = &[
    "exists",
    "equals",
    "matches",
    "count",
    "sum",
    "within_tolerance",
    "and",
    "or",
    "not",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_fixture_parses() {
        let raw = include_str!("../../../contracts/fixtures/gates_basic.json");
        let doc: serde_json::Value = serde_json::from_str(raw).unwrap();
        let gates = doc["gates"].as_array().unwrap();
        assert!(!gates.is_empty());
        for g in gates {
            let gate: Gate = serde_json::from_value(g.clone()).expect("gate parses");
            let op = gate.expr["op"].as_str().unwrap();
            assert!(GATE_OPS.contains(&op), "unknown op {op}");
        }
    }
}
