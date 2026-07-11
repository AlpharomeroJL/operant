//! Selector-chain resolution: turn an ordered list of `Selector`
//! candidates into a fresh clickable point against a snapshot, trying
//! each in `docs/specs/perception.md`'s priority order: AutomationId,
//! then role+name path, then role+ordinal path. Shared by
//! `FixturePerceiver` and (behind `real-uia`) `UiaPerceiver`: resolution
//! is pure data over an already-captured `Snapshot`, so both backends get
//! identical semantics for free, and a caller gets a "fresh" point simply
//! by re-snapshotting before calling this.

use operant_core::perceive::{PerceptionError, Resolved};
use operant_ir::snapshot::{Element, Snapshot};
use operant_ir::{NameRoleSeg, OrdinalSeg, Selector};

use crate::topology::{find_unique, Topology};

/// Try `selectors` against `snapshot` in order; return the first
/// unambiguous match's clickable point (the center of its bounds).
/// `PerceptionError::SelectorMiss` if none resolve.
pub fn resolve_in_snapshot(
    snapshot: &Snapshot,
    selectors: &[Selector],
) -> Result<Resolved, PerceptionError> {
    let topo = Topology::build(&snapshot.elements);
    for selector in selectors {
        if let Some(element) = match_selector(snapshot, &topo, selector) {
            return resolved_point(element);
        }
    }
    Err(PerceptionError::SelectorMiss)
}

fn resolved_point(element: &Element) -> Result<Resolved, PerceptionError> {
    let bounds = element.bounds.as_ref().ok_or_else(|| {
        PerceptionError::Backend(format!(
            "element idx={} (\"{}\") matched but carries no bounds to click",
            element.idx, element.name
        ))
    })?;
    Ok(Resolved {
        x: bounds.x + bounds.w / 2.0,
        y: bounds.y + bounds.h / 2.0,
        monitor: bounds.monitor.clone(),
    })
}

fn match_selector<'a>(
    snapshot: &'a Snapshot,
    topo: &Topology<'a>,
    selector: &Selector,
) -> Option<&'a Element> {
    match selector {
        Selector::AutomationId { value } => {
            if value.is_empty() {
                None
            } else {
                find_unique(&snapshot.elements, |e| {
                    e.automation_id.as_deref() == Some(value.as_str())
                })
            }
        }
        Selector::NameRolePath { path } => find_unique(&snapshot.elements, |e| {
            names_match(&topo.name_role_path(e.idx), path)
        }),
        Selector::OrdinalPath { path } => find_unique(&snapshot.elements, |e| {
            ordinals_match(&topo.ordinal_path(&snapshot.elements, e.idx), path)
        }),
        // UIA has no notion of CSS; this exists so `FixturePerceiver` can
        // also resolve browser/webapp fixtures. Only the common `#id`
        // shorthand is understood, matched against `automation_id`
        // (fixtures mint css ids and automation ids from the same source
        // attribute; see `contracts/fixtures/drift_renamed_button`).
        Selector::Css { value } => value
            .strip_prefix('#')
            .filter(|id| !id.is_empty())
            .and_then(|id| {
                find_unique(&snapshot.elements, |e| {
                    e.automation_id.as_deref() == Some(id)
                })
            }),
    }
}

fn names_match(candidate: &[(String, String)], path: &[NameRoleSeg]) -> bool {
    candidate.len() == path.len()
        && candidate
            .iter()
            .zip(path)
            .all(|((role, name), seg)| *role == seg.role && *name == seg.name)
}

fn ordinals_match(candidate: &[(String, u32)], path: &[OrdinalSeg]) -> bool {
    candidate.len() == path.len()
        && candidate
            .iter()
            .zip(path)
            .all(|((role, ordinal), seg)| *role == seg.role && *ordinal == seg.ordinal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::snapshot::Role;

    fn notepad() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../contracts/fixtures/snapshot_notepad.json"
        ))
        .unwrap()
    }

    #[test]
    fn resolves_by_automation_id() {
        let snap = notepad();
        let resolved = resolve_in_snapshot(
            &snap,
            &[Selector::AutomationId {
                value: "RichEditD2DPT".into(),
            }],
        )
        .unwrap();
        let editor = snap.find(Role::Document, "Text editor").unwrap();
        let b = editor.bounds.as_ref().unwrap();
        assert_eq!(
            resolved,
            Resolved {
                x: b.x + b.w / 2.0,
                y: b.y + b.h / 2.0,
                monitor: b.monitor.clone()
            }
        );
    }

    #[test]
    fn resolves_by_name_role_path() {
        let snap = notepad();
        let selectors = vec![Selector::NameRolePath {
            path: vec![
                NameRoleSeg {
                    role: "window".into(),
                    name: "Untitled - Notepad".into(),
                },
                NameRoleSeg {
                    role: "document".into(),
                    name: "Text editor".into(),
                },
            ],
        }];
        let resolved = resolve_in_snapshot(&snap, &selectors).unwrap();
        let editor = snap.find(Role::Document, "Text editor").unwrap();
        let b = editor.bounds.as_ref().unwrap();
        assert_eq!(resolved.x, b.x + b.w / 2.0);
        assert_eq!(resolved.y, b.y + b.h / 2.0);
    }

    #[test]
    fn resolves_by_ordinal_path() {
        let snap = notepad();
        // Second menuitem ("Edit") under the menubar under the window.
        let selectors = vec![Selector::OrdinalPath {
            path: vec![
                OrdinalSeg {
                    role: "window".into(),
                    ordinal: 0,
                },
                OrdinalSeg {
                    role: "menubar".into(),
                    ordinal: 0,
                },
                OrdinalSeg {
                    role: "menuitem".into(),
                    ordinal: 1,
                },
            ],
        }];
        let resolved = resolve_in_snapshot(&snap, &selectors).unwrap();
        let edit = snap.find(Role::Menuitem, "Edit").unwrap();
        let b = edit.bounds.as_ref().unwrap();
        assert_eq!(resolved.x, b.x + b.w / 2.0);
        assert_eq!(resolved.y, b.y + b.h / 2.0);
    }

    #[test]
    fn falls_through_to_the_next_selector_when_the_first_misses() {
        let snap = notepad();
        let selectors = vec![
            Selector::AutomationId {
                value: "does-not-exist".into(),
            },
            Selector::AutomationId {
                value: "StatusBar".into(),
            },
        ];
        let resolved = resolve_in_snapshot(&snap, &selectors).unwrap();
        let status = snap.find(Role::Statusbar, "Status bar").unwrap();
        let b = status.bounds.as_ref().unwrap();
        assert_eq!(resolved.x, b.x + b.w / 2.0);
    }

    #[test]
    fn selector_miss_when_nothing_matches() {
        let snap = notepad();
        let err = resolve_in_snapshot(
            &snap,
            &[Selector::AutomationId {
                value: "nope".into(),
            }],
        )
        .unwrap_err();
        assert!(matches!(err, PerceptionError::SelectorMiss));
    }

    #[test]
    fn empty_selector_list_is_a_miss() {
        let snap = notepad();
        let err = resolve_in_snapshot(&snap, &[]).unwrap_err();
        assert!(matches!(err, PerceptionError::SelectorMiss));
    }
}
