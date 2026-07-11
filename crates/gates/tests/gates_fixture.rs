//! Contract test: every gate in `contracts/fixtures/gates_basic.json` evaluates
//! without panicking, and the pre/post gates over the notepad snapshot fixture
//! return the expected pass/fail.

use operant_gates::{evaluate_gate, EvalContext};
use operant_ir::{Gate, GateResult, Role, Snapshot};
use serde_json::json;

const GATES_JSON: &str = include_str!("../../../contracts/fixtures/gates_basic.json");
const SNAPSHOT_JSON: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");

fn load_gates() -> Vec<Gate> {
    let doc: serde_json::Value = serde_json::from_str(GATES_JSON).unwrap();
    doc["gates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| serde_json::from_value::<Gate>(g.clone()).expect("gate parses"))
        .collect()
}

fn load_snapshot() -> Snapshot {
    serde_json::from_str(SNAPSHOT_JSON).expect("snapshot parses")
}

/// A unique, self-cleaned temp directory (no external crate).
struct TmpDir(std::path::PathBuf);
impl TmpDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!("operant-gates-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        TmpDir(p)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_gate_evaluates_without_panicking() {
    let gates = load_gates();
    let ctx = EvalContext::new().with_snapshot(load_snapshot());
    for (i, gate) in gates.iter().enumerate() {
        // Must not panic and must not surface a structural error: every shape in
        // the fixture is well-formed and therefore yields Pass or Fail.
        let r = evaluate_gate(gate, &ctx);
        assert!(r.is_ok(), "gate {i} raised a structural error: {r:?}");
    }
}

#[test]
fn pre_gates_pass_over_notepad_snapshot() {
    let gates = load_gates();
    let ctx = EvalContext::new().with_snapshot(load_snapshot());

    // gate 0: foreground process is notepad.exe.
    assert_eq!(evaluate_gate(&gates[0], &ctx).unwrap(), GateResult::Pass);
    // gate 1: the "Text editor" document element exists.
    assert_eq!(evaluate_gate(&gates[1], &ctx).unwrap(), GateResult::Pass);
    // gate 3: within_tolerance over count of listitems (there are none) == 0.
    assert_eq!(evaluate_gate(&gates[3], &ctx).unwrap(), GateResult::Pass);
    // gate 6: or-branch `true == true` keeps this green even with no adapter result.
    assert_eq!(evaluate_gate(&gates[6], &ctx).unwrap(), GateResult::Pass);
}

#[test]
fn post_gate_pivots_on_document_value() {
    let gates = load_gates();

    // Empty document (as captured): the invoice-text postcondition must FAIL.
    let empty = EvalContext::new().with_snapshot(load_snapshot());
    assert_eq!(evaluate_gate(&gates[2], &empty).unwrap(), GateResult::Fail);

    // Same gate PASSES once the document holds the expected invoice line and no
    // "Save As" window is open.
    let mut snap = load_snapshot();
    for el in snap.elements.iter_mut() {
        if el.role == Role::Document && el.name == "Text editor" {
            el.value = Some("Invoice 2026-07-11 total $142.50".to_string());
        }
    }
    let filled = EvalContext::new().with_snapshot(snap);
    assert_eq!(evaluate_gate(&gates[2], &filled).unwrap(), GateResult::Pass);
}

#[test]
fn post_gates_pass_against_a_successful_run() {
    let gates = load_gates();
    let tmp = TmpDir::new("post");
    let out = tmp.path().join("invoice.txt");
    std::fs::write(&out, b"Invoice 2026-07-11 total $142.50\n").unwrap();

    let mut snap = load_snapshot();
    for el in snap.elements.iter_mut() {
        if el.role == Role::Document && el.name == "Text editor" {
            el.value = Some("Invoice 2026-07-11 total $142.50".to_string());
        }
    }

    let ctx = EvalContext::new()
        .with_snapshot(snap)
        .with_input("output_path", &out.to_string_lossy())
        .with_adapter_result(
            "01JZFIXTURESTEP000000004",
            json!({ "exit_code": 0, "rows": [{ "amount": 100.0 }, { "amount": 42.5 }] }),
        );

    // gate 2: value matches the anchored invoice regex AND no Save As window.
    assert_eq!(evaluate_gate(&gates[2], &ctx).unwrap(), GateResult::Pass);
    // gate 4: the output file exists with size >= 1.
    assert_eq!(evaluate_gate(&gates[4], &ctx).unwrap(), GateResult::Pass);
    // gate 5: adapter exit_code == 0.
    assert_eq!(evaluate_gate(&gates[5], &ctx).unwrap(), GateResult::Pass);
    // gate 6: sum(rows[].amount) == 142.5 (first or-branch now also holds).
    assert_eq!(evaluate_gate(&gates[6], &ctx).unwrap(), GateResult::Pass);
}

#[test]
fn missing_file_fails_fs_gate_cleanly() {
    let gates = load_gates();
    // No `output_path` input: the `{output_path}` template stays unresolved and
    // names a nonexistent path. The gate must FAIL, not error.
    let ctx = EvalContext::new().with_snapshot(load_snapshot());
    assert_eq!(evaluate_gate(&gates[4], &ctx).unwrap(), GateResult::Fail);
}
