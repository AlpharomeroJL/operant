//! Trajectory compiler (C8): normalize, parameterize, selectorize, insert waits and asserts, emit TypeScript DSL plus manifest. L8A implements the five passes; L8B adds drift repair.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-compiler";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-compiler");
        let _ = "compiler";
    }
}
