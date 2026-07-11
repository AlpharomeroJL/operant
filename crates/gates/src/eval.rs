//! The gate expression evaluator.
//!
//! Evaluates the JSON predicate AST (`Gate::expr`) over an [`EvalContext`].
//! Operators: `exists`, `equals`, `matches` (anchored regex), `count`, `sum`,
//! `within_tolerance`, `and`, `or`, `not`. Query kinds address the perception
//! snapshot (`snapshot_window_process`, `snapshot_element`,
//! `snapshot_element_value`), the filesystem (`fs`), adapter results
//! (`adapter_result`), and inline `literal`s; `count`/`sum` may also appear in a
//! value position.

use operant_ir::{Element, Gate, GateResult, Role};
use regex::Regex;
use serde_json::Value as Json;

use crate::context::EvalContext;
use crate::error::GateError;
use crate::value::{val_equals, Val};

/// Evaluate a single gate to [`GateResult::Pass`] or [`GateResult::Fail`].
///
/// Returns [`Err`] only for a structurally malformed predicate; a well-formed
/// predicate that does not hold is a `Fail`, never an error.
pub fn evaluate_gate(gate: &Gate, ctx: &EvalContext) -> Result<GateResult, GateError> {
    let val = eval(&gate.expr, ctx)?;
    Ok(if val.truthy() { GateResult::Pass } else { GateResult::Fail })
}

/// Evaluate every gate, returning one result (or error) per gate, in order.
pub fn evaluate_gates(
    gates: &[Gate],
    ctx: &EvalContext,
) -> Vec<Result<GateResult, GateError>> {
    gates.iter().map(|g| evaluate_gate(g, ctx)).collect()
}

/// Evaluate any predicate node (dispatches on `op` then `kind`).
pub fn eval(node: &Json, ctx: &EvalContext) -> Result<Val, GateError> {
    let obj = node
        .as_object()
        .ok_or_else(|| GateError::NotANode(node.to_string()))?;

    if let Some(op) = obj.get("op").and_then(Json::as_str) {
        eval_op(op, obj, ctx)
    } else if let Some(kind) = obj.get("kind").and_then(Json::as_str) {
        eval_kind(kind, obj, ctx)
    } else {
        Err(GateError::NotANode(node.to_string()))
    }
}

type Obj = serde_json::Map<String, Json>;

fn field<'a>(obj: &'a Obj, op: &str, name: &'static str) -> Result<&'a Json, GateError> {
    obj.get(name).ok_or_else(|| GateError::MissingField {
        op: op.to_string(),
        field: name,
    })
}

fn eval_op(op: &str, obj: &Obj, ctx: &EvalContext) -> Result<Val, GateError> {
    match op {
        "and" => {
            let args = field(obj, op, "args")?
                .as_array()
                .ok_or_else(|| GateError::MissingField { op: op.into(), field: "args" })?;
            let mut all = true;
            for a in args {
                if !eval(a, ctx)?.truthy() {
                    all = false;
                }
            }
            Ok(Val::Bool(all))
        }
        "or" => {
            let args = field(obj, op, "args")?
                .as_array()
                .ok_or_else(|| GateError::MissingField { op: op.into(), field: "args" })?;
            let mut any = false;
            for a in args {
                if eval(a, ctx)?.truthy() {
                    any = true;
                }
            }
            Ok(Val::Bool(any))
        }
        "not" => {
            let arg = field(obj, op, "arg")?;
            Ok(Val::Bool(!eval(arg, ctx)?.truthy()))
        }
        "exists" => {
            let query = field(obj, op, "query")?;
            Ok(Val::Bool(exists(query, ctx)?))
        }
        "equals" => {
            let left = eval(field(obj, op, "left")?, ctx)?;
            let right = eval(field(obj, op, "right")?, ctx)?;
            Ok(Val::Bool(val_equals(&left, &right)))
        }
        "matches" => {
            let query = field(obj, op, "query")?;
            let regex = field(obj, op, "regex")?
                .as_str()
                .ok_or_else(|| GateError::MissingField { op: op.into(), field: "regex" })?;
            let val = eval(query, ctx)?;
            let text = val.as_str().unwrap_or("");
            Ok(Val::Bool(anchored_match(regex, text)?))
        }
        "count" => {
            let n = count(field(obj, op, "query")?, ctx)?;
            finalize_numeric(op, n, obj)
        }
        "sum" => {
            let s = sum(field(obj, op, "query")?, ctx)?;
            finalize_numeric(op, s, obj)
        }
        "within_tolerance" => {
            let left = eval(field(obj, op, "left")?, ctx)?;
            let right = eval(field(obj, op, "right")?, ctx)?;
            let l = numeric(op, &left)?;
            let r = numeric(op, &right)?;
            let tol = obj.get("tolerance").and_then(Json::as_f64).unwrap_or(0.0);
            Ok(Val::Bool((l - r).abs() <= tol + 1e-12))
        }
        other => Err(GateError::UnknownOp(other.to_string())),
    }
}

