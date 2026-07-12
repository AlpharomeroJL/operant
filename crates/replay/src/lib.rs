//! Deterministic replay executor (C8).
//!
//! Links ONLY against `operant-ir`, `operant-action`, and `operant-gates`: it
//! has no path to any model backend, so zero-model replay is enforced by the
//! crate graph, not a runtime flag. Network is denied by construction; the only
//! side effects a replay can produce flow through the injected
//! [`operant_action::Synthesizer`].
//!
//! A [`Replayer`] takes a [`CompiledWorkflow`] (the compiler's output: a
//! manifest plus ordered actions) and drives [`operant_action::Executor`] step
//! by step with pacing forced to instant. Before the steps it evaluates the
//! manifest's `pre` gates; after them, the `post` gates. `assert` steps are not
//! dispatched to the executor (which refuses them); they are surfaced by the
//! compiler as the postcondition gate and evaluated here through
//! [`operant_gates`].
//!
//! ```
//! use std::collections::BTreeMap;
//! use operant_replay::Replayer;
//! use operant_gates::EvalContext;
//! use operant_ir::{Manifest, Action};
//!
//! # fn demo(manifest: Manifest, actions: Vec<Action>) {
//! let replayer = Replayer::with_mock();
//! let inputs = BTreeMap::new(); // fall back to the manifest's input defaults
//! let pre = EvalContext::new();
//! let post = EvalContext::new();
//! let _ = replayer.replay(&manifest, &actions, &inputs, &pre, &post);
//! # }
//! ```

pub mod compose;

use std::collections::BTreeMap;

use operant_action::{AdapterRegistry, Executor, MockSynthesizer, NoopSleeper, Synthesizer};
use operant_gates::{evaluate_gate, EvalContext, GateError};
use operant_ir::{Action, ActionKind, GateKind, GateResult, Manifest, Pace};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// The compiler output the replay executor consumes: a manifest plus the
/// ordered actions. Structurally identical to `operant_compiler::CompiledWorkflow`
/// and JSON-compatible with it, defined here so the replay crate need not depend
/// on the compiler (which would drag in the recorder and break the backend-free
/// crate graph).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledWorkflow {
    pub manifest: Manifest,
    pub actions: Vec<Action>,
}

