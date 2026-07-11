//! Safety, permissions, audit (C10): capability grants, dry-run interpreter, hash-chained audit log, and the runtime-enforced hard invariants (FR-S4) that no workflow file can disable. L6A owns this.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-safety";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-safety");
        let _ = "safety";
    }
}
