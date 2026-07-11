//! Snapshot digest for browser-sourced snapshots: BLAKE3 over the
//! normalized element list minus bounds, exactly
//! `contracts/perception_snapshot.schema.json`'s "layout changes must not
//! change the digest; content changes must" -- the same property, and the
//! same algorithm, `operant-perception-uia::digest::compute_digest`
//! already implements for UIA snapshots
//! (`crates/perception-uia/src/digest.rs`).
//!
//! Duplicated here rather than pulling `operant-perception-uia` in as a
//! dependency of this crate for one ten-line pure function: this lane's
//! owned paths are `crates/action/src/adapters/browser` plus this crate's
//! own `Cargo.toml`, and `operant-perception-uia` is a Windows UIA
//! perception crate with no reason to be a dependency of the action
//! layer otherwise. See FOLLOWUPS in `RESULT.md` for hoisting this into
//! `operant-core` so every snapshot source (UIA, browser, future AX/
//! AT-SPI) shares one implementation instead of two identical copies.

use operant_ir::snapshot::Element;

/// BLAKE3 hex digest over `elements`, with every element's `bounds`
/// stripped before hashing. Deterministic: the input bytes come from
/// `Element`'s serde field order, not iteration/hash order, so equal
/// element lists always hash identically regardless of how they were
/// built.
pub fn compute_digest(elements: &[Element]) -> String {
    let normalized: Vec<Element> = elements
        .iter()
        .cloned()
        .map(|mut e| {
            e.bounds = None;
            e
        })
        .collect();
    let bytes = serde_json::to_vec(&normalized).expect("Element serialization is infallible");
    blake3::hash(&bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use operant_ir::snapshot::{Bounds, Role};

    fn sample() -> Vec<Element> {
        vec![Element {
            idx: 0,
            parent: None,
            role: Role::Button,
            name: "Save invoice".into(),
            value: None,
            automation_id: Some("save-btn".into()),
            bounds: Some(Bounds {
                x: 40.0,
                y: 220.0,
                w: 140.0,
                h: 36.0,
                monitor: Some("MON1".into()),
            }),
            enabled: true,
            offscreen: false,
            is_password: false,
            patterns: vec!["invoke".into()],
            selectors: vec![],
        }]
    }

    #[test]
    fn digest_stable_under_bounds_only_change() {
        let a = sample();
        let mut b = sample();
        b[0].bounds = Some(Bounds {
            x: 999.0,
            y: 999.0,
            w: 5.0,
            h: 5.0,
            monitor: Some("MON2".into()),
        });
        assert_eq!(compute_digest(&a), compute_digest(&b));

        let mut c = sample();
        c[0].bounds = None;
        assert_eq!(compute_digest(&a), compute_digest(&c));
    }

    #[test]
    fn digest_changes_when_the_button_is_renamed() {
        let a = sample();
        let mut renamed = sample();
        renamed[0].name = "Store invoice".into();
        renamed[0].automation_id = Some("store-btn".into());
        assert_ne!(compute_digest(&a), compute_digest(&renamed));
    }

    #[test]
    fn digest_changes_under_value_change() {
        let a = sample();
        let mut valued = sample();
        valued[0].value = Some("dirty".into());
        assert_ne!(compute_digest(&a), compute_digest(&valued));
    }
}