/// `count`/`sum` used as operators may carry a comparison (`equals`) that turns
/// the reduction into a boolean; otherwise they yield the raw number.
fn finalize_numeric(op: &str, n: f64, obj: &Obj) -> Result<Val, GateError> {
    if let Some(target) = obj.get("equals") {
        let t = Val::from_json(target)
            .as_number()
            .ok_or_else(|| GateError::NotNumeric { op: op.into(), found: target.to_string() })?;
        Ok(Val::Bool((n - t).abs() < 1e-9))
    } else {
        Ok(Val::Num(n))
    }
}

fn numeric(op: &str, v: &Val) -> Result<f64, GateError> {
    v.as_number()
        .ok_or_else(|| GateError::NotNumeric { op: op.to_string(), found: format!("{v:?}") })
}

fn eval_kind(kind: &str, obj: &Obj, ctx: &EvalContext) -> Result<Val, GateError> {
    match kind {
        "literal" => Ok(Val::from_json(obj.get("value").unwrap_or(&Json::Null))),
        "snapshot_window_process" => Ok(ctx
            .snapshot
            .as_ref()
            .map(|s| Val::Str(s.window.process.clone()))
            .unwrap_or(Val::Null)),
        "snapshot_element" => {
            // As a value: the set of matching elements (used by exists/count via
            // their own paths; here for completeness).
            let els = matching_elements(obj, ctx)
                .into_iter()
                .map(element_to_val)
                .collect();
            Ok(Val::Array(els))
        }
        "snapshot_element_value" => {
            let v = first_matching_element(obj, ctx)
                .and_then(|e| e.value.clone())
                .map(Val::Str)
                .unwrap_or(Val::Null);
            Ok(v)
        }
        "count" => Ok(Val::Num(count(field(obj, "count", "query")?, ctx)?)),
        "sum" => Ok(Val::Num(sum(field(obj, "sum", "query")?, ctx)?)),
        "fs" => {
            // As a value: the file size when present, else Null.
            match fs_size(obj, ctx) {
                Some(sz) => Ok(Val::Num(sz as f64)),
                None => Ok(Val::Null),
            }
        }
        "adapter_result" => Ok(resolve_adapter(obj, ctx)),
        other => Err(GateError::UnknownKind(other.to_string())),
    }
}

// ---- exists over the several query kinds -----------------------------------

fn exists(query: &Json, ctx: &EvalContext) -> Result<bool, GateError> {
    let obj = query
        .as_object()
        .ok_or_else(|| GateError::NotANode(query.to_string()))?;
    match obj.get("kind").and_then(Json::as_str) {
        Some("snapshot_element") => Ok(!matching_elements(obj, ctx).is_empty()),
        Some("snapshot_element_value") => {
            Ok(first_matching_element(obj, ctx).and_then(|e| e.value.as_ref()).is_some())
        }
        Some("snapshot_window_process") => {
            Ok(ctx.snapshot.as_ref().is_some_and(|s| !s.window.process.is_empty()))
        }
        Some("fs") => fs_ok(obj, ctx),
        Some("adapter_result") => Ok(resolve_adapter(obj, ctx).present()),
        _ => Ok(eval(query, ctx)?.present()),
    }
}

