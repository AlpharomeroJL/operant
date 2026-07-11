//! [`FixturePerceiver`]: loads `Snapshot`s straight from JSON and answers
//! `Perceiver` calls deterministically off that fixed data, no OS calls
//! involved. Always built (no `windows` dependency) so every lane -- and
//! `cargo test -p operant-perception-uia` -- gets a working `Perceiver`
//! headless. Selector resolution is shared with the real UIA backend
//! through `resolve.rs`; this module only owns loading and picking which
//! snapshot in a process's timeline is "current".

use std::collections::HashMap;
use std::path::Path;

use operant_core::perceive::{Perceiver, PerceptionError, Resolved};
use operant_ir::snapshot::Snapshot;
use operant_ir::Selector;

use crate::resolve::resolve_in_snapshot;

/// A Perceiver backed by static, hand-authored (or recorded) snapshots.
/// Each window process gets an ordered timeline of states (e.g. a drift
/// `before.json` / `after.json` pair); `snapshot` always answers with the
/// latest entry, i.e. "the current live state". `wait_until_changed`
/// compares that same current state against the caller-supplied
/// `prev_digest`: the trait only ever hands this Perceiver an opaque
/// digest the caller last observed, not a point in history to resume
/// from, so "changed" can only ever mean "differs from current" -- same
/// as the real UIA backend polling a live window. Fully immutable once
/// built, so it needs no locking to be `Send + Sync`.
#[derive(Debug, Clone, Default)]
pub struct FixturePerceiver {
    timelines: HashMap<String, Vec<Snapshot>>,
}

impl FixturePerceiver {
    /// Build from already-parsed snapshots, grouped into per-process
    /// timelines by `window.process`, preserving arrival order within a
    /// process.
    pub fn new(snapshots: impl IntoIterator<Item = Snapshot>) -> Self {
        let mut timelines: HashMap<String, Vec<Snapshot>> = HashMap::new();
        for snap in snapshots {
            timelines
                .entry(snap.window.process.clone())
                .or_default()
                .push(snap);
        }
        FixturePerceiver { timelines }
    }

    /// Convenience for the common case of a single fixed snapshot.
    pub fn single(snapshot: Snapshot) -> Self {
        Self::new([snapshot])
    }

    /// Parse one JSON fixture (e.g. via `include_str!`).
    pub fn from_json(json: &str) -> Result<Self, PerceptionError> {
        let snap: Snapshot = serde_json::from_str(json)
            .map_err(|e| PerceptionError::Backend(format!("fixture JSON: {e}")))?;
        Ok(Self::single(snap))
    }

    /// Load one JSON fixture file from disk.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self, PerceptionError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|e| {
            PerceptionError::Backend(format!("reading fixture {}: {e}", path.display()))
        })?;
        Self::from_json(&raw)
    }

    /// Load several JSON fixture files as ordered timelines, grouped by
    /// each snapshot's `window.process` (e.g. a drift `before.json` /
    /// `after.json` pair for the same process).
    pub fn load_files(
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, PerceptionError> {
        let mut snapshots = Vec::new();
        for path in paths {
            let path = path.as_ref();
            let raw = std::fs::read_to_string(path).map_err(|e| {
                PerceptionError::Backend(format!("reading fixture {}: {e}", path.display()))
            })?;
            let snap: Snapshot = serde_json::from_str(&raw).map_err(|e| {
                PerceptionError::Backend(format!("fixture JSON {}: {e}", path.display()))
            })?;
            snapshots.push(snap);
        }
        Ok(Self::new(snapshots))
    }
}

impl Perceiver for FixturePerceiver {
    fn snapshot(&self, window_process: &str) -> Result<Snapshot, PerceptionError> {
        self.timelines
            .get(window_process)
            .and_then(|t| t.last())
            .cloned()
            .ok_or_else(|| PerceptionError::WindowNotFound(window_process.to_string()))
    }

    fn resolve(
        &self,
        snapshot: &Snapshot,
        selectors: &[Selector],
    ) -> Result<Resolved, PerceptionError> {
        resolve_in_snapshot(snapshot, selectors)
    }

