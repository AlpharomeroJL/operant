//! Snapshot topology helpers: ancestor chains and role-scoped sibling
//! ordinals, shared by selector resolution (`resolve.rs`), structural
//! diffing (`diff.rs`), and (behind `real-uia`) selector construction
//! during capture (`selectors.rs`). All three walk the same
//! parent-indexed flat element list the way `docs/specs/perception.md`
//! describes selector chains: "role plus name path from the window root"
//! or "role plus ordinal among siblings".

use std::collections::HashMap;

use operant_ir::snapshot::Element;

use crate::role::role_str;

pub struct Topology<'a> {
    by_idx: HashMap<u32, &'a Element>,
}

impl<'a> Topology<'a> {
    pub fn build(elements: &'a [Element]) -> Self {
        Topology {
            by_idx: elements.iter().map(|e| (e.idx, e)).collect(),
        }
    }

    pub fn get(&self, idx: u32) -> Option<&'a Element> {
        self.by_idx.get(&idx).copied()
    }

    /// Ancestor chain from the root down to (and including) `idx`. Guards
    /// against a malformed `parent` cycle rather than looping forever.
    pub fn ancestors_inclusive(&self, idx: u32) -> Vec<&'a Element> {
        let mut chain = Vec::new();
        let mut cur = self.get(idx);
        while let Some(e) = cur {
            chain.push(e);
            if chain.len() > self.by_idx.len() {
                break;
            }
            cur = e.parent.and_then(|p| self.get(p));
        }
        chain.reverse();
        chain
    }

    /// 0-based index of `idx` among elements sharing its parent AND role,
    /// in element-array order: "role plus ordinal among siblings"
    /// (`docs/specs/perception.md`). O(n) in the element count; snapshot
    /// sizes here are UI trees (hundreds, not millions), so this stays
    /// simple rather than precomputing per-sibling-group indices.
    pub fn role_ordinal(&self, elements: &[Element], idx: u32) -> u32 {
        let Some(target) = self.get(idx) else {
            return 0;
        };
        elements
            .iter()
            .filter(|e| e.parent == target.parent && e.role == target.role)
            .position(|e| e.idx == idx)
            .unwrap_or(0) as u32
    }

    /// Role+name path from root to `idx`, as in `Selector::NameRolePath`.
    pub fn name_role_path(&self, idx: u32) -> Vec<(String, String)> {
        self.ancestors_inclusive(idx)
            .into_iter()
            .map(|e| (role_str(e.role), e.name.clone()))
            .collect()
    }

    /// Role+ordinal path from root to `idx`, as in `Selector::OrdinalPath`.
    pub fn ordinal_path(&self, elements: &[Element], idx: u32) -> Vec<(String, u32)> {
        self.ancestors_inclusive(idx)
            .into_iter()
            .map(|e| (role_str(e.role), self.role_ordinal(elements, e.idx)))
            .collect()
    }
}

/// The single element in `elements` for which `pred` holds, or `None` if
/// zero or more than one do. Ambiguous matches are treated as no match:
/// callers fall through to a less specific identity signal rather than
/// guess (`docs/specs/perception.md`: AutomationId counts only "if
/// nonempty AND unique in scope").
pub fn find_unique(elements: &[Element], pred: impl Fn(&Element) -> bool) -> Option<&Element> {
    let mut found: Option<&Element> = None;
    for e in elements {
        if pred(e) {
            if found.is_some() {
                return None;
            }
            found = Some(e);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::snapshot::{Role, Snapshot};

    fn notepad() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../contracts/fixtures/snapshot_notepad.json"
        ))
        .unwrap()
    }

    #[test]
    fn ancestors_walk_from_root() {
        let snap = notepad();
        let topo = Topology::build(&snap.elements);
        // idx 3 is the "File" menuitem: window -> menubar -> menuitem.
        let chain = topo.ancestors_inclusive(3);
        let roles: Vec<Role> = chain.iter().map(|e| e.role).collect();
        assert_eq!(roles, vec![Role::Window, Role::Menubar, Role::Menuitem]);
    }

    #[test]
    fn role_ordinal_is_scoped_to_same_role_siblings() {
        let snap = notepad();
        let topo = Topology::build(&snap.elements);
        // Window's four children (titlebar, menubar, document, statusbar)
        // are each the ONLY child of their role, so all ordinal 0 -- not
        // 0,1,2,3 as a role-agnostic sibling index would give.
        for idx in [1u32, 2, 5, 6] {
            assert_eq!(topo.role_ordinal(&snap.elements, idx), 0, "idx {idx}");
        }
        // "File" and "Edit" are both menuitems under the same menubar:
        // ordinals 0 and 1 respectively.
        assert_eq!(topo.role_ordinal(&snap.elements, 3), 0);
        assert_eq!(topo.role_ordinal(&snap.elements, 4), 1);
    }

    #[test]
    fn find_unique_rejects_ambiguous_matches() {
        let snap = notepad();
        assert!(find_unique(&snap.elements, |e| e.role == Role::Menuitem).is_none());
        assert!(find_unique(&snap.elements, |e| e.role == Role::Titlebar).is_some());
        assert!(find_unique(&snap.elements, |e| e.name == "nope").is_none());
    }
}