// ---- snapshot element addressing -------------------------------------------

fn role_name(el_role: Role) -> String {
    // Reuse the serde `rename_all = "lowercase"` mapping for a stable string.
    serde_json::to_value(el_role)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn matching_elements<'a>(obj: &Obj, ctx: &'a EvalContext) -> Vec<&'a Element> {
    let Some(snap) = ctx.snapshot.as_ref() else {
        return Vec::new();
    };
    let want_role = obj.get("role").and_then(Json::as_str);
    let want_name = obj.get("name").and_then(Json::as_str);
    snap.elements
        .iter()
        .filter(|e| match want_role {
            Some(r) => role_name(e.role).eq_ignore_ascii_case(r),
            None => true,
        })
        .filter(|e| match want_name {
            Some("*") | None => true,
            Some(n) => e.name == n,
        })
        .collect()
}

fn first_matching_element<'a>(obj: &Obj, ctx: &'a EvalContext) -> Option<&'a Element> {
    matching_elements(obj, ctx).into_iter().next()
}

fn element_to_val(e: &Element) -> Val {
    serde_json::to_value(e)
        .ok()
        .map(|j| Val::from_json(&j))
        .unwrap_or(Val::Null)
}

// ---- count / sum reductions -------------------------------------------------

fn count(query: &Json, ctx: &EvalContext) -> Result<f64, GateError> {
    let obj = query
        .as_object()
        .ok_or_else(|| GateError::NotANode(query.to_string()))?;
    match obj.get("kind").and_then(Json::as_str) {
        Some("snapshot_element") => Ok(matching_elements(obj, ctx).len() as f64),
        Some("adapter_result") => Ok(match resolve_adapter(obj, ctx) {
            Val::Array(a) => a.len() as f64,
            Val::Null => 0.0,
            _ => 1.0,
        }),
        _ => Ok(match eval(query, ctx)? {
            Val::Array(a) => a.len() as f64,
            Val::Null => 0.0,
            _ => 1.0,
        }),
    }
}

fn sum(query: &Json, ctx: &EvalContext) -> Result<f64, GateError> {
    let val = eval(query, ctx)?;
    Ok(match val {
        Val::Array(items) => items.iter().filter_map(Val::as_number).sum(),
        other => other.as_number().unwrap_or(0.0),
    })
}

// ---- adapter result field addressing ---------------------------------------

fn resolve_adapter(obj: &Obj, ctx: &EvalContext) -> Val {
    let step_ref = obj.get("step_ref").and_then(Json::as_str).unwrap_or("");
    let field = obj.get("field").and_then(Json::as_str).unwrap_or("");
    let Some(result) = ctx.adapter_results.get(step_ref) else {
        return Val::Null;
    };
    let segs = parse_field_path(field);
    resolve_path(result, &segs)
}

/// A field-path segment: a key plus whether it projects across an array (`[]`).
struct Seg {
    key: String,
    project: bool,
}

fn parse_field_path(field: &str) -> Vec<Seg> {
    if field.is_empty() {
        return Vec::new();
    }
    field
        .split('.')
        .map(|part| {
            if let Some(base) = part.strip_suffix("[]") {
                Seg { key: base.to_string(), project: true }
            } else {
                Seg { key: part.to_string(), project: false }
            }
        })
        .collect()
}

fn resolve_path(cur: &Json, segs: &[Seg]) -> Val {
    let Some((seg, rest)) = segs.split_first() else {
        return Val::from_json(cur);
    };
    let child = cur.get(&seg.key).unwrap_or(&Json::Null);
    if seg.project {
        match child.as_array() {
            Some(arr) => Val::Array(arr.iter().map(|item| resolve_path(item, rest)).collect()),
            None => Val::Array(Vec::new()),
        }
    } else {
        resolve_path(child, rest)
    }
}