/// The outcome of a replay: how many steps dispatched, and the pre/post gate
/// results in order.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayReport {
    pub steps_executed: usize,
    pub pre: Vec<GateResult>,
    pub post: Vec<GateResult>,
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("precondition gate #{index} failed; halting before any step ran")]
    Precondition { index: usize },
    #[error("postcondition gate #{index} failed after replay")]
    Postcondition { index: usize },
    #[error("click step `{action_id}` has no resolved point (no cached coordinates to replay)")]
    Unresolved { action_id: String },
    #[error(transparent)]
    Action(#[from] operant_action::ActionError),
    #[error(transparent)]
    Gate(#[from] GateError),
}

/// Drives a [`CompiledWorkflow`] deterministically through the action layer.
pub struct Replayer<S: Synthesizer> {
    exec: Executor<S>,
}

impl Replayer<MockSynthesizer> {
    /// A replayer backed by the deterministic in-memory [`MockSynthesizer`],
    /// the backend every test drives.
    pub fn with_mock() -> Self {
        Self::new(MockSynthesizer::new())
    }
}

impl<S: Synthesizer> Replayer<S> {
    /// Build a replayer over any [`Synthesizer`]. Pacing is instant: the
    /// executor is installed with a [`NoopSleeper`], so retry backoff and the
    /// degraded wait sleep cost no wall-clock time.
    pub fn new(synth: S) -> Self {
        Self {
            exec: Executor::new(synth).with_sleeper(Box::new(NoopSleeper)),
        }
    }

    /// Build a replayer with a caller-supplied [`AdapterRegistry`], for
    /// compiled workflows that dispatch `adapter_call` steps (C5's `browser`
    /// namespace, for one: it has no `coords_last_known` to resolve, so it
    /// replays through a registered adapter rather than the synthesizer's
    /// point-based click/type). Same instant pacing as [`Self::new`].
    pub fn with_adapters(synth: S, adapters: AdapterRegistry) -> Self {
        Self {
            exec: Executor::with_adapters(synth, adapters).with_sleeper(Box::new(NoopSleeper)),
        }
    }

    /// The underlying synthesizer, for inspecting recorded calls in tests.
    pub fn synthesizer(&self) -> &S {
        self.exec.synthesizer()
    }

    /// Replay a [`CompiledWorkflow`]. Convenience over [`Self::replay`].
    pub fn replay_compiled(
        &self,
        wf: &CompiledWorkflow,
        inputs: &BTreeMap<String, String>,
        pre: &EvalContext,
        post: &EvalContext,
    ) -> Result<ReplayReport, ReplayError> {
        self.replay(&wf.manifest, &wf.actions, inputs, pre, post)
    }

    /// Replay the manifest and ordered actions.
    ///
    /// `inputs` override the manifest's schema defaults for `{template}`
    /// substitution in step text. `pre`/`post` are the gate evaluation contexts
    /// (the perception snapshots the runtime supplies) for the workflow's
    /// pre/post conditions.
    pub fn replay(
        &self,
        manifest: &Manifest,
        actions: &[Action],
        inputs: &BTreeMap<String, String>,
        pre: &EvalContext,
        post: &EvalContext,
    ) -> Result<ReplayReport, ReplayError> {
        let bindings = merge_defaults(manifest, inputs);

        // Preconditions first: a failing `halt` gate stops before any side effect.
        let pre_results = eval_gates(manifest, GateKind::Pre, pre)?;
        if let Some(index) = first_failure(&pre_results) {
            return Err(ReplayError::Precondition { index });
        }

        let mut steps_executed = 0usize;
        for action in actions {
            // `assert` is never dispatched to the executor (it refuses the
            // kind); the compiler already surfaced it as the postcondition gate.
            if action.kind == ActionKind::Assert {
                continue;
            }
            let step = self.prepare(action, &bindings);
            let resolved = step
                .target
                .as_ref()
                .and_then(|t| t.coords_last_known.clone());
            if step.kind == ActionKind::Click && resolved.is_none() {
                return Err(ReplayError::Unresolved {
                    action_id: step.id.clone(),
                });
            }
            self.exec.execute(&step, resolved.as_ref(), None)?;
            steps_executed += 1;
        }

        // Postconditions last.
        let post_results = eval_gates(manifest, GateKind::Post, post)?;
        if let Some(index) = first_failure(&post_results) {
            return Err(ReplayError::Postcondition { index });
        }

        Ok(ReplayReport {
            steps_executed,
            pre: pre_results,
            post: post_results,
        })
    }

    /// Prepare one action for deterministic dispatch: force instant pacing,
    /// substitute input bindings into templated text, and neutralize a wait's
    /// scope (this backend-free executor cannot poll a scope, so a wait is a
    /// deterministic no-op and must not focus a window).
    fn prepare(&self, action: &Action, bindings: &BTreeMap<String, String>) -> Action {
        let mut a = action.clone();
        a.pace = Pace::Instant;
        if let Some(text) = a.params.get("text").and_then(Value::as_str) {
            let substituted = substitute(text, bindings);
            a.params
                .insert("text".to_string(), Value::String(substituted));
        }
        if a.kind == ActionKind::Wait {
            a.target = None;
        }
        a
    }
}

fn eval_gates(
    manifest: &Manifest,
    kind: GateKind,
    ctx: &EvalContext,
) -> Result<Vec<GateResult>, ReplayError> {
    manifest
        .gates
        .iter()
        .filter(|g| g.kind == kind)
        .map(|g| evaluate_gate(g, ctx).map_err(ReplayError::Gate))
        .collect()
}

fn first_failure(results: &[GateResult]) -> Option<usize> {
    results.iter().position(|r| *r == GateResult::Fail)
}

/// Overlay caller inputs on the manifest's schema defaults.
fn merge_defaults(
    manifest: &Manifest,
    inputs: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(props) = manifest
        .inputs_schema
        .get("properties")
        .and_then(Value::as_object)
    {
        for (name, schema) in props {
            if let Some(default) = schema.get("default").and_then(Value::as_str) {
                out.insert(name.clone(), default.to_string());
            }
        }
    }
    for (k, v) in inputs {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// Replace `{name}` placeholders with their bound values, leaving unknown
/// placeholders (and every other character, including a literal `$`) verbatim.
fn substitute(text: &str, bindings: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        if let Some(close) = after.find('}') {
            let key = &after[..close];
            match bindings.get(key) {
                Some(v) => out.push_str(v),
                None => {
                    out.push('{');
                    out.push_str(key);
                    out.push('}');
                }
            }
            rest = &after[close + 1..];
        } else {
            out.push('{');
            out.push_str(after);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-replay";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_present() {
        assert_eq!(CRATE, "operant-replay");
    }

    #[test]
    fn substitute_fills_placeholders_and_preserves_the_dollar() {
        let mut b = BTreeMap::new();
        b.insert("invoice_date".to_string(), "2026-07-11".to_string());
        b.insert("amount".to_string(), "142.50".to_string());
        assert_eq!(
            substitute("Invoice {invoice_date} total ${amount}", &b),
            "Invoice 2026-07-11 total $142.50"
        );
        // Unknown placeholder is left intact.
        assert_eq!(substitute("{missing}", &b), "{missing}");
    }

    #[test]
    fn replay_crate_is_backend_free() {
        // The shipped crate must link nothing that can reach a model backend or
        // a socket. This mirrors scripts/check_airgap.mjs and extends it: the
        // whole point of the replay lane is that zero-model replay is a property
        // of the crate graph.
        let toml = include_str!("../Cargo.toml");
        let (runtime, _dev) = toml.split_once("[dev-dependencies]").unwrap_or((toml, ""));
        for banned in [
            "operant-orchestrator",
            "operant-scheduler",
            "operant-registry",
            "reqwest",
            "tokio",
            "hyper",
            "reticle",
        ] {
            assert!(
                !runtime.contains(banned),
                "replay runtime deps must not include `{banned}`"
            );
        }
    }
}
