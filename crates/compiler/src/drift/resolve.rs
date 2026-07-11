//! Minimal, self-contained selector resolution and snapshot topology over
//! `operant_ir` snapshots.
//!
//! Drift repair lives in the compiler crate, which depends only on
//! `operant-ir`, not on `operant-perception-uia` where the production
//! resolver (`resolve.rs`) and topology (`topology.rs`) live. Detection and
//! re-grounding need to answer the same two questions those modules answer,
//! "does this selector chain still resolve?" and "what is this element's
//! stable tree position?", so this file reimplements just that slice against
//! the shared IR types. It keeps the same selector priority order documented
//! in `docs/specs/perception.md`: AutomationId, then role plus name path,
//! then role plus ordinal path, with Css understood as the `#id` shorthand
//! matched against `automation_id`.

use std::collections::HashMap;

use operant_ir::snapshot::{Element, Role, Snapshot};
use operant_ir::{NameRoleSeg, OrdinalSeg, Selector};

/// Serialize a `Role` to its lowercase schema string (`Role::Button` becomes
/// "button"), the representation `Selector::NameRolePath` segments and the
/// perception snapshot wire format use. Reads it back through the same serde
/// derive `operant_ir::Role` uses so it can never drift from the wire form.
pub fn role_str(role: Role) -> String {
    match serde_json::to_value(role) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

/// A flat element tree indexed by `idx`, mirroring the perception-uia
/// `Topology` this crate cannot reach.
pub struct Topology<'a> {
    by_idx: HashMap<u32, &'a Element>,
}

impl<'a> Topology<'a> {
    pub fn build(elements: &'a [Element]) -> Self {
        Topology {
            by_idx: elements.iter().map(|e| (e.idx, e)).collect(),
        }
    }

    fn get(&self, idx: u32) -> Option<&'a Element> {
        self.by_idx.get(&idx).copied()
    }

    /// Ancestor chain from the root down to (and including) `idx`. Guards a
    /// malformed `parent` cycle rather than looping forever.
    fn ancestors_inclusive(&self, idx: u32) -> Vec<&'a Element> {
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

    /// 0-based index of `idx` among elements sharing its parent and role, in
    /// element-array order: the "role plus ordinal among siblings" notion
    /// from `docs/specs/perception.md`.
    fn role_ordinal(&self, elements: &[Element], idx: u32) -> u32 {
        let Some(target) = self.get(idx) else {
            return 0;
        };
        elements
            .iter()
            .filter(|e| e.parent == target.parent && e.role == target.role)
            .position(|e| e.idx == idx)
            .unwrap_or(0) as u32
    }

    /// Role plus name path from root to `idx`, as in `Selector::NameRolePath`.
    pub fn name_role_path(&self, idx: u32) -> Vec<(String, String)> {
        self.ancestors_inclusive(idx)
            .into_iter()
            .map(|e| (role_str(e.role), e.name.clone()))
            .collect()
    }

    /// Role plus ordinal path from root to `idx`, as in
    /// `Selector::OrdinalPath`. This is the "stable position" re-grounding
    /// keys on when an element keeps its slot but changes id and name.
    pub fn ordinal_path(&self, elements: &[Element], idx: u32) -> Vec<(String, u32)> {
        self.ancestors_inclusive(idx)
            .into_iter()
            .map(|e| (role_str(e.role), self.role_ordinal(elements, e.idx)))
            .collect()
    }
}

/// The single element for which `pred` holds, or `None` if zero or more than
/// one do. An ambiguous match is treated as no match: callers fall through to
/// a less specific identity signal rather than guess, the same rule the
/// production resolver follows.
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

/// Resolve one selector against a snapshot, returning the unique matching
/// element (absent or ambiguous yields `None`), with the same fall-through
/// semantics as the production resolver.
pub fn match_selector<'a>(
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
        Selector::Css { value } => value
            .strip_prefix('#')
            .filter(|id| !id.is_empty())
            .and_then(|id| {
                find_unique(&snapshot.elements, |e| e.automation_id.as_deref() == Some(id))
            }),
    }
}

/// Resolve a selector chain: the first selector that resolves uniquely wins,
/// returning that element. `None` when every selector in the chain misses.
pub fn resolve<'a>(snapshot: &'a Snapshot, selectors: &[Selector]) -> Option<&'a Element> {
    let topo = Topology::build(&snapshot.elements);
    selectors
        .iter()
        .find_map(|s| match_selector(snapshot, &topo, s))
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
