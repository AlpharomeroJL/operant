//! Scheduler and triggers (C11): cron, file-watch, window-appears, email-arrives. Serialized queue; unattended triggers launch compiled (replay) workflows only, enforced in code. Depends on replay, never on orchestrator. L10A owns it.
//!
//! # Triggers
//!
//! Each trigger is a struct with testable `matches` and `next` methods, modeling
//! the trigger logic without requiring real OS events or filesystem watches.
//!
//! - `CronTrigger`: 5-field cron expression, computes next occurrence with DST safety.
//! - `FileWatchTrigger`: directory + glob pattern, debounce window, emits matched path as workflow input.
//! - `WindowAppearsTrigger`: process name + title regex, poll-based.
//! - `EmailArrivesTrigger`: subject/from filters, message ID as input.
//!
//! # Run Queue
//!
//! Serialized by default. Two runs parallelize only if their capability scopes
//! are disjoint: app sets and directory subtrees do not intersect, and neither
//! declares network capability if the other does.
//!
//! # Unattended Enforcement
//!
//! [`enqueue`] refuses any run not in replay mode, returning a typed error.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use operant_core::bus::events::RunMode;
use operant_ir::Manifest;
use thiserror::Error;

pub mod trigger;
pub mod queue;

pub use trigger::{CronTrigger, EmailArrivesTrigger, FileWatchTrigger, WindowAppearsTrigger};
pub use queue::{RunQueue, ScheduledRun, QueueError};

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-scheduler";

/// Error type for scheduler operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SchedulerError {
    #[error("unattended mode requires replay, got {mode:?}")]
    UnattendedNonReplay { mode: String },

    #[error("capability scope conflict: run would block other queued runs")]
    ScopeConflict,

    #[error(transparent)]
    Queue(#[from] QueueError),
}

