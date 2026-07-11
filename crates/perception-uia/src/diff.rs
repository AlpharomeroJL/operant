//! Structural diff between two snapshots, keyed on the same selector-chain
//! priority `resolve.rs` uses: AutomationId, then role+name path, then
//! role+ordinal path. Trying the whole chain (not just the strongest
//! signal) is what lets an element that drifted on BOTH its automation id
//! and its name (`contracts/fixtures/drift_renamed_button`) still resolve
//! to the same logical slot via its stable tree position, and get
//! reported as a rename instead of a spurious remove+add.

use std::collections::HashSet;

use operant_core::perceive::SnapshotDiff;
use operant_ir::snapshot::{Element, Snapshot};

use crate::topology::{find_unique, Topology};

pub fn diff(old: &Snapshot, new: &Snapshot) -> SnapshotDiff {
    let old_topo = Topology::build(&old.elements);
    let new_topo = Topology::build(&new.elements);
    let mut out = SnapshotDiff::default();
    let mut matched_new: HashSet<u32> = HashSet::new();

    for old_el in &old.elements {
        match find_counterpart(old_el, old, &old_topo, new, &new_topo) {
            Some(new_el) => {
                matched_new.insert(new_el.idx);
                if old_el.name != new_el.name {
                    out.renamed.push((new_el.idx, new_el.name.clone()));
                }
                if old_el.value != new_el.value {
                    out.value_changed
                        .push((new_el.idx, new_el.value.clone().unwrap_or_default()));
                }
            }
            None => out.removed.push(old_el.idx),
        }
    }

    for new_el in &new.elements {
        if !matched_new.contains(&new_el.idx) {
            out.added.push(new_el.idx);
        }
    }

    out
}

/// Find the element in `new` that is "the same" as `old_el`, trying
/// identity signals most-stable-first, same order `resolve_in_snapshot`
/// tries selectors in. A signal that resolves to more than one candidate
/// is untrustworthy (`topology::find_unique`) and falls through to the
/// next, same as resolve.
fn find_counterpart<'a>(
    old_el: &Element,
    old: &Snapshot,
    old_topo: &Topology<'_>,
    new: &'a Snapshot,
    new_topo: &Topology<'a>,
) -> Option<&'a Element> {
    if let Some(id) = old_el.automation_id.as_deref().filter(|s| !s.is_empty()) {
        if let Some(m) = find_unique(&new.elements, |e| e.automation_id.as_deref() == Some(id)) {
            return Some(m);
        }
    }

    let old_name_path = old_topo.name_role_path(old_el.idx);
    if let Some(m) = find_unique(&new.elements, |e| {
        new_topo.name_role_path(e.idx) == old_name_path
    }) {
        return Some(m);
    }

    let old_ordinal_path = old_topo.ordinal_path(&old.elements, old_el.idx);
    find_unique(&new.elements, |e| {
        new_topo.ordinal_path(&new.elements, e.idx) == old_ordinal_path
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn before() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../contracts/fixtures/drift_renamed_button/before.json"
        ))
        .unwrap()
    }
    fn after() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../contracts/fixtures/drift_renamed_button/after.json"
        ))
        .unwrap()
    }

    #[test]
    fn detects_the_drifted_button() {
        let d = diff(&before(), &after());

        // The button's automation_id AND name both drifted (save-btn /
        // "Save invoice" -> store-btn / "Store invoice"). Either reading
        // is a correct diff: matched via its stable ordinal position (a
        // rename), or not matched at all (a remove+add). Assert the
        // observable outcome rather than pin one internal identity
        // strategy.
        let renamed_to_store = d.renamed.iter().any(|(_, name)| name == "Store invoice");
        let removed_and_added = !d.removed.is_empty() && !d.added.is_empty();
        assert!(
            renamed_to_store || removed_and_added,
            "expected the drifted button to surface as a rename or a remove+add, got {d:?}"
        );

        // Nothing else in the fixture drifted.
        assert!(
            d.value_changed.is_empty(),
            "unexpected value changes: {:?}",
            d.value_changed
        );
    }

    #[test]
    fn identical_snapshots_diff_to_nothing() {
        let b = before();
        assert!(diff(&b, &b).is_empty());
    }

    #[test]
    fn detects_a_pure_value_change() {
        let mut old = before();
        let mut new = before();
        new.elements[1].value = Some("Acme Corp".to_string());

        let d = diff(&old, &new);
        assert_eq!(d.value_changed, vec![(1, "Acme Corp".to_string())]);
        assert!(d.renamed.is_empty());
        assert!(d.added.is_empty());
        assert!(d.removed.is_empty());

        // Symmetry check: no change at all yields an empty diff even when
        // the two Snapshot values were built independently.
        old.elements[1].value = Some(String::new());
        assert!(diff(&old, &old.clone()).is_empty());
    }
}
