//! Single-step re-grounding: relocate one drifted target in the live snapshot.
//!
//! Re-grounding is deliberately narrow. It repairs exactly one step's target,
//! never a chain of steps in a single pass. It keys on the invariant the drift
//! fixture models: a control whose id and name changed but whose role and tree
//! slot did not. The element occupying the same role plus stable ordinal path
//! in the live snapshot is taken to be the same logical control, and its fresh
//! selector chain plus a re-captured anchor become the repair candidate.

use operant_ir::snapshot::{Element, Snapshot};
use operant_ir::{Action, Anchor, Selector};

use crate::drift::resolve::{find_unique, resolve, role_str, Topology};

/// A re-grounded target: the element found in the live snapshot to occupy the
/// same stable slot the step used to point at, plus the fresh selector chain
/// and anchor lifted from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The relocated element's selector chain, ordered by stability score.
    pub selectors: Vec<Selector>,
    /// A freshly captured anchor for the relocated element, when the step
    /// carried one.
    pub anchor: Option<Anchor>,
    /// The relocated element's current name, for the plain-language offer.
    pub name: String,
    /// The relocated element's index in the live snapshot.
    pub idx: u32,
}

/// Re-ground `step` against the live snapshot `current`, using `before` (the
/// snapshot the workflow was compiled against) to recover the target's stable
/// tree position.
///
/// The old target is recovered by resolving the step's selectors against
/// `before` (where they still match), reading off its role and ordinal path.
/// The repair is the single live element with that same role and ordinal path.
/// Returns `None` when the old target cannot be recovered or no unique live
/// element occupies the same slot, in which case the caller must halt to a
/// human rather than guess.
pub fn reground(step: &Action, before: &Snapshot, current: &Snapshot) -> Option<Candidate> {
    let selectors = step
        .target
        .as_ref()
        .map(|t| t.selectors.as_slice())
        .unwrap_or(&[]);

    // Recover the original target from the snapshot it was recorded against.
    let old = resolve(before, selectors)?;
    let old_role = old.role;
    let before_topo = Topology::build(&before.elements);
    let old_path = before_topo.ordinal_path(&before.elements, old.idx);

    // The repair is the single live element with the same role and the same
    // stable ordinal path. `find_unique` refuses an ambiguous match, so a slot
    // that now holds two same-role siblings will not be silently mis-repaired.
    let current_topo = Topology::build(&current.elements);
    let matched = find_unique(&current.elements, |e| {
        e.role == old_role
            && current_topo.ordinal_path(&current.elements, e.idx) == old_path
    })?;

    Some(Candidate {
        selectors: ordered_selectors(matched),
        anchor: reground_anchor(step, matched),
        name: matched.name.clone(),
        idx: matched.idx,
    })
}

/// The matched element's selectors, ordered by descending stability score so
/// the strongest identity signal leads, the same ordering compiler pass 3
/// applies. A stable sort keeps equally scored selectors in recorded order.
fn ordered_selectors(el: &Element) -> Vec<Selector> {
    let mut selectors = el.selectors.clone();
    selectors.sort_by_key(|s| std::cmp::Reverse(s.score()));
    selectors
}

/// Re-capture the visual anchor for the relocated element, keeping the step's
/// original tolerance. The hash stands in for a freshly grabbed screenshot
/// region and is derived deterministically from the element's identity so the
/// same drift always yields the same candidate. Returns `None` when the step
/// carried no anchor.
fn reground_anchor(step: &Action, matched: &Element) -> Option<Anchor> {
    let tolerance = step
        .target
        .as_ref()
        .and_then(|t| t.anchor.as_ref())
        .map(|a| a.tolerance)?;
    let seed = format!(
        "{}:{}:{:?}",
        role_str(matched.role),
        matched.name,
        matched.bounds
    );
    Some(Anchor {
        img_hash: blake3::hash(seed.as_bytes()).to_hex().to_string(),
        tolerance,
    })
}
