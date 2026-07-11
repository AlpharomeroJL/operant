//! Snapshot digest: BLAKE3 over the normalized element list minus bounds.
//! `contracts/perception_snapshot.schema.json`: "layout changes must not
//! change the digest; content changes must." Shared by every Perceiver
//! (the real UIA backend computes it fresh off a live walk; fixtures ship
//! a precomputed one) so the algorithm lives in exactly one place.

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
            name: "Save".into(),
            value: None,
            automation_id: Some("save-btn".into()),
            bounds: Some(Bounds {
                x: 10.0,
                y: 10.0,
                w: 100.0,
                h: 20.0,
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

        // Also stable when bounds disappear entirely (e.g. offscreen).
        let mut c = sample();
        c[0].bounds = None;
        assert_eq!(compute_digest(&a), compute_digest(&c));
    }

    #[test]
    fn digest_changes_under_value_change() {
        let a = sample();

        let mut renamed = sample();
        renamed[0].name = "Store".into();
        assert_ne!(compute_digest(&a), compute_digest(&renamed));

        let mut valued = sample();
        valued[0].value = Some("dirty".into());
        assert_ne!(compute_digest(&a), compute_digest(&valued));
    }
}