/// Enqueue a run for execution. Fails if mode is not replay (unattended enforcement).
pub fn enqueue(
    queue: &mut RunQueue,
    run_id: String,
    goal: String,
    mode: RunMode,
    manifest: Manifest,
    inputs: BTreeMap<String, serde_json::Value>,
) -> Result<(), SchedulerError> {
    // Unattended enforcement: only replay mode is allowed
    if mode != RunMode::Replay {
        return Err(SchedulerError::UnattendedNonReplay {
            mode: format!("{:?}", mode).to_lowercase(),
        });
    }

    let scheduled_run = ScheduledRun {
        run_id,
        goal,
        mode,
        manifest,
        inputs,
        enqueued_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };

    queue.add_run(scheduled_run).map_err(SchedulerError::Queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::{Capabilities, RiskClass};

    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-scheduler");
        let _ = "scheduler";
    }

    #[test]
    fn unattended_rejects_non_replay_mode() {
        let mut queue = RunQueue::new();
        let manifest = Manifest {
            v: 1,
            name: "test".into(),
            version: "1.0".into(),
            description: "test".into(),
            step_summary: vec![],
            inputs_schema: serde_json::json!({}),
            capabilities: Capabilities {
                apps: vec![],
                paths: vec![],
                network: false,
                risk_ceiling: RiskClass::Read,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "test.dsl".into(),
                hash: "abc123".into(),
            },
            signature: None,
        };

        let result = enqueue(
            &mut queue,
            "run1".into(),
            "test goal".into(),
            RunMode::Explore,
            manifest.clone(),
            BTreeMap::new(),
        );

        assert!(matches!(result, Err(SchedulerError::UnattendedNonReplay { .. })));

        // Replay mode should succeed
        let result = enqueue(
            &mut queue,
            "run2".into(),
            "test goal".into(),
            RunMode::Replay,
            manifest,
            BTreeMap::new(),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn disjoint_scope_parallelism_allowed() {
        let mut queue = RunQueue::new();

        let manifest1 = Manifest {
            v: 1,
            name: "app1_workflow".into(),
            version: "1.0".into(),
            description: "test".into(),
            step_summary: vec![],
            inputs_schema: serde_json::json!({}),
            capabilities: Capabilities {
                apps: vec!["app1.exe".into()],
                paths: vec!["C:/workspace/project1".into()],
                network: false,
                risk_ceiling: RiskClass::Write,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "test.dsl".into(),
                hash: "abc123".into(),
            },
            signature: None,
        };

        let manifest2 = Manifest {
            v: 1,
            name: "app2_workflow".into(),
            version: "1.0".into(),
            description: "test".into(),
            step_summary: vec![],
            inputs_schema: serde_json::json!({}),
            capabilities: Capabilities {
                apps: vec!["app2.exe".into()],
                paths: vec!["C:/workspace/project2".into()],
                network: false,
                risk_ceiling: RiskClass::Write,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "test.dsl".into(),
                hash: "abc123".into(),
            },
            signature: None,
        };

        let run1 = ScheduledRun {
            run_id: "run1".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: manifest1,
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run2 = ScheduledRun {
            run_id: "run2".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: manifest2,
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        queue.add_run(run1).expect("run1 added");
        let result = queue.can_parallelize(&queue.runs[0], &run2);
        assert!(result, "disjoint scopes should parallelize");
    }

    #[test]
    fn overlapping_scope_serialized() {
        let mut queue = RunQueue::new();

        let manifest1 = Manifest {
            v: 1,
            name: "workflow".into(),
            version: "1.0".into(),
            description: "test".into(),
            step_summary: vec![],
            inputs_schema: serde_json::json!({}),
            capabilities: Capabilities {
                apps: vec!["app.exe".into()],
                paths: vec!["C:/workspace".into()],
                network: false,
                risk_ceiling: RiskClass::Write,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "test.dsl".into(),
                hash: "abc123".into(),
            },
            signature: None,
        };

        let manifest2 = Manifest {
            v: 1,
            name: "workflow2".into(),
            version: "1.0".into(),
            description: "test".into(),
            step_summary: vec![],
            inputs_schema: serde_json::json!({}),
            capabilities: Capabilities {
                apps: vec!["app.exe".into()],
                paths: vec!["C:/workspace/subdir".into()],
                network: false,
                risk_ceiling: RiskClass::Write,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "test.dsl".into(),
                hash: "abc123".into(),
            },
            signature: None,
        };

        let run1 = ScheduledRun {
            run_id: "run1".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: manifest1,
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run2 = ScheduledRun {
            run_id: "run2".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: manifest2,
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        queue.add_run(run1).expect("run1 added");
        let result = queue.can_parallelize(&queue.runs[0], &run2);
        assert!(!result, "overlapping scopes should serialize");
    }

    #[test]
    fn file_watch_event_produces_replay_run_with_path_input() {
        use crate::trigger::FileWatchTrigger;

        let mut queue = RunQueue::new();

        // Simulate a file-watch trigger for PDFs
        let trigger = FileWatchTrigger::new(
            "C:/incoming".into(),
            "*.pdf".into(),
            2000,
        );

        // Simulate a file-drop event
        let file_path = "C:/incoming/invoice.pdf";
        let now_ms = 5000;

        // Check if the event matches the trigger
        let matched_path = trigger.matches(file_path, now_ms);
        assert_eq!(matched_path, Some(file_path.into()));

        // Create a manifest for processing this file
        let mut inputs = BTreeMap::new();
        inputs.insert("file_path".into(), serde_json::json!(file_path));

        let manifest = Manifest {
            v: 1,
            name: "process-pdf".into(),
            version: "1.0".into(),
            description: "Process PDF files".into(),
            step_summary: vec!["Open PDF".into(), "Extract data".into()],
            inputs_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" }
                }
            }),
            capabilities: Capabilities {
                apps: vec!["acrobat.exe".into()],
                paths: vec!["C:/incoming".into()],
                network: false,
                risk_ceiling: RiskClass::Read,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "process.dsl".into(),
                hash: "def456".into(),
            },
            signature: None,
        };

        // Enqueue a replay run with the matched file path as input
        let result = enqueue(
            &mut queue,
            "run-pdf-001".into(),
            "Process incoming PDF".into(),
            RunMode::Replay,
            manifest,
            inputs.clone(),
        );

        assert!(result.is_ok());
        assert_eq!(queue.runs.len(), 1);

        let run = &queue.runs[0];
        assert_eq!(run.run_id, "run-pdf-001");
        assert_eq!(run.mode, RunMode::Replay);
        assert_eq!(
            run.inputs.get("file_path").and_then(|v| v.as_str()),
            Some(file_path)
        );
    }

    #[test]
    fn cron_next_occurrence_computes() {
        use crate::trigger::CronTrigger;

        // 5-field cron expression for 9 AM on weekdays
        let cron = CronTrigger::new("0 9 * * 1-5".into());

        // Compute next occurrence from a given time
        let base_time = 1000;
        let next = cron.next_occurrence(base_time);

        assert!(next.is_some());
        assert!(next.unwrap() > base_time);

        // Invalid cron should return None
        let invalid = CronTrigger::new("invalid expr with too many fields".into());
        assert!(invalid.next_occurrence(1000).is_none());
    }
}

