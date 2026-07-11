//! The evaluation context: the world a gate predicate is addressed against.
//!
//! A gate never reaches out on its own; it only reads what the runtime places
//! here: the current perception snapshot, workflow inputs (for `{template}`
//! resolution in filesystem paths), adapter results keyed by step, and an
//! optional base directory that relative filesystem paths resolve under.

use std::collections::BTreeMap;
use std::path::PathBuf;

use operant_ir::Snapshot;
use serde_json::Value as Json;

/// Everything a gate predicate may address. Cheap to build; borrowed by the evaluator.
#[derive(Debug, Default, Clone)]
pub struct EvalContext {
    /// The perception snapshot (window + element tree).
    pub snapshot: Option<Snapshot>,
    /// Workflow inputs, used to resolve `{name}` templates in filesystem paths.
    pub inputs: BTreeMap<String, String>,
    /// Adapter results keyed by the producing step's id.
    pub adapter_results: BTreeMap<String, Json>,
    /// Optional base directory that relative filesystem paths resolve under.
    pub fs_base: Option<PathBuf>,
}

impl EvalContext {
    /// An empty context (no snapshot, no inputs, no adapter results).
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a perception snapshot.
    pub fn with_snapshot(mut self, snapshot: Snapshot) -> Self {
        self.snapshot = Some(snapshot);
        self
    }

    /// Bind one workflow input used for `{template}` resolution.
    pub fn with_input(mut self, key: &str, value: &str) -> Self {
        self.inputs.insert(key.to_string(), value.to_string());
        self
    }

    /// Record an adapter result under the id of the step that produced it.
    pub fn with_adapter_result(mut self, step_ref: &str, result: Json) -> Self {
        self.adapter_results.insert(step_ref.to_string(), result);
        self
    }

    /// Set the base directory relative filesystem paths resolve under.
    pub fn with_fs_base(mut self, base: impl Into<PathBuf>) -> Self {
        self.fs_base = Some(base.into());
        self
    }

    /// Resolve `{name}` templates in `raw` against [`Self::inputs`].
    ///
    /// Unknown placeholders are left verbatim (so an unresolved `{output_path}`
    /// simply names a path that does not exist, and the gate fails cleanly rather
    /// than panicking).
    pub fn resolve_template(&self, raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            if let Some(close) = after.find('}') {
                let key = &after[..close];
                match self.inputs.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        // Leave the placeholder intact.
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            } else {
                // No closing brace; emit the remainder literally.
                out.push('{');
                out.push_str(after);
                rest = "";
            }
        }
        out.push_str(rest);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_resolves_known_and_preserves_unknown() {
        let ctx = EvalContext::new().with_input("output_path", "C:/tmp/out.txt");
        assert_eq!(ctx.resolve_template("{output_path}"), "C:/tmp/out.txt");
        assert_eq!(ctx.resolve_template("pre/{output_path}/post"), "pre/C:/tmp/out.txt/post");
        // Unknown placeholder is preserved verbatim.
        assert_eq!(ctx.resolve_template("{missing}"), "{missing}");
    }
}
