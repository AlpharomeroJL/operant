//! The MCP client direction (`docs/specs/mcp.md`): connect to one external
//! MCP server over any [`Transport`], drive the handshake
//! (`initialize` -> `notifications/initialized` -> `tools/list`), and
//! register the tools it discovers as `mcp` namespace
//! [`operant_action::Adapter`] verbs -- "tool schemas map directly to
//! adapter_call schemas" -- so an external server's tools reach the
//! executor exactly like any native adapter, through an ordinary
//! `adapter_call` step.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use operant_action::adapter::{Adapter, AdapterError, Idempotency, VerbSpec};
use operant_ir::RiskClass;
use serde_json::{json, Value};
use thiserror::Error;

use super::jsonrpc;
use super::protocol::{CallToolResult, InitializeResult, McpTool, PROTOCOL_VERSION};
use super::transport::Transport;

#[derive(Debug, Error)]
pub enum McpClientError {
    #[error("transport error: {0}")]
    Transport(#[from] std::io::Error),
    #[error("the peer closed the connection before responding")]
    ClosedConnection,
    #[error("peer returned a JSON-RPC error {code}: {message}")]
    Peer { code: i64, message: String },
    #[error("peer response did not match the expected shape: {0}")]
    MalformedResponse(String),
}

/// A connection to one external MCP server, over any [`Transport`]. Owns
/// the client half of the handshake and every request/response id it has
/// issued.
pub struct McpClient<T: Transport> {
    transport: T,
    next_id: AtomicI64,
}

impl<T: Transport> McpClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: AtomicI64::new(1),
        }
    }

    /// Send a request, then read messages until the one whose `id` matches
    /// (any other message received first -- a stray notification from the
    /// peer -- is skipped, not treated as an error).
    fn call_method(&mut self, method: &str, params: Value) -> Result<Value, McpClientError> {
        let id = Value::from(self.next_id.fetch_add(1, Ordering::SeqCst));
        self.transport
            .send(&jsonrpc::request(id.clone(), method, params))?;
        loop {
            let msg = self
                .transport
                .recv()?
                .ok_or(McpClientError::ClosedConnection)?;
            if msg.get("id") != Some(&id) {
                continue;
            }
            if let Some(err) = msg.get("error") {
                let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
                let message = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_string();
                return Err(McpClientError::Peer { code, message });
            }
            return msg.get("result").cloned().ok_or_else(|| {
                McpClientError::MalformedResponse(
                    "response has neither result nor error".to_string(),
                )
            });
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), McpClientError> {
        self.transport
            .send(&jsonrpc::notification(method, params))?;
        Ok(())
    }

    /// `initialize`, then the `notifications/initialized` acknowledgement.
    pub fn initialize(
        &mut self,
        client_name: &str,
        client_version: &str,
    ) -> Result<InitializeResult, McpClientError> {
        let result = self.call_method(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientInfo": { "name": client_name, "version": client_version },
                "capabilities": {}
            }),
        )?;
        let init: InitializeResult = serde_json::from_value(result)
            .map_err(|e| McpClientError::MalformedResponse(e.to_string()))?;
        self.notify("notifications/initialized", json!({}))?;
        Ok(init)
    }

    /// `tools/list`.
    pub fn list_tools(&mut self) -> Result<Vec<McpTool>, McpClientError> {
        let result = self.call_method("tools/list", json!({}))?;
        let tools = result.get("tools").cloned().unwrap_or_else(|| json!([]));
        serde_json::from_value(tools).map_err(|e| McpClientError::MalformedResponse(e.to_string()))
    }

    /// `tools/call`.
    pub fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, McpClientError> {
        let result = self.call_method(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        serde_json::from_value(result).map_err(|e| McpClientError::MalformedResponse(e.to_string()))
    }
}

/// Registers one connected [`McpClient`]'s discovered tools as `mcp`
/// namespace adapter verbs. `docs/specs/mcp.md`: "external MCP servers
/// configured in settings register their tools as adapters under the mcp:
/// namespace with risk class write by default (user-adjustable per tool)".
pub struct McpClientAdapter<T: Transport> {
    client: Mutex<McpClient<T>>,
    verbs: Vec<VerbSpec>,
}

