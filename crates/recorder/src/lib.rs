//! Trajectory recorder (C7): SQLite (WAL) store plus a content-addressed blob store.
//!
//! Records the Action IR, snapshot digests, grounding decision, timing, outcome, and
//! any human correction for every step of a run. This is both the compiler's input
//! (see `contracts/fixtures/trajectory_notepad.json`, the frozen canonical trajectory)
//! and the audit substrate (the hash-chained `audit` table).
//!
//! Crash safety: [`Recorder::record_step`] is one committed transaction per step on a
//! WAL-mode connection, so a database reopened after an unclean shutdown contains
//! exactly its committed prefix, never a torn write (see `tests.rs` for the
//! kill-mid-write test).
//!
//! ```
//! use operant_recorder::{NewStep, Recorder, RunMode, RunStatus};
//! use operant_ir::{Action, ActionKind, Grounding, RiskClass};
//!
//! let rec = Recorder::open_in_memory().unwrap();
//! let run_id = rec.start_run("write an invoice note", RunMode::Explore, None).unwrap();
//!
//! let action = Action {
//!     v: 1,
//!     id: "s1".into(),
//!     kind: ActionKind::Key,
//!     intent: Some("save the file".into()),
//!     target: None,
//!     params: {
//!         let mut m = serde_json::Map::new();
//!         m.insert("combo".into(), serde_json::json!("ctrl+s"));
//!         m
//!     },
//!     pace: Default::default(),
//!     risk_class: RiskClass::Write,
//!     irreversible: false,
//!     grounding: Grounding::Uia,
//!     timeout_ms: 5000,
//!     retry: Default::default(),
//! };
//! rec.record_step(&run_id, NewStep::new(1, action, Grounding::Uia, "ok", 310)).unwrap();
//! rec.end_run(&run_id, RunStatus::Completed).unwrap();
//!
//! assert_eq!(rec.list_steps(&run_id).unwrap().len(), 1);
//! ```

mod blobs;
pub mod backup;
mod error;
mod ids;
mod misc;
mod runs;
mod schema;
mod steps;
mod store;
pub mod undo;

#[cfg(test)]
mod tests;

pub use blobs::ArtifactRecord;
pub use error::{RecorderError, Result};
pub use misc::{AuditRecord, GateRecord, MetricsRecord, UndoEntry, WorkflowRecord, WorkflowVersionRecord};
pub use runs::{RunMode, RunRecord, RunStatus};
pub use steps::{NewStep, StepRecord};
pub use store::Recorder;

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-recorder";

#[cfg(test)]
mod crate_marker_tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-recorder");
        let _ = "recorder";
    }
}
