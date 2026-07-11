//! Deterministic replay executor (C8). Links ONLY against ir, action, and gates: it has no path to any model backend, so zero-model replay is enforced by the crate graph, not a runtime flag. L8A implements the interpreter.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-replay";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-replay");
        let _ = "replay";
    }
}
