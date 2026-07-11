//! Orchestrator and model backends (C6): the explore loop with element-digest discipline and HITL, plus the backend trait and provider quirk tables. This is the ONLY crate with network model access. L4A/L7A own it.
//!
//! Scaffold: this crate compiles and its smoke test passes. The owning lane replaces
//! this body with the real implementation against the frozen contracts in `contracts/`.
//!
//! [`backends`] (L4A): the `ModelBackend` trait, the provider quirk table,
//! the injectable HTTP transport, and the mock/fixture backends CI runs
//! against. See `backends`' own module doc for the full surface; the
//! explore loop itself (L7A) is not implemented here yet.

pub mod backends;

pub use backends::ModelBackend;

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
