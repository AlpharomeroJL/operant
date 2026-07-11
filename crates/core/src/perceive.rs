//! The OS-agnostic `Perceiver` trait (C2). UIA, browser, and stub backends
//! implement it. Defined in core so every consumer depends on the trait, not a
//! platform crate.

use operant_ir::snapshot::Snapshot;
use operant_ir::Selector;

/// Typed perception errors. Elevated/secure-desktop windows return
/// [`PerceptionError::Denied`], never an empty tree.
#[derive(Debug, thiserror::Error)]
pub enum PerceptionError {
    #[error("perception denied: {0}")]
    Denied(String),
    #[error("target window not found: {0}")]
    WindowNotFound(String),
    #[error("selector did not resolve")]
    SelectorMiss,
    #[error("timed out after {0} ms")]
    Timeout(u64),
    #[error("backend error: {0}")]
    Backend(String),
}

/// A resolved element location for the action layer to act on.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub x: f64,
    pub y: f64,
    pub monitor: Option<String>,
}

/// Snapshot, diff, resolve, wait. Implementations live in perception crates.
pub trait Perceiver: Send + Sync {
    /// Full normalized snapshot of the target window subtree.
    fn snapshot(&self, window_process: &str) -> Result<Snapshot, PerceptionError>;

    /// Resolve a selector chain to a fresh clickable point at execution time.
    fn resolve(
        &self,
        snapshot: &Snapshot,
        selectors: &[Selector],
    ) -> Result<Resolved, PerceptionError>;

    /// Block until the scope's digest changes or `timeout_ms` elapses.
    fn wait_until_changed(
        &self,
        window_process: &str,
        prev_digest: &str,
        timeout_ms: u64,
    ) -> Result<Snapshot, PerceptionError>;
}

/// A keyed structural diff between two snapshots.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SnapshotDiff {
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
    pub renamed: Vec<(u32, String)>,
    pub value_changed: Vec<(u32, String)>,
}

impl SnapshotDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.renamed.is_empty()
            && self.value_changed.is_empty()
    }
}
