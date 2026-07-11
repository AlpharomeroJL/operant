//! Self-diagnostics (C19 doctor): model reachability, disk, updater, permissions, audio. Surfaced as `operant doctor` and the 'Check my setup' button. U3A owns it.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-doctor";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-doctor");
        let _ = "doctor";
    }
}
