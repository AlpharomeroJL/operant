//! Pass 5: emit.
//!
//! Renders the two artifacts the rest of the system consumes:
//!
//! * a readable TypeScript file over `@operant/sdk` (one statement per step,
//!   the plain-English intent as a leading comment), and
//! * an [`operant_ir::Manifest`] (name, version, inferred inputs schema,
//!   capabilities rolled up from the step needs, and the pre/post gate
//!   bindings) whose `dsl.hash` is the BLAKE3 of the emitted TypeScript bytes.

use operant_ir::{
    Action, ActionKind, Capabilities, DslRef, Gate, GateKind, Manifest, OnFail, RiskClass,
    Selector, WindowMatch,
};
use serde_json::{json, Map, Value};

use crate::pipeline::{risk_ceiling, union_apps, InputDef};

/// The manifest plus the TypeScript source it points at.
pub struct Emitted {
    pub manifest: Manifest,
    pub dsl_source: String,
}

const DSL_PATH: &str = "workflow.ts";
const WORKFLOW_VERSION: &str = "1.0.0";

/// Build the manifest and the TypeScript DSL from the lowered steps.
pub fn emit(
    goal: &str,
    run_id: &str,
    actions: &[Action],
    inputs: &[InputDef],
    post_expr: Option<&Value>,
) -> Emitted {
    let apps = union_apps(actions);
    let name = derive_name(goal, &apps);
    let description = derive_description(goal);
    let step_summary: Vec<String> = actions
        .iter()
        .map(|a| a.intent.clone().unwrap_or_default())
        .collect();

    let dsl_source = render_ts(run_id, goal, &name, &description, inputs, actions);
    let hash = blake3::hash(dsl_source.as_bytes()).to_hex().to_string();

    let manifest = Manifest {
        v: 1,
        name,
        version: WORKFLOW_VERSION.to_string(),
        description,
        step_summary,
        inputs_schema: inputs_schema(inputs),
        capabilities: Capabilities {
            apps: apps.clone(),
            paths: Vec::new(),
            network: false,
            risk_ceiling: risk_ceiling(actions),
        },
        gates: gates(&apps, post_expr),
        min_operant_version: Some(WORKFLOW_VERSION.to_string()),
        source_run_id: Some(run_id.to_string()),
        dsl: DslRef {
            path: DSL_PATH.to_string(),
            hash,
        },
        signature: None,
    };

    Emitted {
        manifest,
        dsl_source,
    }
}

/// The JSON Schema object for the inferred inputs (manifest `inputs_schema`).
pub fn inputs_schema(inputs: &[InputDef]) -> Value {
    let mut props = Map::new();
    for inp in inputs {
        props.insert(inp.name.clone(), inp.schema());
    }
    json!({
        "type": "object",
        "properties": Value::Object(props),
        "additionalProperties": false,
    })
}

/// The workflow-level gate bindings: a precondition that the foreground process
/// is the single app the workflow drives, and the outcome-bearing postcondition.
fn gates(apps: &[String], post_expr: Option<&Value>) -> Vec<Gate> {
    let mut gates = Vec::new();
    if let [app] = apps {
        gates.push(Gate {
            step_ref: None,
            kind: GateKind::Pre,
            expr: json!({
                "op": "equals",
                "left": { "kind": "snapshot_window_process" },
                "right": { "kind": "literal", "value": app },
            }),
            on_fail: OnFail::Halt,
        });
    }
    if let Some(expr) = post_expr {
        gates.push(Gate {
            step_ref: None,
            kind: GateKind::Post,
            expr: expr.clone(),
            on_fail: OnFail::Halt,
        });
    }
    gates
}

// ---- name and description ---------------------------------------------------

fn is_name_stopword(w: &str) -> bool {
    const STOP: &[&str] = &[
        "a", "an", "the", "and", "or", "in", "on", "at", "to", "of", "for", "with", "into", "it",
        "its", "this", "that", "is", "be", "then", "using", "use", "write", "writes", "save",
        "saves", "open", "opens", "close", "click", "type", "press", "enter", "create", "make",
        "add", "set", "go", "run", "fill", "select",
    ];
    STOP.contains(&w)
}

