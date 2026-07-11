//! Bare JSON-RPC 2.0 envelope helpers. MCP's wire format is JSON-RPC 2.0
//! messages, one per line, over whichever [`super::transport::Transport`]
//! carries them. This module knows nothing about MCP's own methods
//! (`initialize`, `tools/list`, `tools/call`); see [`super::protocol`] and
//! [`super::server`]/[`super::client`] for that.

use serde_json::{json, Value};

pub const VERSION: &str = "2.0";

/// Standard JSON-RPC 2.0 error codes this module's callers use.
pub mod error_code {
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

/// Build a request envelope: `{ jsonrpc, id, method, params }`.
pub fn request(id: Value, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": VERSION, "id": id, "method": method, "params": params })
}

/// Build a notification envelope: `{ jsonrpc, method, params }`, no `id`.
/// A notification never gets a response, by definition of JSON-RPC.
pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": VERSION, "method": method, "params": params })
}

/// Build a success response envelope: `{ jsonrpc, id, result }`.
pub fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": VERSION, "id": id, "result": result })
}

/// Build an error response envelope: `{ jsonrpc, id, error: { code, message } }`.
pub fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": VERSION, "id": id, "error": { "code": code, "message": message.into() } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_carries_method_id_and_params() {
        let msg = request(json!(7), "tools/list", json!({}));
        assert_eq!(msg["jsonrpc"], json!("2.0"));
        assert_eq!(msg["id"], json!(7));
        assert_eq!(msg["method"], json!("tools/list"));
        assert_eq!(msg["params"], json!({}));
    }

    #[test]
    fn notification_has_no_id() {
        let msg = notification("notifications/initialized", json!({}));
        assert!(msg.get("id").is_none());
        assert_eq!(msg["method"], json!("notifications/initialized"));
    }

    #[test]
    fn success_and_error_responses_share_the_request_id() {
        let ok = success(json!(1), json!({ "tools": [] }));
        assert_eq!(ok["id"], json!(1));
        assert_eq!(ok["result"]["tools"], json!([]));

        let err = error_response(json!(1), error_code::METHOD_NOT_FOUND, "nope");
        assert_eq!(err["id"], json!(1));
        assert_eq!(err["error"]["code"], json!(error_code::METHOD_NOT_FOUND));
        assert_eq!(err["error"]["message"], json!("nope"));
    }
}