    fn wait_until_changed(
        &self,
        window_process: &str,
        prev_digest: &str,
        timeout_ms: u64,
    ) -> Result<Snapshot, PerceptionError> {
        // Only ever compare against the current (latest) state: the trait
        // hands us an opaque digest the caller last observed, not a point
        // in history to resume scanning from, so scanning the whole
        // timeline for "any entry that differs" would wrongly match an
        // earlier state when queried with a later one's digest.
        let current = self.snapshot(window_process)?;
        if current.digest != prev_digest {
            Ok(current)
        } else {
            Err(PerceptionError::Timeout(timeout_ms))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::snapshot::Role;

    const NOTEPAD: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");

    fn fixtures_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/fixtures")
    }

    #[test]
    fn snapshot_fixture_parses() {
        let fx = FixturePerceiver::from_json(NOTEPAD).expect("fixture parses");
        let snap = fx.snapshot("notepad.exe").expect("snapshot present");
        assert_eq!(snap.window.process, "notepad.exe");
        assert_eq!(snap.elements.len(), 7);
        let editor = snap
            .find(Role::Document, "Text editor")
            .expect("editor element present");
        assert_eq!(editor.automation_id.as_deref(), Some("RichEditD2DPT"));
    }

    #[test]
    fn snapshot_unknown_process_is_window_not_found() {
        let fx = FixturePerceiver::from_json(NOTEPAD).unwrap();
        let err = fx.snapshot("nonexistent.exe").unwrap_err();
        assert!(matches!(err, PerceptionError::WindowNotFound(_)));
    }

    #[test]
    fn resolve_delegates_to_shared_selector_resolution() {
        let fx = FixturePerceiver::from_json(NOTEPAD).unwrap();
        let snap = fx.snapshot("notepad.exe").unwrap();
        let selectors = vec![
            Selector::AutomationId {
                value: "does-not-exist".into(),
            },
            Selector::AutomationId {
                value: "RichEditD2DPT".into(),
            },
        ];
        let resolved = fx
            .resolve(&snap, &selectors)
            .expect("falls through to the matching selector");
        let editor = snap.find(Role::Document, "Text editor").unwrap();
        let b = editor.bounds.as_ref().unwrap();
        assert_eq!(resolved.x, b.x + b.w / 2.0);
        assert_eq!(resolved.y, b.y + b.h / 2.0);
    }

    #[test]
    fn load_file_and_load_files_read_real_fixtures_from_disk() {
        let dir = fixtures_dir();
        let fx = FixturePerceiver::load_file(dir.join("snapshot_notepad.json")).unwrap();
        assert!(fx.snapshot("notepad.exe").is_ok());

        let drift_dir = dir.join("drift_renamed_button");
        let fx2 = FixturePerceiver::load_files([
            drift_dir.join("before.json"),
            drift_dir.join("after.json"),
        ])
        .unwrap();
        let latest = fx2.snapshot("fixture-webapp").unwrap();
        assert_eq!(
            latest.digest,
            "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1"
        );
    }

    #[test]
    fn wait_until_changed_returns_current_state_when_it_differs_from_prev_digest() {
        let dir = fixtures_dir().join("drift_renamed_button");
        let fx = FixturePerceiver::load_files([dir.join("before.json"), dir.join("after.json")])
            .unwrap();

        let before_digest = "b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0";
        let after_digest = "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1";

        // Current state is `after` (the last entry in the timeline); a
        // caller whose last-known digest is `before`'s sees it as changed.
        let changed = fx
            .wait_until_changed("fixture-webapp", before_digest, 1_000)
            .unwrap();
        assert_eq!(changed.digest, after_digest);

        // Nothing later in the timeline differs from `after`'s own digest.
        let err = fx
            .wait_until_changed("fixture-webapp", after_digest, 50)
            .unwrap_err();
        assert!(matches!(err, PerceptionError::Timeout(50)));
    }

    #[test]
    fn wait_until_changed_unknown_process_is_window_not_found() {
        let fx = FixturePerceiver::from_json(NOTEPAD).unwrap();
        let err = fx
            .wait_until_changed("nonexistent.exe", "whatever", 10)
            .unwrap_err();
        assert!(matches!(err, PerceptionError::WindowNotFound(_)));
    }
}
