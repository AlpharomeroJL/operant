//! Scripted MCP handshake test (C14, FR-O3): `docs/specs/mcp.md`'s "Both
//! directions covered by a scripted handshake test in CI... without a real
//! external server (use an in-process mock peer)."
//!
//! SERVER direction: build an [`McpServer`] over one real compiled
//! workflow -- the shared Notepad fixture, compiled the same way `operant
//! compile` does -- and drive it directly with hand-built JSON-RPC request
//! `Value`s, the same envelopes a real client would send over stdio:
//! `initialize`, `tools/list`, `tools/call`. This half needs no peer at
//! all: the test itself plays the client role.
//!
//! CLIENT direction: [`MockPeer`] is a small, independently coded fake
//! external MCP server (deliberately NOT `McpServer`'s own code, so this
//! half of the test cannot pass merely because the client and server share
//! a bug) exposing one `ping` tool. [`McpClient`] drives the same
//! handshake against it over [`InProcessTransport`] -- no socket, no
//! spawned process -- then its discovered tool is registered as an `mcp`
//! namespace adapter and invoked through `operant_action::AdapterRegistry`,
//! proving the full path: handshake -> tool discovery -> adapter
//! registration -> `adapter_call`.

use std::collections::HashMap;

use operant_action::{Adapter, AdapterRegistry};
use operant_compiler::{compile, Trajectory};
use operant_ir::{ActionKind, RiskClass, Role, Snapshot};
use operant_orchestrator::mcp::{
    InProcessTransport, McpClient, McpClientAdapter, McpServer, McpServerConfig,
};
use operant_replay::CompiledWorkflow;
use serde_json::{json, Value};

const TRAJECTORY: &str = include_str!("../../../contracts/fixtures/trajectory_notepad.json");
const SNAPSHOT: &str = include_str!("../../../contracts/fixtures/snapshot_notepad.json");

/// The shared Notepad fixture, compiled exactly as `operant compile` would.
fn compiled_notepad_workflow() -> CompiledWorkflow {
    let traj: Trajectory = serde_json::from_str(TRAJECTORY).expect("fixture trajectory parses");
    let compilation = compile(&traj).expect("fixture trajectory compiles");
    // `operant_compiler::CompiledWorkflow` and `operant_replay::CompiledWorkflow`
    // are separate types on purpose (see `crates/replay/src/lib.rs`'s own
    // module doc); cross the boundary the way a real deployment would, by
    // round-tripping the JSON they are both defined to share.
    let persisted = serde_json::to_string(&compilation.workflow).unwrap();
    serde_json::from_str(&persisted).expect("compiler output deserializes as replay's own shape")
}

/// The fixture's pre-run snapshot, patched to what a successful run leaves
/// behind, standing in for "the screen after the run" the same way
/// `e2e/golden-path`'s own `notepad_snapshot()` does for the same reason:
/// headless replay has no live perceiver to produce one.
fn notepad_gate_snapshot() -> Snapshot {
    let mut snap: Snapshot = serde_json::from_str(SNAPSHOT).expect("fixture snapshot parses");
    for el in &mut snap.elements {
        if el.role == Role::Document && el.name == "Text editor" {
            el.value = Some("Invoice 2026-07-11 total $142.50".to_string());
        }
    }
    snap
}

// ---- SERVER direction -------------------------------------------------

