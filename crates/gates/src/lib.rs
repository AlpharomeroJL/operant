//! Invariant gate engine (C9): evaluates the JSON predicate AST from contracts/gates against snapshot, filesystem, and adapter results. L6A implements the evaluator and the runtime-owned safety deny-list.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-gates";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-gates");
        let _ = "gates";
    }
}
