//! The MCP server direction (`docs/specs/mcp.md`): expose a fixed set of
//! already-compiled workflows as MCP tools, one `workflow_<slug>` per
//! workflow, and answer `tools/call` by replaying headless through
//! `operant-replay` -- the same seam the CLI's `run` verb drives, against a
//! fresh mock synthesizer every time. `operant-replay` cannot reach a model
//! or a network call by construction (see its own crate doc), so neither
//! can a `tools/call` reach one through this server.

use std::io;

use operant_gates::EvalContext;
use operant_ir::{RiskClass, Snapshot};
use operant_replay::{CompiledWorkflow, Replayer};
use serde_json::{json, Value};

use super::jsonrpc::{self, error_code};
use super::protocol::{CallToolResult, InitializeResult, McpTool, ServerInfo, PROTOCOL_VERSION};
use super::transport::Transport;

/// Server-side policy. `docs/specs/mcp.md`: "destructive-capable workflows
/// are exposed only if the user enabled 'allow tools to run risky
/// workflows' (off by default)".
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub server_name: String,
    pub server_version: String,
    pub allow_destructive: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            server_name: "operant".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            allow_destructive: false,
        }
    }
}

/// Exposes a fixed set of compiled workflows as MCP tools over whatever
/// [`Transport`] `serve` is given (stdio in production). Stateless aside
/// from that fixed set and the gate snapshot every headless replay
/// evaluates its pre/postconditions against; nothing here is mutated by a
/// `tools/call`, so one server can answer concurrent callers safely (the
/// stdio loop is single-threaded in this build, but nothing here assumes
/// that).
pub struct McpServer {
    config: McpServerConfig,
    workflows: Vec<CompiledWorkflow>,
    /// The gate context for both the pre- and postcondition evaluation of
    /// every headless `tools/call` replay. Headless replay has no live
    /// perceiver -- `crates/replay`'s whole point is zero perception
    /// dependency -- so, exactly like `e2e/golden-path`'s own REPLAY phase
    /// and the CLI's `run` verb, a fixed snapshot fixture stands in for
    /// "the screen after the run."
    gate_snapshot: Snapshot,
}

impl McpServer {
    pub fn new(
        config: McpServerConfig,
        workflows: Vec<CompiledWorkflow>,
        gate_snapshot: Snapshot,
    ) -> Self {
        Self {
            config,
            workflows,
            gate_snapshot,
        }
    }

    fn slug(manifest_name: &str) -> String {
        format!("workflow_{manifest_name}")
    }

    fn exposed_workflows(&self) -> Vec<&CompiledWorkflow> {
        self.workflows
            .iter()
            .filter(|w| {
                self.config.allow_destructive
                    || w.manifest.capabilities.risk_ceiling != RiskClass::Destructive
            })
            .collect()
    }

    fn find(&self, tool_name: &str) -> Option<&CompiledWorkflow> {
        self.exposed_workflows()
            .into_iter()
            .find(|w| Self::slug(&w.manifest.name) == tool_name)
    }

    /// Every tool this server currently advertises: `workflow_<slug>` name,
    /// the manifest's inputs schema as the tool schema, and the manifest's
    /// (already plain-English) description as the tool description.
    pub fn tools(&self) -> Vec<McpTool> {
        self.exposed_workflows()
            .into_iter()
            .map(|w| McpTool {
                name: Self::slug(&w.manifest.name),
                description: w.manifest.description.clone(),
                input_schema: w.manifest.inputs_schema.clone(),
            })
            .collect()
    }

    /// Run one workflow headless against a fresh mock synthesizer and
    /// translate the outcome into MCP's tool result shape ("invoking runs
    /// replay mode headless and returns the outcome plus postcondition
    /// results"). Any string-valued argument is bound as a workflow input
    /// by name; anything else in `arguments` is ignored.
    pub fn invoke(&self, tool_name: &str, arguments: &Value) -> CallToolResult {
        let Some(workflow) = self.find(tool_name) else {
            return CallToolResult::error_text(format!("no such tool `{tool_name}`"));
        };

        let inputs: std::collections::BTreeMap<String, String> = arguments
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect();

        let replayer = Replayer::with_mock();
        let ctx = EvalContext::new().with_snapshot(self.gate_snapshot.clone());
        match replayer.replay_compiled(workflow, &inputs, &ctx, &ctx) {
            Ok(report) => CallToolResult::text(
                json!({
                    "steps_executed": report.steps_executed,
                    "pre": report.pre.iter().map(gate_result_str).collect::<Vec<_>>(),
                    "post": report.post.iter().map(gate_result_str).collect::<Vec<_>>(),
                })
                .to_string(),
            ),
            Err(e) => CallToolResult::error_text(e.to_string()),
        }
    }

    // ---- JSON-RPC dispatch --------------------------------------------

