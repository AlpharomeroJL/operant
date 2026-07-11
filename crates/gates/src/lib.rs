//! Invariant gate engine (C9).
//!
//! Evaluates the JSON predicate AST from `contracts/gates` (mirrored by
//! [`operant_ir::Gate`]) against a perception snapshot, the filesystem, and
//! adapter results. The language is data, never strings-of-code.
//!
//! # Operators
//! `exists`, `equals`, `matches` (anchored regex), `count`, `sum`,
//! `within_tolerance`, `and`, `or`, `not`.
//!
//! # Query kinds
//! - Snapshot: `snapshot_window_process`, `snapshot_element` (role + name, `*`
//!   wildcard), `snapshot_element_value`.
//! - Filesystem: `fs` (`path` with `{template}` resolution, `min_size`, `hash`).
//! - Adapter results: `adapter_result` (`step_ref` + dotted `field` path with
//!   `[]` array projection).
//! - Inline: `literal`; `count`/`sum` may also stand in a value position.
//!
//! ```
//! use operant_gates::{evaluate_gate, EvalContext};
//! use operant_ir::{Gate, GateResult};
//!
//! let gate: Gate = serde_json::from_value(serde_json::json!({
//!     "kind": "pre",
//!     "expr": {
//!         "op": "equals",
//!         "left": { "kind": "snapshot_window_process" },
//!         "right": { "kind": "literal", "value": "notepad.exe" }
//!     }
//! })).unwrap();
//!
//! let snap: operant_ir::Snapshot = serde_json::from_str(
//!     include_str!("../../../contracts/fixtures/snapshot_notepad.json")
//! ).unwrap();
//! let ctx = EvalContext::new().with_snapshot(snap);
//! assert_eq!(evaluate_gate(&gate, &ctx).unwrap(), GateResult::Pass);
//! ```

mod context;
mod error;
mod eval;
mod value;

pub use context::EvalContext;
pub use error::GateError;
pub use eval::{eval, evaluate_gate, evaluate_gates};
pub use value::{val_equals, Val};

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-gates";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-gates");
    }
}
