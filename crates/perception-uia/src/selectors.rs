//! Selector-chain construction for a freshly captured element list:
//! `docs/specs/perception.md`'s "(1) AutomationId if nonempty and unique
//! in scope, (2) role plus name path from the window root, (3) role plus
//! ordinal among siblings. Store all three." `FixturePerceiver` never
//! calls this -- its snapshots ship selectors pre-authored in JSON -- it
//! is only for a live capture (the `real-uia` `UiaPerceiver`), run once
//! over the fully-built element list (parent links must already be final
//! so ancestor paths resolve). No `windows` dependency, so it lives at the
//! crate root and is exercised by the default headless test run rather
//! than only under `--features real-uia`.
//!
//! `resolve.rs` and `diff.rs` match against exactly this same chain, so
//! what gets captured here and what replay/diff later match against can
//! never disagree about what "the same element" means.

use operant_ir::snapshot::Element;
use operant_ir::{NameRoleSeg, OrdinalSeg, Selector};

use crate::topology::Topology;

pub fn attach_selectors(elements: &mut [Element]) {
    let snapshot = elements.to_vec();
    let topo = Topology::build(&snapshot);

    for element in elements.iter_mut() {
        let mut selectors = Vec::with_capacity(3);

        if let Some(id) = element.automation_id.as_deref() {
            if !id.is_empty() && is_unique_automation_id(&snapshot, id) {
                selectors.push(Selector::AutomationId {
                    value: id.to_string(),
                });
            }
        }

        let name_path = topo
            .name_role_path(element.idx)
            .into_iter()
            .map(|(role, name)| NameRoleSeg { role, name })
            .collect();
        selectors.push(Selector::NameRolePath { path: name_path });

        // Root elements have no siblings; an ordinal path would be a
        // trivially-always-0 no-op, so it is omitted rather than stored
        // (matches `contracts/fixtures/snapshot_notepad.json`'s root,
        // which carries only a name_role_path selector).
        if element.parent.is_some() {
            let ordinal_path = topo
                .ordinal_path(&snapshot, element.idx)
                .into_iter()
                .map(|(role, ordinal)| OrdinalSeg { role, ordinal })
                .collect();
            selectors.push(Selector::OrdinalPath { path: ordinal_path });
        }

        element.selectors = selectors;
    }
}

fn is_unique_automation_id(elements: &[Element], id: &str) -> bool {
    elements
        .iter()
        .filter(|e| e.automation_id.as_deref() == Some(id))
        .count()
        == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::snapshot::Role;

    fn leaf(
        idx: u32,
        parent: Option<u32>,
        role: Role,
        name: &str,
        automation_id: Option<&str>,
    ) -> Element {
        Element {
            idx,
            parent,
            role,
            name: name.to_string(),
            value: None,
            automation_id: automation_id.map(|s| s.to_string()),
            bounds: None,
            enabled: true,
            offscreen: false,
            is_password: false,
            patterns: vec![],
            selectors: vec![],
        }
    }

    #[test]
    fn root_gets_no_ordinal_path_leaf_gets_all_three() {
        let mut elements = vec![
            leaf(0, None, Role::Window, "Win", None),
            leaf(1, Some(0), Role::Button, "OK", Some("ok-btn")),
        ];
        attach_selectors(&mut elements);

        assert!(!elements[0]
            .selectors
            .iter()
            .any(|s| matches!(s, Selector::OrdinalPath { .. })));
        assert_eq!(elements[0].selectors.len(), 1);

        assert!(matches!(
            elements[1].selectors[0],
            Selector::AutomationId { .. }
        ));
        assert!(elements[1]
            .selectors
            .iter()
            .any(|s| matches!(s, Selector::NameRolePath { .. })));
        assert!(elements[1]
            .selectors
            .iter()
            .any(|s| matches!(s, Selector::OrdinalPath { .. })));
        assert_eq!(elements[1].selectors.len(), 3);
    }

    #[test]
    fn duplicate_automation_id_is_not_stored() {
        let mut elements = vec![
            leaf(0, None, Role::Window, "Win", None),
            leaf(1, Some(0), Role::Button, "A", Some("dup")),
            leaf(2, Some(0), Role::Button, "B", Some("dup")),
        ];
        attach_selectors(&mut elements);
        for e in &elements[1..] {
            assert!(!e
                .selectors
                .iter()
                .any(|s| matches!(s, Selector::AutomationId { .. })));
        }
    }

    #[test]
    fn empty_automation_id_is_not_stored() {
        let mut elements = vec![
            leaf(0, None, Role::Window, "Win", None),
            leaf(1, Some(0), Role::Button, "OK", Some("")),
        ];
        attach_selectors(&mut elements);
        assert!(!elements[1]
            .selectors
            .iter()
            .any(|s| matches!(s, Selector::AutomationId { .. })));
    }
}
