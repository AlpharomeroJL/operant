//! Windows UIA Perceiver (C2). L2A implements windows-rs COM snapshotting here and adds the `windows` dependency. Scaffold provides a fixture-backed Perceiver so the workspace compiles and tests run headless.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

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
