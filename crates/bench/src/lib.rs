//! Benchmark harness (C17): compiled replay vs re-inference-mock, emits BENCHMARKS.md. B1A scaffolds the renderer; L9B builds the real suite and the CI regression threshold.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-bench";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-bench");
        let _ = "bench";
    }
}
