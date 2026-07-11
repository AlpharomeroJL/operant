//! Orchestrator and model backends (C6): the explore loop with element-digest discipline and HITL, plus the backend trait and provider quirk tables. This is the ONLY crate with network model access. L4A/L7A own it.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-orchestrator";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-orchestrator");
        let _ = "orchestrator";
    }
}
