//! A bundled post-execution snapshot for headless `run`/`dry-run` when the
//! caller passes no `--snapshot`.
//!
//! Headless replay (`crates/replay`'s whole point) has no live perceiver,
//! so something has to stand in as "the screen after the run" for the
//! compiled workflow's pre/post gates to evaluate against -- exactly the
//! role `e2e/golden-path/tests/golden_path.rs`'s own `notepad_snapshot()`
//! plays, and for the same reason. This is that snapshot for the CLI: the
//! shared `contracts/fixtures/snapshot_notepad.json` fixture (the state
//! *before* the run), with the "Text editor" element's value patched to
//! what a successful bundled Notepad invoice run leaves behind, matching
//! that fixture workflow's own postcondition regex
//! (`^Invoice \d{4}-\d{2}-\d{2} total \$\d+\.\d{2}$`). Only meaningful for
//! that one bundled workflow; any other compiled workflow's gates need a
//! real `--snapshot`.

use std::path::Path;

use anyhow::{Context, Result};
use operant_ir::{Role, Snapshot};

const RAW_SNAPSHOT: &str = include_str!("../../contracts/fixtures/snapshot_notepad.json");
const DEFAULT_INVOICE_TEXT: &str = "Invoice 2026-07-11 total $142.50";

pub fn bundled_notepad_snapshot() -> Snapshot {
    let mut snap: Snapshot =
        serde_json::from_str(RAW_SNAPSHOT).expect("bundled snapshot fixture parses");
    for el in &mut snap.elements {
        if el.role == Role::Document && el.name == "Text editor" {
            el.value = Some(DEFAULT_INVOICE_TEXT.to_string());
        }
    }
    snap
}

/// Load a caller-supplied snapshot JSON file for `--snapshot`.
pub fn load_snapshot(path: &Path) -> Result<Snapshot> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading snapshot {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing snapshot {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshot_satisfies_the_bundled_workflows_gates() {
        let snap = bundled_notepad_snapshot();
        assert_eq!(snap.window.process, "notepad.exe");
        let doc = snap
            .find(Role::Document, "Text editor")
            .expect("the fixture has a Text editor element");
        assert_eq!(doc.value.as_deref(), Some(DEFAULT_INVOICE_TEXT));
    }
}
