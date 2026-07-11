//! Dry-run interpreter.
//!
//! Renders a plan against a snapshot with ZERO side effects: it reads the action
//! IR and the snapshot and describes, step by step, what a real run *would* do.
//! It never touches the filesystem, the network, or the desktop. The invariant
//! tested elsewhere: a filesystem diff taken around a dry-run is empty.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use operant_ir::{Action, ActionKind, Snapshot};

/// A rendered dry-run: one human-readable line per step, plus the surface a real
/// run *would* have touched (for review), all computed without side effects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DryRunReport {
    /// One plain-language line per planned step.
    pub lines: Vec<String>,
    /// Filesystem paths the plan *would* have written, in plan order. Recorded,
    /// never touched.
    pub would_touch: Vec<PathBuf>,
}

/// Render a plan against a snapshot with no side effects.
pub fn dry_run(plan: &[Action], snapshot: &Snapshot) -> DryRunReport {
    let mut report = DryRunReport::default();
    for (i, action) in plan.iter().enumerate() {
        report.lines.push(render_step(i + 1, action, snapshot));
        if let Some(path) = touched_path(action) {
            report.would_touch.push(path);
        }
    }
    report
}

fn render_step(n: usize, action: &Action, snapshot: &Snapshot) -> String {
    let intent = action.intent.as_deref().unwrap_or("(no intent)");
    let target = describe_target(action, snapshot);
    let verb = match action.kind {
        ActionKind::Click => "click",
        ActionKind::Type => "type into",
        ActionKind::Key => "send keys to",
        ActionKind::Scroll => "scroll",
        ActionKind::Drag => "drag",
        ActionKind::Wait => "wait on",
        ActionKind::Assert => "assert against",
        ActionKind::AdapterCall => "call adapter for",
    };
    format!("step {n}: would {verb} {target} -- {intent} [{}]", risk_word(action))
}

fn risk_word(action: &Action) -> String {
    format!("{:?}", action.risk_class).to_lowercase()
}

fn describe_target(action: &Action, snapshot: &Snapshot) -> String {
    // Prefer a snapshot element the primary selector resolves to; fall back to
    // the raw selector or window description. Read-only throughout.
    if let Some(target) = &action.target {
        if let Some(name) = target.selectors.iter().find_map(|s| selector_name(s, snapshot)) {
            return format!("\"{name}\"");
        }
        if let Some(win) = &target.window {
            if let Some(p) = &win.process {
                return format!("the {p} window");
            }
        }
    }
    "the foreground window".to_string()
}

fn selector_name(selector: &operant_ir::Selector, snapshot: &Snapshot) -> Option<String> {
    use operant_ir::Selector;
    match selector {
        Selector::AutomationId { value } => snapshot
            .elements
            .iter()
            .find(|e| e.automation_id.as_deref() == Some(value))
            .map(|e| e.name.clone()),
        Selector::NameRolePath { path } => path.last().map(|seg| seg.name.clone()),
        _ => None,
    }
}

/// The filesystem path a real run of this action would write, if any. Derived
/// purely from the action IR; nothing is read from or written to disk.
fn touched_path(action: &Action) -> Option<PathBuf> {
    action.params.get("path").and_then(|v| v.as_str()).map(PathBuf::from)
}

/// A content fingerprint of a directory tree: relative path -> (size, BLAKE3 hex).
///
/// Used by tests to prove a dry-run is side-effect free (the fingerprint is
/// identical before and after). Deterministic (sorted) and recursive.
pub fn fs_fingerprint(root: &Path) -> BTreeMap<String, (u64, String)> {
    let mut map = BTreeMap::new();
    fingerprint_into(root, root, &mut map);
    map
}

fn fingerprint_into(root: &Path, dir: &Path, map: &mut BTreeMap<String, (u64, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            fingerprint_into(root, &path, map);
        } else if ft.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let hash = blake3::hash(&bytes).to_hex().to_string();
                map.insert(rel, (bytes.len() as u64, hash));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../contracts/fixtures/snapshot_notepad.json"
        ))
        .unwrap()
    }

    fn type_action() -> Action {
        serde_json::from_value(json!({
            "v": 1, "id": "s2", "kind": "type", "intent": "Type the invoice note",
            "target": {
                "window": { "process": "notepad.exe" },
                "selectors": [{ "kind": "automation_id", "value": "RichEditD2DPT" }]
            },
            "params": { "text": "Invoice 2026-07-11 total $142.50" },
            "risk_class": "write", "grounding": "uia"
        }))
        .unwrap()
    }

    #[test]
    fn renders_a_line_per_step_without_touching_disk() {
        let report = dry_run(&[type_action()], &snapshot());
        assert_eq!(report.lines.len(), 1);
        assert!(report.lines[0].contains("would type into"));
        // Resolved the automation-id selector to the element name.
        assert!(report.lines[0].contains("Text editor"));
    }
}