// ---- filesystem addressing --------------------------------------------------

fn fs_path(obj: &Obj, ctx: &EvalContext) -> std::path::PathBuf {
    let raw = obj.get("path").and_then(Json::as_str).unwrap_or("");
    let resolved = ctx.resolve_template(raw);
    let p = std::path::PathBuf::from(&resolved);
    match &ctx.fs_base {
        Some(base) if p.is_relative() => base.join(p),
        _ => p,
    }
}

/// The size of an addressed file, if it exists as a file.
fn fs_size(obj: &Obj, ctx: &EvalContext) -> Option<u64> {
    let p = fs_path(obj, ctx);
    std::fs::metadata(&p).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// `exists` semantics for a filesystem query: the path must exist and satisfy
/// any `min_size` floor and any `hash` (BLAKE3 hex) constraint.
fn fs_ok(obj: &Obj, ctx: &EvalContext) -> Result<bool, GateError> {
    let p = fs_path(obj, ctx);
    let Ok(meta) = std::fs::metadata(&p) else {
        return Ok(false);
    };
    if let Some(min) = obj.get("min_size").and_then(Json::as_u64) {
        if meta.len() < min {
            return Ok(false);
        }
    }
    // `hash`/`blake3`: full-content BLAKE3 comparison, case-insensitive hex.
    let want_hash = obj
        .get("hash")
        .or_else(|| obj.get("blake3"))
        .and_then(Json::as_str);
    if let Some(want) = want_hash {
        if !meta.is_file() {
            return Ok(false);
        }
        let Ok(bytes) = std::fs::read(&p) else {
            return Ok(false);
        };
        let got = blake3::hash(&bytes).to_hex();
        if !got.as_str().eq_ignore_ascii_case(want) {
            return Ok(false);
        }
    }
    Ok(true)
}

// ---- regex ------------------------------------------------------------------

/// Anchored full-string match. The pattern is wrapped so it must span the whole
/// input, regardless of whether the author already anchored it.
fn anchored_match(pattern: &str, text: &str) -> Result<bool, GateError> {
    let anchored = format!("^(?:{pattern})$");
    let re = Regex::new(&anchored).map_err(|e| GateError::Regex {
        pattern: anchored.clone(),
        reason: e.to_string(),
    })?;
    Ok(re.is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anchored_match_requires_full_span() {
        assert!(anchored_match("abc", "abc").unwrap());
        // Unanchored substring must NOT match under anchored semantics.
        assert!(!anchored_match("abc", "xabcx").unwrap());
        // Author-supplied anchors are tolerated (redundant, still correct).
        assert!(anchored_match("^abc$", "abc").unwrap());
    }

    #[test]
    fn adapter_array_projection_and_sum() {
        let ctx = EvalContext::new().with_adapter_result(
            "s4",
            json!({ "exit_code": 0, "rows": [{ "amount": 100.0 }, { "amount": 42.5 }] }),
        );
        let sum_expr = json!({
            "op": "sum",
            "query": { "kind": "adapter_result", "step_ref": "s4", "field": "rows[].amount" },
            "equals": 142.5
        });
        assert_eq!(eval(&sum_expr, &ctx).unwrap(), Val::Bool(true));

        let ec = json!({
            "op": "equals",
            "left": { "kind": "adapter_result", "step_ref": "s4", "field": "exit_code" },
            "right": { "kind": "literal", "value": 0 }
        });
        assert_eq!(eval(&ec, &ctx).unwrap(), Val::Bool(true));
    }

    #[test]
    fn unknown_operator_is_typed_error() {
        let bad = json!({ "op": "xor", "args": [] });
        assert_eq!(eval(&bad, &EvalContext::new()), Err(GateError::UnknownOp("xor".into())));
    }
}
