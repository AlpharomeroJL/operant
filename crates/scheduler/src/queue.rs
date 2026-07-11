//! Run queue with capability scope checking for parallelism.

use std::collections::BTreeMap;

use operant_core::bus::events::RunMode;
use operant_ir::Manifest;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error type for queue operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QueueError {
    #[error("run already exists in queue")]
    DuplicateRunId,
}

/// A scheduled run with its manifest and inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledRun {
    pub run_id: String,
    pub goal: String,
    pub mode: RunMode,
    pub manifest: Manifest,
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// Time the run was enqueued (ms since epoch)
    pub enqueued_at: u64,
}

/// Serialized run queue with capability scope checking.
/// Two runs may parallelize only if their capability scopes are disjoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunQueue {
    pub runs: Vec<ScheduledRun>,
}

impl RunQueue {
    /// Create a new empty queue.
    pub fn new() -> Self {
        Self { runs: Vec::new() }
    }

    /// Add a run to the queue.
    pub fn add_run(&mut self, run: ScheduledRun) -> Result<(), QueueError> {
        // Check for duplicate run IDs
        if self.runs.iter().any(|r| r.run_id == run.run_id) {
            return Err(QueueError::DuplicateRunId);
        }

        self.runs.push(run);
        Ok(())
    }

    /// Check if two runs can execute in parallel given their capability scopes.
    /// Returns true if scopes are disjoint; false if they would conflict.
    pub fn can_parallelize(&self, run_a: &ScheduledRun, run_b: &ScheduledRun) -> bool {
        let cap_a = &run_a.manifest.capabilities;
        let cap_b = &run_b.manifest.capabilities;

        // If either declares network, the other cannot
        if cap_a.network && cap_b.network {
            return false;
        }

        // Check app set intersection: must be disjoint
        if has_intersection(&cap_a.apps, &cap_b.apps) {
            return false;
        }

        // Check path intersection: must be disjoint
        if has_path_intersection(&cap_a.paths, &cap_b.paths) {
            return false;
        }

        true
    }

