//! MCP (Model Context Protocol), both directions (C14, FR-O3):
//! `docs/specs/mcp.md`: "Server: stdio and HTTP transports; every compiled
//! workflow exposes as a tool named workflow_<slug> with the manifest
//! inputs schema as the tool schema and the plain-English summary as the
//! description; invoking runs replay mode headless and returns the outcome
//! plus postcondition results... Client: external MCP servers configured
//! in settings register their tools as adapters under the mcp: namespace
//! with risk class write by default (user-adjustable per tool); tool
//! schemas map directly to adapter_call schemas. Both directions covered
//! by a scripted handshake test in CI."
//!
//! L13A implements the stdio transport for both directions (HTTP is a
//! followup; see this crate's own test/FOLLOWUPS notes).
//!
//! - [`jsonrpc`]: bare JSON-RPC 2.0 envelope helpers (request, response,
//!   notification, error codes). MCP's wire format.
//! - [`protocol`]: MCP's own message shapes layered on JSON-RPC --
//!   [`protocol::McpTool`], [`protocol::InitializeResult`],
//!   [`protocol::CallToolResult`].
//! - [`transport`]: [`transport::Transport`], the trait that carries
//!   JSON-RPC envelopes between peers; [`transport::StdioTransport`] (real
//!   newline-delimited stdio, generic over any reader/writer so it is
//!   itself unit-testable) and [`transport::InProcessTransport`] (an
//!   in-process peer function, no socket or child process -- what the
//!   client-direction handshake test wires [`client::McpClient`] to).
//! - [`server`]: [`server::McpServer`], the server direction: holds a
//!   fixed set of already-compiled workflows and answers `initialize`,
//!   `tools/list`, and `tools/call` by replaying headless through
//!   `operant-replay`.
//! - [`client`]: [`client::McpClient`], the client direction (one
//!   connection to one external server), and [`client::McpClientAdapter`],
//!   which wraps a connected client as an `operant_action::Adapter` under
//!   the `mcp` namespace so its tools reach the executor as ordinary
//!   `adapter_call` steps.
//!
//! ```
//! use std::collections::HashMap;
//! use operant_orchestrator::mcp::{InProcessTransport, McpClient, McpServer, McpServerConfig};
//! use serde_json::json;
//!
//! // No compiled workflows and no external peer here -- just the shape of
//! // the two constructors this doctest wants to prove compile and link.
//! # fn snapshot() -> operant_ir::Snapshot {
//! #     operant_ir::Snapshot {
//! #         v: 1, source: operant_ir::SnapshotSource::Fixture,
//! #         window: operant_ir::WindowInfo { hwnd: None, process: "x".into(), title: "x".into(), monitor: None, dpi_scale: 1.0 },
//! #         digest: "d".into(), truncated: false, captured_ms: None, elements: vec![],
//! #     }
//! # }
//! let server = McpServer::new(McpServerConfig::default(), vec![], snapshot());
//! assert!(server.tools().is_empty());
//!
//! let transport = InProcessTransport::new(|_msg| None);
//! let _client = McpClient::new(transport);
//! ```

pub mod client;
pub mod jsonrpc;
pub mod protocol;
pub mod server;
pub mod transport;

pub use client::{McpClient, McpClientAdapter, McpClientError};
pub use protocol::{
    CallToolResult, InitializeResult, McpTool, ServerInfo, ToolContent, PROTOCOL_VERSION,
};
pub use server::{McpServer, McpServerConfig};
pub use transport::{InProcessTransport, StdioTransport, Transport};

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-orchestrator-mcp";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-orchestrator-mcp");
    }
}
