//! Registry client (C16): fetch, Ed25519 verify against pinned publisher keys, staged install with grants, unsigned = dry-run only. R1B implements install; L7B adds publish.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-registry";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-registry");
        let _ = "registry";
    }
}
