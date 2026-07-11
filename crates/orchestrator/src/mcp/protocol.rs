//! MCP's own message shapes, layered on top of bare [`super::jsonrpc`]
//! envelopes: the `initialize` result, a tool descriptor, and a
//! `tools/call` result. Field names match the wire protocol exactly (see
//! the `serde(rename)` attributes), so these types serialize as real MCP
//! JSON, not an approximation of it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The MCP protocol version this build speaks.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// One tool as advertised by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// `serverInfo` / `clientInfo`: name and version of one side of the
/// connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// The result of `initialize`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
    pub capabilities: Value,
}

/// One content block of a `tools/call` result. MCP defines several kinds
/// (`text`, `image`, ...); Operant's tools only ever produce `text`, the
/// plain-English replay outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
}

/// The result of `tools/call`: MCP reports a failed tool call as a
/// *successful* JSON-RPC response with `isError: true`, reserving a real
/// JSON-RPC error for protocol-level problems (unknown method, malformed
/// params). See `server::McpServer::dispatch_call` for the split.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

impl CallToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent {
                kind: "text".to_string(),
                text: text.into(),
            }],
            is_error: false,
        }
    }

    pub fn error_text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent {
                kind: "text".to_string(),
                text: text.into(),
            }],
            is_error: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn call_tool_result_serializes_with_mcp_field_names() {
        let result = CallToolResult::text("ok");
        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(
            value,
            json!({ "content": [ { "type": "text", "text": "ok" } ], "isError": false })
        );
    }

    #[test]
    fn mcp_tool_uses_input_schema_camel_case_on_the_wire() {
        let tool = McpTool {
            name: "workflow_x".to_string(),
            description: "does x".to_string(),
            input_schema: json!({ "type": "object" }),
        };
        let value = serde_json::to_value(&tool).unwrap();
        assert_eq!(value["inputSchema"], json!({ "type": "object" }));
        assert!(value.get("input_schema").is_none());
    }
}
