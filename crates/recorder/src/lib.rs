//! Trajectory recorder (C7): SQLite (WAL) plus content-addressed blob store. Records Action IR, snapshot digests, grounding, timing, outcomes, corrections. Compiler input and audit substrate. L5A implements the full schema and GC.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-recorder";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-recorder");
        let _ = "recorder";
    }
}
