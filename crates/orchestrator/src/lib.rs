//! Orchestrator and model backends (C6): the explore loop with element-digest discipline and HITL, plus the backend trait and provider quirk tables. This is the ONLY crate with network model access. L4A/L7A own it.
//!
//! [`backends`] (L4A): the `ModelBackend` trait, the provider quirk table,
//! the injectable HTTP transport, and the mock/fixture backends CI runs
//! against. See `backends`' own module doc for the full surface.
//!
//! [`explore`] (L7A): [`explore::ExploreLoop`], the EXPLORE loop itself --
//! goal -> perceive -> element digest -> plan -> propose an Action -> the
//! safety gate -> execute -> observe -> record -> repeat until the planner
//! signals done, with HITL pause/redirect/resume as bus events. See
//! `explore`'s own module doc for the full surface.
//!
//! [`mcp`] (L13A): MCP both directions (C14, FR-O3) -- a server that
//! exposes every compiled workflow as a `workflow_<slug>` tool over stdio,
//! and a client that registers an external MCP server's tools as `mcp`
//! namespace adapters. See `mcp`'s own module doc and `docs/specs/mcp.md`.

pub mod backends;
pub mod explore;
pub mod mcp;

pub use backends::ModelBackend;
pub use explore::ExploreLoop;

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