    /// Handle one already-parsed JSON-RPC message. Returns `None` for a
    /// notification (`notifications/initialized` is the only one the
    /// handshake defines from the client; JSON-RPC notifications never get
    /// a response regardless).
    pub fn handle_message(&self, msg: &Value) -> Option<Value> {
        let method = msg.get("method")?.as_str()?;
        let id = msg.get("id").cloned()?;

        let outcome = match method {
            "initialize" => Ok(self.initialize_result()),
            "tools/list" => Ok(json!({ "tools": self.tools() })),
            "tools/call" => self.dispatch_call(msg.get("params").unwrap_or(&Value::Null)),
            other => Err((
                error_code::METHOD_NOT_FOUND,
                format!("unknown method `{other}`"),
            )),
        };

        Some(match outcome {
            Ok(value) => jsonrpc::success(id, value),
            Err((code, message)) => jsonrpc::error_response(id, code, message),
        })
    }

    fn initialize_result(&self) -> Value {
        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            server_info: ServerInfo {
                name: self.config.server_name.clone(),
                version: self.config.server_version.clone(),
            },
            capabilities: json!({ "tools": {} }),
        };
        serde_json::to_value(result).expect("InitializeResult always serializes")
    }

    fn dispatch_call(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).ok_or((
            error_code::INVALID_PARAMS,
            "`tools/call` requires a string `name`".to_string(),
        ))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let result = self.invoke(name, &arguments);
        serde_json::to_value(result).map_err(|e| (error_code::INTERNAL_ERROR, e.to_string()))
    }

    /// The real stdio transport loop ("Server: stdio... transport"): read
    /// one JSON-RPC message at a time, write one response line back for
    /// every request, until the peer closes the connection. This is
    /// `operant mcp`'s whole body.
    pub fn serve(&self, transport: &mut dyn Transport) -> io::Result<()> {
        while let Some(msg) = transport.recv()? {
            if let Some(response) = self.handle_message(&msg) {
                transport.send(&response)?;
            }
        }
        Ok(())
    }
}

fn gate_result_str(r: &operant_ir::GateResult) -> &'static str {
    match r {
        operant_ir::GateResult::Pass => "pass",
        operant_ir::GateResult::Fail => "fail",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            v: 1,
            source: operant_ir::SnapshotSource::Fixture,
            window: operant_ir::WindowInfo {
                hwnd: None,
                process: "notepad.exe".to_string(),
                title: "Untitled - Notepad".to_string(),
                monitor: None,
                dpi_scale: 1.0,
            },
            digest: "d".repeat(64),
            truncated: false,
            captured_ms: None,
            elements: vec![],
        }
    }

    fn manifest(name: &str, risk_ceiling: RiskClass) -> operant_ir::Manifest {
        operant_ir::Manifest {
            v: 1,
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: format!("Does {name}."),
            step_summary: vec![],
            inputs_schema: json!({ "type": "object", "properties": {} }),
            capabilities: operant_ir::Capabilities {
                apps: vec!["notepad.exe".to_string()],
                paths: vec![],
                network: false,
                risk_ceiling,
            },
            gates: vec![],
            min_operant_version: None,
            source_run_id: None,
            dsl: operant_ir::DslRef {
                path: "workflow.ts".to_string(),
                hash: "h".repeat(64),
            },
            signature: None,
        }
    }

    fn workflow(name: &str, risk_ceiling: RiskClass) -> CompiledWorkflow {
        CompiledWorkflow {
            manifest: manifest(name, risk_ceiling),
            actions: vec![],
        }
    }

    #[test]
    fn tool_name_is_workflow_prefixed_and_uses_the_manifest_description_and_schema() {
        let server = McpServer::new(
            McpServerConfig::default(),
            vec![workflow("demo", RiskClass::Write)],
            snapshot(),
        );
        let tools = server.tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "workflow_demo");
        assert_eq!(tools[0].description, "Does demo.");
        assert_eq!(
            tools[0].input_schema,
            json!({ "type": "object", "properties": {} })
        );
    }

    #[test]
    fn destructive_workflows_are_hidden_unless_allowed() {
        let hidden = McpServer::new(
            McpServerConfig::default(),
            vec![workflow("risky", RiskClass::Destructive)],
            snapshot(),
        );
        assert!(hidden.tools().is_empty());

        let shown = McpServer::new(
            McpServerConfig {
                allow_destructive: true,
                ..McpServerConfig::default()
            },
            vec![workflow("risky", RiskClass::Destructive)],
            snapshot(),
        );
        assert_eq!(shown.tools().len(), 1);
    }

    #[test]
    fn unknown_method_is_a_typed_jsonrpc_error_not_a_panic() {
        let server = McpServer::new(McpServerConfig::default(), vec![], snapshot());
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "not/a/method", "params": {} });
        let resp = server.handle_message(&req).expect("still a response");
        assert_eq!(resp["error"]["code"], json!(error_code::METHOD_NOT_FOUND));
    }

    #[test]
    fn a_notification_gets_no_response() {
        let server = McpServer::new(McpServerConfig::default(), vec![], snapshot());
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} });
        assert!(server.handle_message(&note).is_none());
    }

    #[test]
    fn calling_an_unknown_tool_is_a_successful_response_with_is_error_true() {
        let server = McpServer::new(McpServerConfig::default(), vec![], snapshot());
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "workflow_ghost", "arguments": {} } });
        let resp = server.handle_message(&req).expect("a response");
        assert!(resp.get("error").is_none(), "not a protocol-level error");
        assert_eq!(resp["result"]["isError"], json!(true));
    }
}