    /// Get all runs that can execute in parallel with the given run.
    /// Returns the indices of compatible runs.
    pub fn parallel_compatible(&self, run_index: usize) -> Vec<usize> {
        if run_index >= self.runs.len() {
            return Vec::new();
        }

        let run = &self.runs[run_index];
        self.runs
            .iter()
            .enumerate()
            .filter_map(|(i, other)| {
                if i == run_index {
                    return None;
                }
                if self.can_parallelize(run, other) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Default for RunQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if two string sets have any intersection.
fn has_intersection(a: &[String], b: &[String]) -> bool {
    a.iter().any(|item| b.contains(item))
}

/// Check if two path sets have any intersection or containment.
/// Paths are considered intersecting if one is a prefix of the other or they're identical.
fn has_path_intersection(paths_a: &[String], paths_b: &[String]) -> bool {
    for path_a in paths_a {
        for path_b in paths_b {
            if paths_overlap(path_a, path_b) {
                return true;
            }
        }
    }
    false
}

/// Check if two paths overlap (one contains the other or they're identical).
fn paths_overlap(path_a: &str, path_b: &str) -> bool {
    // Normalize paths by removing trailing slashes
    let a = path_a.trim_end_matches(['/', '\\']);
    let b = path_b.trim_end_matches(['/', '\\']);

    // Exact match
    if a == b {
        return true;
    }

    // Check containment: one is a prefix of the other
    let a_with_sep = format!("{}/", a);
    let b_with_sep = format!("{}/", b);

    b.starts_with(&a_with_sep) || a.starts_with(&b_with_sep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::{Capabilities, RiskClass};

    fn make_manifest(apps: Vec<String>, paths: Vec<String>, network: bool) -> Manifest {
        Manifest {
            v: 1,
            name: "test".into(),
            version: "1.0".into(),
            description: "test".into(),
            step_summary: vec![],
            inputs_schema: serde_json::json!({}),
            capabilities: Capabilities {
                apps,
                paths,
                network,
                risk_ceiling: RiskClass::Write,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "test.dsl".into(),
                hash: "abc".into(),
            },
            signature: None,
        }
    }

    #[test]
    fn queue_duplicate_run_id() {
        let mut queue = RunQueue::new();

        let run1 = ScheduledRun {
            run_id: "run1".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec![], false),
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run2 = ScheduledRun {
            run_id: "run1".into(),
            goal: "test2".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec![], false),
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        queue.add_run(run1).expect("first run");
        let result = queue.add_run(run2);
        assert!(matches!(result, Err(QueueError::DuplicateRunId)));
    }

    #[test]
    fn disjoint_apps_can_parallelize() {
        let queue = RunQueue::new();

        let run_a = ScheduledRun {
            run_id: "a".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec!["app1.exe".into()], vec![], false),
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run_b = ScheduledRun {
            run_id: "b".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec!["app2.exe".into()], vec![], false),
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        assert!(queue.can_parallelize(&run_a, &run_b));
    }

    #[test]
    fn same_app_blocks_parallelization() {
        let queue = RunQueue::new();

        let run_a = ScheduledRun {
            run_id: "a".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec!["app.exe".into()], vec![], false),
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run_b = ScheduledRun {
            run_id: "b".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec!["app.exe".into()], vec![], false),
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        assert!(!queue.can_parallelize(&run_a, &run_b));
    }

    #[test]
    fn overlapping_paths_block_parallelization() {
        let queue = RunQueue::new();

        let run_a = ScheduledRun {
            run_id: "a".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec!["C:/workspace".into()], false),
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run_b = ScheduledRun {
            run_id: "b".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(
                vec![],
                vec!["C:/workspace/subdir".into()],
                false,
            ),
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        assert!(!queue.can_parallelize(&run_a, &run_b));
    }

    #[test]
    fn disjoint_paths_can_parallelize() {
        let queue = RunQueue::new();

        let run_a = ScheduledRun {
            run_id: "a".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec!["C:/proj1".into()], false),
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run_b = ScheduledRun {
            run_id: "b".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec!["C:/proj2".into()], false),
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        assert!(queue.can_parallelize(&run_a, &run_b));
    }

    #[test]
    fn network_blocks_parallelization() {
        let queue = RunQueue::new();

        let run_a = ScheduledRun {
            run_id: "a".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec![], true),
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run_b = ScheduledRun {
            run_id: "b".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec![], true),
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        assert!(!queue.can_parallelize(&run_a, &run_b));
    }

    #[test]
    fn one_network_doesnt_block_non_network() {
        let queue = RunQueue::new();

        let run_a = ScheduledRun {
            run_id: "a".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec![], true),
            inputs: BTreeMap::new(),
            enqueued_at: 1000,
        };

        let run_b = ScheduledRun {
            run_id: "b".into(),
            goal: "test".into(),
            mode: RunMode::Replay,
            manifest: make_manifest(vec![], vec![], false),
            inputs: BTreeMap::new(),
            enqueued_at: 2000,
        };

        assert!(queue.can_parallelize(&run_a, &run_b));
    }

    #[test]
    fn path_overlap_detection() {
        assert!(paths_overlap("C:/workspace", "C:/workspace/subdir"));
        assert!(paths_overlap("C:/workspace/subdir", "C:/workspace"));
        assert!(paths_overlap("C:/workspace", "C:/workspace"));
        assert!(!paths_overlap("C:/proj1", "C:/proj2"));
        assert!(!paths_overlap("C:/workspace", "C:/work/space"));
    }

    #[test]
    fn path_normalization() {
        assert!(paths_overlap("C:/workspace/", "C:/workspace"));
        assert!(paths_overlap("C:/workspace", "C:/workspace/"));
        assert!(paths_overlap("C:/workspace/", "C:/workspace/subdir/"));
    }
}