impl<T: Transport> McpClientAdapter<T> {
    /// Perform the handshake and discover tools, then build the adapter.
    /// `risk_overrides` marks specific tool names with a non-default risk
    /// class; anything not named there gets [`RiskClass::Write`], the
    /// spec's default.
    pub fn connect(
        mut client: McpClient<T>,
        client_name: &str,
        client_version: &str,
        risk_overrides: &HashMap<String, RiskClass>,
    ) -> Result<Self, McpClientError> {
        client.initialize(client_name, client_version)?;
        let tools = client.list_tools()?;
        let verbs = tools
            .into_iter()
            .map(|t| {
                let risk = risk_overrides
                    .get(&t.name)
                    .copied()
                    .unwrap_or(RiskClass::Write);
                VerbSpec::new(t.name, t.input_schema, risk, Idempotency::Unknown)
            })
            .collect();
        Ok(Self {
            client: Mutex::new(client),
            verbs,
        })
    }
}

impl<T: Transport> Adapter for McpClientAdapter<T> {
    fn namespace(&self) -> &str {
        "mcp"
    }

    fn verbs(&self) -> &[VerbSpec] {
        &self.verbs
    }

    fn call(&self, verb: &str, args: &Value) -> Result<Value, AdapterError> {
        let mut client = self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result =
            client
                .call_tool(verb, args.clone())
                .map_err(|e| AdapterError::CallFailed {
                    namespace: "mcp".to_string(),
                    verb: verb.to_string(),
                    message: e.to_string(),
                })?;
        if result.is_error {
            let message = result
                .content
                .first()
                .map(|c| c.text.clone())
                .unwrap_or_default();
            return Err(AdapterError::CallFailed {
                namespace: "mcp".to_string(),
                verb: verb.to_string(),
                message,
            });
        }
        serde_json::to_value(&result).map_err(|e| AdapterError::CallFailed {
            namespace: "mcp".to_string(),
            verb: verb.to_string(),
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::transport::InProcessTransport;

    /// A tiny in-process peer for unit-level client tests: one `echo` tool.
    /// The full handshake-both-directions proof against an independently
    /// coded peer lives in `tests/mcp_handshake.rs`; this just exercises
    /// `McpClient`'s own request/response bookkeeping (id matching,
    /// skipping stray notifications, error propagation) in isolation.
    fn echo_peer(msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned()?;
        match msg.get("method").and_then(Value::as_str)? {
            "initialize" => Some(json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "serverInfo": { "name": "echo-peer", "version": "1.0.0" },
                    "capabilities": {}
                }
            })),
            "tools/list" => Some(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "tools": [ {
                    "name": "echo",
                    "description": "Echoes its input.",
                    "inputSchema": { "type": "object" }
                } ] }
            })),
            "tools/call" => {
                let args = msg
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or(json!({}));
                Some(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [ { "type": "text", "text": args.to_string() } ], "isError": false }
                }))
            }
            _ => None,
        }
    }

    #[test]
    fn initialize_list_and_call_round_trip_against_an_in_process_peer() {
        let transport = InProcessTransport::new(echo_peer);
        let mut client = McpClient::new(transport);

        let init = client
            .initialize("test", "0.0.0")
            .expect("handshake succeeds");
        assert_eq!(init.server_info.name, "echo-peer");

        let tools = client.list_tools().expect("tools/list succeeds");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        let result = client
            .call_tool("echo", json!({ "x": 1 }))
            .expect("tools/call succeeds");
        assert!(!result.is_error);
        assert_eq!(result.content[0].text, json!({ "x": 1 }).to_string());
    }

    #[test]
    fn a_peer_error_response_surfaces_as_a_typed_client_error() {
        let transport = InProcessTransport::new(|msg: Value| {
            let id = msg.get("id").cloned()?;
            Some(
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "nope" } }),
            )
        });
        let mut client = McpClient::new(transport);
        let err = client.list_tools().unwrap_err();
        assert!(
            matches!(err, McpClientError::Peer { code: -32601, .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn connect_builds_one_verb_per_discovered_tool_at_the_spec_default_risk_class() {
        let transport = InProcessTransport::new(echo_peer);
        let client = McpClient::new(transport);
        let adapter = McpClientAdapter::connect(client, "test", "0.0.0", &HashMap::new())
            .expect("connect succeeds");
        assert_eq!(adapter.namespace(), "mcp");
        assert_eq!(adapter.verbs().len(), 1);
        assert_eq!(adapter.verbs()[0].name, "echo");
        assert_eq!(adapter.verbs()[0].risk_class, RiskClass::Write);
    }

    #[test]
    fn connect_honors_a_per_tool_risk_override() {
        let transport = InProcessTransport::new(echo_peer);
        let client = McpClient::new(transport);
        let mut overrides = HashMap::new();
        overrides.insert("echo".to_string(), RiskClass::Read);
        let adapter = McpClientAdapter::connect(client, "test", "0.0.0", &overrides)
            .expect("connect succeeds");
        assert_eq!(adapter.verbs()[0].risk_class, RiskClass::Read);
    }
}