#[test]
fn server_direction_initialize_lists_and_invokes_the_compiled_workflow_tool() {
    let workflow = compiled_notepad_workflow();
    let expected_tool_name = format!("workflow_{}", workflow.manifest.name);
    let expected_description = workflow.manifest.description.clone();
    let expected_schema = workflow.manifest.inputs_schema.clone();
    let expected_steps_executed = workflow
        .actions
        .iter()
        .filter(|a| a.kind != ActionKind::Assert)
        .count();

    let server = McpServer::new(
        McpServerConfig {
            server_name: "operant-test".to_string(),
            server_version: "1.0.0".to_string(),
            allow_destructive: false,
        },
        vec![workflow],
        notepad_gate_snapshot(),
    );

    // 1. initialize
    let init_req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "clientInfo": { "name": "test-client", "version": "0.0.0" }, "capabilities": {} }
    });
    let init_resp = server
        .handle_message(&init_req)
        .expect("initialize responds");
    assert_eq!(init_resp["id"], json!(1));
    assert!(init_resp.get("error").is_none(), "got {init_resp:?}");
    assert_eq!(
        init_resp["result"]["serverInfo"]["name"],
        json!("operant-test")
    );
    assert_eq!(init_resp["result"]["protocolVersion"], json!("2024-11-05"));

    // notifications/initialized: a notification. Must get NO response.
    let ack = json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} });
    assert!(server.handle_message(&ack).is_none());

    // 2. tools/list
    let list_req = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} });
    let list_resp = server
        .handle_message(&list_req)
        .expect("tools/list responds");
    let tools = list_resp["result"]["tools"]
        .as_array()
        .expect("a tools array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], json!(expected_tool_name));
    assert_eq!(tools[0]["description"], json!(expected_description));
    assert_eq!(tools[0]["inputSchema"], expected_schema);

    // 3. tools/call
    let call_req = json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": expected_tool_name, "arguments": {} }
    });
    let call_resp = server
        .handle_message(&call_req)
        .expect("tools/call responds");
    assert_eq!(call_resp["id"], json!(3));
    assert!(call_resp.get("error").is_none(), "got {call_resp:?}");
    let result = &call_resp["result"];
    assert_eq!(result["isError"], json!(false), "got {result:?}");
    let text = result["content"][0]["text"]
        .as_str()
        .expect("a text content block");
    let outcome: Value = serde_json::from_str(text).expect("the outcome is JSON");
    assert_eq!(outcome["steps_executed"], json!(expected_steps_executed));
    assert_eq!(
        outcome["pre"],
        json!(["pass"]),
        "the pre gate: foreground process is notepad.exe"
    );
    assert_eq!(
        outcome["post"],
        json!(["pass"]),
        "the post gate: the compiler's own postcondition assert"
    );

    // An unrecognized method is a typed JSON-RPC error, not a panic or silence.
    let bad_req = json!({ "jsonrpc": "2.0", "id": 4, "method": "not/a/method", "params": {} });
    let bad_resp = server
        .handle_message(&bad_req)
        .expect("even an error is still a response");
    assert!(bad_resp.get("error").is_some());
}

// ---- CLIENT direction --------------------------------------------------

/// A minimal, independently coded fake external MCP server: one `ping`
/// tool that echoes its `message` argument back prefixed with "pong: ".
/// Not `McpServer` -- this stands in for a third party's server so the
/// client direction is proven against code that does not share the
/// server's own implementation.
struct MockPeer;

impl MockPeer {
    fn handle(msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned()?;
        match msg.get("method").and_then(Value::as_str)? {
            "initialize" => Some(json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": { "name": "mock-external-server", "version": "9.9.9" },
                    "capabilities": { "tools": {} }
                }
            })),
            "tools/list" => Some(json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "tools": [ {
                    "name": "ping",
                    "description": "Replies pong with the given message.",
                    "inputSchema": {
                        "type": "object",
                        "required": ["message"],
                        "properties": { "message": { "type": "string" } }
                    }
                } ] }
            })),
            "tools/call" => {
                let message = msg
                    .pointer("/params/arguments/message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                Some(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "content": [ { "type": "text", "text": format!("pong: {message}") } ], "isError": false }
                }))
            }
            _ => None,
        }
    }
}

#[test]
fn client_direction_handshakes_discovers_and_registers_a_mock_peers_tool_as_an_mcp_adapter() {
    let transport = InProcessTransport::new(MockPeer::handle);
    let client = McpClient::new(transport);

    let adapter =
        McpClientAdapter::connect(client, "operant-test-client", "1.0.0", &HashMap::new())
            .expect("the handshake and tool discovery succeed against the mock peer");

    assert_eq!(adapter.namespace(), "mcp");
    assert_eq!(adapter.verbs().len(), 1);
    assert_eq!(adapter.verbs()[0].name, "ping");
    assert_eq!(
        adapter.verbs()[0].risk_class,
        RiskClass::Write,
        "risk class write by default per docs/specs/mcp.md"
    );

    let mut registry = AdapterRegistry::new();
    registry.register(Box::new(adapter));

    // adapter_call params (namespace/verb/args) map straight onto the
    // discovered tool's own schema and name -- "tool schemas map directly
    // to adapter_call schemas".
    let result = registry
        .call("mcp", "ping", &json!({ "message": "hello" }))
        .expect("the adapter_call round-trips through the registry to the mock peer and back");
    let text = result["content"][0]["text"]
        .as_str()
        .expect("a text content block");
    assert_eq!(text, "pong: hello");

    // The tool's own schema is enforced before the call ever reaches the
    // peer: a payload missing the required `message` is refused locally.
    let err = registry.call("mcp", "ping", &json!({})).unwrap_err();
    assert!(
        matches!(err, operant_action::AdapterError::SchemaValidation { .. }),
        "got {err:?}"
    );
}
