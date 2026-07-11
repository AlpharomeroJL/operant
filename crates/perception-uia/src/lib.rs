//! Perceiver implementations against the `operant-core` `Perceiver` trait
//! (C2): a real Windows UIA backend, and a fixture-backed one every lane
//! and `cargo test` runs against headlessly (`docs/specs/perception.md`).
//!
//! - [`FixturePerceiver`]: always built, no `windows` dependency. Loads
//!   `Snapshot`s straight from JSON
//!   (`contracts/perception_snapshot.schema.json`) and answers
//!   `snapshot`/`resolve`/`wait_until_changed` off that fixed data. This
//!   is what every headless test in this crate runs against.
//! - [`resolve_in_snapshot`]: selector-chain resolution shared by both
//!   backends, so `FixturePerceiver` and the real UIA backend agree on how
//!   a selector list turns into a clickable point.
//! - [`diff_snapshots`]: structural `SnapshotDiff` between two snapshots,
//!   keyed on the same selector-chain priority as resolve.
//! - [`compute_digest`]: the BLAKE3-over-normalized-elements-minus-bounds
//!   digest contract every Perceiver's `Snapshot.digest` follows.
//! - `topology`, `role`, `selectors`: internal helpers `resolve`, `diff`,
//!   and (behind `real-uia`) the live capture path share.
//! - [`uia`] (behind the `real-uia` cargo feature): [`UiaPerceiver`], the
//!   real windows-rs COM backend. Optional so the default build never
//!   links the `windows` crate.

mod diff;
mod digest;
mod fixture;
mod resolve;
mod role;
mod selectors;
mod topology;

#[cfg(feature = "real-uia")]
pub mod uia;

pub use diff::diff as diff_snapshots;
pub use digest::compute_digest;
pub use fixture::FixturePerceiver;
pub use resolve::resolve_in_snapshot;
pub use selectors::attach_selectors;

#[cfg(feature = "real-uia")]
pub use uia::UiaPerceiver;

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-perception-uia";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-perception-uia");
        let _ = "perception_uia";
    }
}