/// Derive the manifest name: the app slug followed by the meaningful nouns of
/// the goal. "Write an invoice note in Notepad and save it" over notepad.exe
/// becomes `notepad-invoice-note`.
pub fn derive_name(goal: &str, apps: &[String]) -> String {
    let app_slug = apps.first().map(|a| app_slug(a)).unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    if !app_slug.is_empty() {
        parts.push(app_slug.clone());
    }
    for tok in goal.split(|c: char| !c.is_ascii_alphanumeric()) {
        let t = tok.to_ascii_lowercase();
        if t.is_empty() || is_name_stopword(&t) || t == app_slug || parts.contains(&t) {
            continue;
        }
        parts.push(t);
    }
    let mut name = parts.join("-");
    if name.is_empty() {
        name = "workflow".to_string();
    }
    name.truncate(64);
    name
}

/// The app basename without extension, lowercased: `notepad.exe` -> `notepad`.
fn app_slug(app: &str) -> String {
    let base = app.rsplit(['/', '\\']).next().unwrap_or(app);
    let stem = base.split('.').next().unwrap_or(base);
    stem.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn derive_description(goal: &str) -> String {
    let g = goal.trim();
    if g.is_empty() {
        return "Compiled workflow.".to_string();
    }
    let mut chars = g.chars();
    let mut out: String = match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    };
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

// ---- TypeScript rendering ---------------------------------------------------

/// Render a Rust string as a TypeScript double-quoted string literal. serde's
/// JSON string encoding is exactly TS-compatible (backslash and quote escaping,
/// no trailing content), so a regex like `\d{2}` round-trips as `\\d{2}`.
fn js_str(s: &str) -> String {
    serde_json::to_string(s).expect("string encodes")
}

fn risk_str(r: RiskClass) -> &'static str {
    match r {
        RiskClass::Read => "read",
        RiskClass::Write => "write",
        RiskClass::Destructive => "destructive",
    }
}

fn render_window(w: &WindowMatch) -> String {
    let mut parts = Vec::new();
    if let Some(p) = &w.process {
        parts.push(format!("process: {}", js_str(p)));
    }
    if let Some(t) = &w.title_pattern {
        parts.push(format!("titlePattern: {}", js_str(t)));
    }
    format!("{{ {} }}", parts.join(", "))
}

fn render_selector(sel: &Selector) -> String {
    match sel {
        Selector::AutomationId { value } => {
            format!("{{ kind: \"automation_id\", value: {} }}", js_str(value))
        }
        Selector::NameRolePath { path } => {
            let segs: Vec<String> = path
                .iter()
                .map(|s| format!("{{ role: {}, name: {} }}", js_str(&s.role), js_str(&s.name)))
                .collect();
            format!(
                "{{ kind: \"name_role_path\", path: [{}] }}",
                segs.join(", ")
            )
        }
        Selector::OrdinalPath { path } => {
            let segs: Vec<String> = path
                .iter()
                .map(|s| format!("{{ role: {}, ordinal: {} }}", js_str(&s.role), s.ordinal))
                .collect();
            format!("{{ kind: \"ordinal_path\", path: [{}] }}", segs.join(", "))
        }
        Selector::Css { value } => format!("{{ kind: \"css\", value: {} }}", js_str(value)),
    }
}

fn is_ident(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn ts_key(key: &str) -> String {
    if is_ident(key) {
        key.to_string()
    } else {
        js_str(key)
    }
}

/// Render a JSON value inline (single line) in TypeScript object-literal style
/// (identifier keys where possible, double-quoted string values).
fn render_inline(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => js_str(s),
        Value::Array(a) => {
            let items: Vec<String> = a.iter().map(render_inline).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(o) => {
            if o.is_empty() {
                return "{}".to_string();
            }
            let items: Vec<String> = o
                .iter()
                .map(|(k, val)| format!("{}: {}", ts_key(k), render_inline(val)))
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
    }
}

/// Render a gate predicate object with each top-level entry on its own line,
/// closing brace at `indent` spaces. Matches the fixture's `expr:` block.
fn render_expr_object(expr: &Value, indent: usize) -> String {
    let Value::Object(o) = expr else {
        return render_inline(expr);
    };
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 2);
    let mut s = String::from("{\n");
    for (k, v) in o {
        s.push_str(&format!("{inner}{}: {},\n", ts_key(k), render_inline(v)));
    }
    s.push_str(&pad);
    s.push('}');
    s
}

fn param_str<'a>(action: &'a Action, key: &str) -> &'a str {
    action.params.get(key).and_then(Value::as_str).unwrap_or("")
}

