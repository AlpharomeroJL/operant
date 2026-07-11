//! `operant-golden-path-e2e`: NFR-6/NFR-7 headless golden path.
//!
//! This crate carries no library code of its own; it exists to hold the
//! integration test in `tests/golden_path.rs`, which drives the full
//! explore -> compile -> replay loop end to end with zero network/GPU. See
//! that file's own module doc for the full story.

/// Crate marker, in the same spirit as every other crate in this workspace
/// (e.g. `operant_recorder::CRATE`).
pub const CRATE: &str = "operant-golden-path-e2e";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-golden-path-e2e");
    }
}