fn render_step(action: &Action, number: usize) -> String {
    let intent = action.intent.clone().unwrap_or_default();
    let window = action.target.as_ref().and_then(|t| t.window.as_ref());
    let selectors = action
        .target
        .as_ref()
        .map(|t| t.selectors.as_slice())
        .unwrap_or(&[]);

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("    // {number}. {intent}"));

    let ctor = match action.kind {
        ActionKind::Click => "click",
        ActionKind::Type => "type",
        ActionKind::Key => "key",
        ActionKind::Scroll => "scroll",
        ActionKind::Wait => "wait",
        ActionKind::Assert => "assert",
        ActionKind::Drag => "drag",
        ActionKind::AdapterCall => "adapterCall",
    };
    lines.push(format!("    step.{ctor}({{"));
    lines.push(format!("      intent: {},", js_str(&intent)));

    match action.kind {
        ActionKind::Wait => {
            if let Some(w) = window {
                lines.push(format!("      scope: {{ window: {} }},", render_window(w)));
            }
            lines.push(format!("      timeoutMs: {},", action.timeout_ms));
        }
        ActionKind::Assert => {
            if let Some(w) = window {
                lines.push(format!("      window: {},", render_window(w)));
            }
            if let Some(expr) = action.params.get("expr") {
                lines.push(format!("      expr: {},", render_expr_object(expr, 6)));
            }
        }
        _ => {
            if let Some(w) = window {
                lines.push(format!("      window: {},", render_window(w)));
            }
            if !selectors.is_empty() {
                lines.push("      selectors: [".to_string());
                for sel in selectors {
                    lines.push(format!("        {},", render_selector(sel)));
                }
                lines.push("      ],".to_string());
            }
            if action.kind == ActionKind::Type {
                lines.push(format!(
                    "      text: {},",
                    js_str(param_str(action, "text"))
                ));
            }
            if action.kind == ActionKind::Key {
                lines.push(format!(
                    "      combo: {},",
                    js_str(param_str(action, "combo"))
                ));
            }
            if action.kind == ActionKind::Scroll {
                lines.push(format!(
                    "      direction: {},",
                    js_str(param_str(action, "direction"))
                ));
            }
            lines.push(format!(
                "      risk: {},",
                js_str(risk_str(action.risk_class))
            ));
        }
    }

    lines.push("    }),".to_string());
    lines.join("\n")
}

fn render_input(inp: &InputDef) -> String {
    format!(
        "    {}: input.{}({{ default: {}, label: {} }}),",
        inp.name,
        inp.kind.ts_ctor(),
        js_str(&inp.default),
        js_str(&inp.title)
    )
}

fn render_ts(
    run_id: &str,
    goal: &str,
    name: &str,
    description: &str,
    inputs: &[InputDef],
    actions: &[Action],
) -> String {
    let mut s = String::new();
    s.push_str(&format!("// Compiled by Operant from run {run_id}\n"));
    s.push_str(&format!("// Goal: {goal}\n"));
    s.push_str("// This file is the canonical compiler OUTPUT shape: declarative, one step per\n");
    s.push_str("// statement, plain-English intent on every step, zero model calls at replay.\n");
    s.push_str("import { defineWorkflow, step, input } from \"@operant/sdk\";\n\n");
    s.push_str("export default defineWorkflow({\n");
    s.push_str(&format!("  name: {},\n", js_str(name)));
    s.push_str(&format!("  version: {},\n", js_str(WORKFLOW_VERSION)));
    s.push_str(&format!("  description: {},\n", js_str(description)));
    s.push_str("  inputs: {\n");
    for inp in inputs {
        s.push_str(&render_input(inp));
        s.push('\n');
    }
    s.push_str("  },\n");
    s.push_str("  steps: [\n");
    for (i, a) in actions.iter().enumerate() {
        s.push_str(&render_step(a, i + 1));
        s.push('\n');
    }
    s.push_str("  ],\n");
    s.push_str("});\n");
    s
}
