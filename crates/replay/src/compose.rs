//! Workflow composition (C22 / FR-O5): a `callWorkflow` step so a compiled
//! workflow can call another as a step.
//!
//! A `callWorkflow` step is an `adapter_call` action targeting the
//! `workflow.call` verb, matching the shape `docs/ARCHITECTURE.md`
//! ("Composition") and `docs/specs/compiler.md` describe: "an `adapter_call`
//! kind targeting another workflow." No new [`operant_ir::ActionKind`]
//! variant is needed; [`as_call_workflow`] recognizes the convention and
//! [`call_workflow_action`] builds it, so the encoding lives in one place.
//!
//! [`CompositionExecutor::call`] loads the callee through a [`WorkflowLoader`],
//! computes the effective capability set as the intersection of the caller's
//! and callee's declared grants (`operant_ir::Capabilities::intersect`), and
//! refuses before anything runs if that intersection cannot cover what the
//! callee itself declares it needs. It then checks every one of the callee's
//! own actions against the effective set before dispatching any of them, so
//! a callee can never reach past the intersection even if one of its steps
//! (compiler drift, or a hand authored DSL) targets more than its own
//! manifest declared. The callee then replays through the same
//! [`Replayer`] (and so the same synthesizer session) as the caller.
//!
//! [`CompositionNode`] is a small rendering summary of one composition edge,
//! for the plain-English renderer (U4A, later) to inline the callee as a
//! collapsible group under the calling step.
//!
//! Nested composition (a callee that itself contains a `callWorkflow` step)
//! is out of scope for v0: the callee replays through the plain
//! [`Replayer::replay_compiled`] path, which dispatches an `adapter_call`
//! action through the ordinary action layer rather than recursing into this
//! module. `docs/ROADMAP.md` notes composition graduating past v0 later.

use std::collections::BTreeMap;
use std::path::Path;

use operant_action::Synthesizer;
use operant_gates::EvalContext;
use operant_ir::{Action, ActionKind, Capabilities, Grounding, Pace, Retry, RiskClass};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{CompiledWorkflow, ReplayError, ReplayReport, Replayer};

/// Adapter-call namespace a compiled action must carry to be recognized as a
/// workflow call.
pub const CALL_WORKFLOW_NAMESPACE: &str = "workflow";
/// Adapter-call verb a compiled action must carry to be recognized as a
/// workflow call.
pub const CALL_WORKFLOW_VERB: &str = "call";

/// A `callWorkflow` step, decoded from an `adapter_call` action's params.
#[derive(Debug, Clone, PartialEq)]
pub struct CallWorkflowStep {
    /// The id of the parent action this step was decoded from.
    pub action_id: String,
    /// Whatever reference string names the callee; a [`WorkflowLoader`]
    /// decides how to resolve it (name, `name@version`, path, registry id).
    pub callee: String,
    /// Static inputs the parent step passes to the callee, overlaying the
    /// callee's own manifest defaults the same way top-level replay inputs
    /// do.
    pub inputs: BTreeMap<String, String>,
}

/// Build the `adapter_call` action IR for a `callWorkflow` step targeting
/// `callee`. `risk_class` is the risk of the call step itself, same as any
/// other compiled step; the risk that actually gates composition is the
/// callee's own manifest `risk_ceiling`, enforced through the effective
/// capability set, not this value.
pub fn call_workflow_action(
    id: impl Into<String>,
    callee: impl Into<String>,
    inputs: BTreeMap<String, String>,
    risk_class: RiskClass,
) -> Action {
    let mut args = serde_json::Map::new();
    args.insert("workflow".to_string(), Value::String(callee.into()));
    if !inputs.is_empty() {
        let mut inputs_obj = serde_json::Map::new();
        for (k, v) in inputs {
            inputs_obj.insert(k, Value::String(v));
        }
        args.insert("inputs".to_string(), Value::Object(inputs_obj));
    }

    let mut params = serde_json::Map::new();
    params.insert(
        "namespace".to_string(),
        Value::String(CALL_WORKFLOW_NAMESPACE.to_string()),
    );
    params.insert("verb".to_string(), Value::String(CALL_WORKFLOW_VERB.to_string()));
    params.insert("args".to_string(), Value::Object(args));

    Action {
        v: 1,
        id: id.into(),
        kind: ActionKind::AdapterCall,
        intent: None,
        target: None,
        params,
        pace: Pace::default(),
        risk_class,
        irreversible: false,
        grounding: Grounding::Adapter,
        timeout_ms: 5000,
        retry: Retry::default(),
    }
}

/// Read a `callWorkflow` step out of an action. `None` for any action that is
/// not an `adapter_call` targeting `workflow.call`, so a caller can scan a
/// mixed action list freely without pre-filtering by kind.
pub fn as_call_workflow(action: &Action) -> Option<CallWorkflowStep> {
    if action.kind != ActionKind::AdapterCall {
        return None;
    }
    if action.params.get("namespace").and_then(Value::as_str) != Some(CALL_WORKFLOW_NAMESPACE) {
        return None;
    }
    if action.params.get("verb").and_then(Value::as_str) != Some(CALL_WORKFLOW_VERB) {
        return None;
    }
    let args = action.params.get("args").and_then(Value::as_object)?;
    let callee = args.get("workflow").and_then(Value::as_str)?.to_string();

    let mut inputs = BTreeMap::new();
    if let Some(obj) = args.get("inputs").and_then(Value::as_object) {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                inputs.insert(k.clone(), s.to_string());
            }
        }
    }

    Some(CallWorkflowStep {
        action_id: action.id.clone(),
        callee,
        inputs,
    })
}

/// Resolves a `callWorkflow` step's callee reference to its compiled
/// workflow. The replay crate has no filesystem or registry access of its
/// own (it stays backend-free by design), so composition is generic over how
/// a workflow is found.
pub trait WorkflowLoader {
    /// Look up the compiled workflow `callee` names. `None` when unknown.
    fn load(&self, callee: &str) -> Option<CompiledWorkflow>;
}

/// A [`WorkflowLoader`] over an in-memory table, keyed by whatever string a
/// `callWorkflow` step's `callee` field carries. The natural loader for
/// tests, and for a caller that has already resolved every dependency of a
/// workflow graph before replay begins.
#[derive(Debug, Default, Clone)]
pub struct MapLoader(pub BTreeMap<String, CompiledWorkflow>);

impl MapLoader {
    /// An empty loader.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Register `workflow` under `callee` and return `self`, for a compact
    /// builder chain in fixtures.
    pub fn with(mut self, callee: impl Into<String>, workflow: CompiledWorkflow) -> Self {
        self.0.insert(callee.into(), workflow);
        self
    }
}

impl WorkflowLoader for MapLoader {
    fn load(&self, callee: &str) -> Option<CompiledWorkflow> {
        self.0.get(callee).cloned()
    }
}

/// Rendering summary of one composition edge: a parent step that calls a
/// child workflow. The plain-English renderer (U4A, later) can use this to
/// inline the callee as a collapsible group under the `callWorkflow` step,
/// without re-loading the child itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositionNode {
    /// The `callWorkflow` action's id in the parent's action list.
    pub action_id: String,
    pub parent_name: String,
    pub parent_version: String,
    pub child_name: String,
    pub child_version: String,
    /// The callee's own plain-English step summaries, carried over so the
    /// renderer does not need to load the child a second time.
    pub child_step_summary: Vec<String>,
    /// The capability set the callee actually ran under: the intersection of
    /// the caller's and callee's declared grants.
    pub effective_capabilities: Capabilities,
}

/// Why composition was refused, typed so a caller can render a plain
/// language explanation without matching on strings.
#[derive(Debug, Error)]
pub enum CompositionError {
    /// The `callWorkflow` step's callee reference did not resolve through
    /// the [`WorkflowLoader`].
    #[error("callWorkflow step `{action_id}` in `{parent_name}` references an unknown workflow `{callee}`")]
    UnknownCallee {
        action_id: String,
        parent_name: String,
        callee: String,
    },

    /// Refuse-at-load: the intersection of the caller's and callee's grants
    /// does not cover everything the callee itself declares it needs. Raised
    /// before any step of the callee runs.
    #[error(
        "`{parent_name}` cannot call `{child_name}`: `{parent_name}` does not grant everything \
         `{child_name}` needs to run, so the effective capability set (the intersection of both \
         workflows' grants) falls short: {reasons}"
    )]
    InsufficientCapabilities {
        parent_name: String,
        child_name: String,
        /// Plain-language reasons, one per unmet dimension, joined for display.
        reasons: String,
    },

    /// One of the callee's own actions would reach past the effective
    /// capability set even though the callee's manifest declared narrower
    /// needs. Raised before any of the callee's actions dispatch.
    #[error(
        "`{child_name}` step `{action_id}` would reach past what `{parent_name}` and `{child_name}` \
         both grant (the effective capability set): {reason}"
    )]
    ExceedsEffective {
        action_id: String,
        parent_name: String,
        child_name: String,
        reason: String,
    },

    /// The callee's own manifest gates or action dispatch failed once
    /// composition actually started running it.
    #[error("composed replay of `{child_name}` failed: {source}")]
    Replay {
        child_name: String,
        #[source]
        source: ReplayError,
    },
}

/// Runs `callWorkflow` steps: loads the callee, computes and checks the
/// effective capability set, and replays the callee through the same
/// [`Replayer`] as the caller.
pub struct CompositionExecutor<'a, S: Synthesizer> {
    replayer: &'a Replayer<S>,
}

impl<'a, S: Synthesizer> CompositionExecutor<'a, S> {
    /// Build a composition executor over the replayer the parent workflow is
    /// (or will be) dispatched through, so a callee runs in the same
    /// synthesizer session as its caller.
    pub fn new(replayer: &'a Replayer<S>) -> Self {
        Self { replayer }
    }

    /// Resolve `step`'s callee and check it, but dispatch nothing. Returns
    /// the callee stamped with the effective capability set (ready to
    /// replay) plus the [`CompositionNode`] summary. Useful standalone for a
    /// dry-run or renderer preview; [`Self::call`] runs this first and stops
    /// before any side effect if it errors.
    pub fn validate(
        &self,
        parent: &CompiledWorkflow,
        step: &CallWorkflowStep,
        loader: &impl WorkflowLoader,
    ) -> Result<(CompiledWorkflow, CompositionNode), CompositionError> {
        let child = loader.load(&step.callee).ok_or_else(|| CompositionError::UnknownCallee {
            action_id: step.action_id.clone(),
            parent_name: parent.manifest.name.clone(),
            callee: step.callee.clone(),
        })?;

        let effective = parent.manifest.capabilities.intersect(&child.manifest.capabilities);

        // Refuse-at-load: the intersection must still cover everything the
        // callee itself declares it needs, before any step of it runs.
        let reasons = unmet_requirements(&effective, &child.manifest.capabilities);
        if !reasons.is_empty() {
            return Err(CompositionError::InsufficientCapabilities {
                parent_name: parent.manifest.name.clone(),
                child_name: child.manifest.name.clone(),
                reasons: reasons.join("; "),
            });
        }

        // Defense in depth: check every one of the callee's own actions
        // against the effective set too, so the callee cannot exceed the
        // intersection even if a step reaches past what its own manifest
        // summary declared. Scanned in full before anything dispatches.
        for action in &child.actions {
            if let Some(reason) = exceeds_effective(&effective, action) {
                return Err(CompositionError::ExceedsEffective {
                    action_id: action.id.clone(),
                    parent_name: parent.manifest.name.clone(),
                    child_name: child.manifest.name.clone(),
                    reason,
                });
            }
        }

        let node = CompositionNode {
            action_id: step.action_id.clone(),
            parent_name: parent.manifest.name.clone(),
            parent_version: parent.manifest.version.clone(),
            child_name: child.manifest.name.clone(),
            child_version: child.manifest.version.clone(),
            child_step_summary: child.manifest.step_summary.clone(),
            effective_capabilities: effective.clone(),
        };

        let mut child_at_intersection = child;
        child_at_intersection.manifest.capabilities = effective;

        Ok((child_at_intersection, node))
    }

    /// Run a `callWorkflow` step end to end: [`Self::validate`], then replay
    /// the callee (stamped at the effective capability set) through the same
    /// replayer, and so the same synthesizer session, as the caller.
    pub fn call(
        &self,
        parent: &CompiledWorkflow,
        step: &CallWorkflowStep,
        loader: &impl WorkflowLoader,
        pre: &EvalContext,
        post: &EvalContext,
    ) -> Result<(ReplayReport, CompositionNode), CompositionError> {
        let (child, node) = self.validate(parent, step, loader)?;

        let report = self
            .replayer
            .replay_compiled(&child, &step.inputs, pre, post)
            .map_err(|source| CompositionError::Replay {
                child_name: node.child_name.clone(),
                source,
            })?;

        Ok((report, node))
    }
}

/// The declared requirements `effective` fails to cover, as plain language
/// sentences suitable for a refusal message. Empty when `effective` covers
/// everything `required` declares.
fn unmet_requirements(effective: &Capabilities, required: &Capabilities) -> Vec<String> {
    let mut reasons = Vec::new();

    for app in &required.apps {
        if !effective.apps.iter().any(|a| a.eq_ignore_ascii_case(app)) {
            reasons.push(format!("needs app `{app}`, which is outside what the parent grants"));
        }
    }
    for path in &required.paths {
        if !effective.paths.iter().any(|p| p.eq_ignore_ascii_case(path)) {
            reasons.push(format!("needs path `{path}`, which is outside what the parent grants"));
        }
    }
    if required.network && !effective.network {
        reasons.push("needs network access, which the parent does not grant".to_string());
    }
    if effective.risk_ceiling < required.risk_ceiling {
        reasons.push(format!(
            "needs a risk ceiling of at least `{}`, but the effective ceiling is `{}`",
            risk_label(required.risk_ceiling),
            risk_label(effective.risk_ceiling)
        ));
    }

    reasons
}

/// Whether a single callee action would reach past `effective`. `None` when
/// the action is within the effective set; otherwise a plain language reason.
fn exceeds_effective(effective: &Capabilities, action: &Action) -> Option<String> {
    if let Some(app) = action
        .target
        .as_ref()
        .and_then(|t| t.window.as_ref())
        .and_then(|w| w.process.as_deref())
    {
        if !effective.apps.iter().any(|granted| app_matches(granted, app)) {
            return Some(format!(
                "targets app `{app}`, which is outside the effective capability set"
            ));
        }
    }

    if let Some(path) = action.params.get("path").and_then(Value::as_str) {
        if !effective.paths.iter().any(|root| path_within(root, path)) {
            return Some(format!(
                "touches path `{path}`, which is outside the effective capability set"
            ));
        }
    }

    let wants_network = action.params.get("url").is_some()
        || action
            .params
            .get("network")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if wants_network && !effective.network {
        return Some("needs network access, which is outside the effective capability set".to_string());
    }

    if action.risk_class.exceeds(effective.risk_ceiling) {
        return Some(format!(
            "carries risk `{}`, which exceeds the effective risk ceiling `{}`",
            risk_label(action.risk_class),
            risk_label(effective.risk_ceiling)
        ));
    }

    None
}

fn risk_label(r: RiskClass) -> &'static str {
    match r {
        RiskClass::Read => "read",
        RiskClass::Write => "write",
        RiskClass::Destructive => "destructive",
    }
}

/// App match: case-insensitive equality, or the granted token equals the
/// file name of the requested executable path (so `notepad.exe` matches
/// `C:\Windows\notepad.exe`). Mirrors the matching rule the execution-time
/// grant check in `operant-safety` uses; reimplemented here because the
/// replay crate cannot depend on that crate (it would drag in a path this
/// backend-free crate must not have).
fn app_matches(granted: &str, requested: &str) -> bool {
    if granted.eq_ignore_ascii_case(requested) {
        return true;
    }
    let basename = |s: &str| {
        Path::new(s)
            .file_name()
            .map(|f| f.to_string_lossy().to_ascii_lowercase())
    };
    match (basename(granted), basename(requested)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Lexical subtree containment for the composition-time capability check:
/// `candidate` is within `root` when, case-insensitively and with
/// backslashes normalized to forward slashes, it starts with `root`. Lighter
/// weight than the `..`-aware canonicalization `operant-safety` does for the
/// OS-facing execution grant; composition only needs to catch a callee
/// action naming a path outside its declared roots.
fn path_within(root: &str, candidate: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
    let root = norm(root);
    let candidate = norm(candidate);
    !root.is_empty() && candidate.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(apps: &[&str], network: bool, risk_ceiling: RiskClass) -> Capabilities {
        Capabilities {
            apps: apps.iter().map(|s| s.to_string()).collect(),
            paths: Vec::new(),
            network,
            risk_ceiling,
        }
    }

    #[test]
    fn call_workflow_action_round_trips_through_as_call_workflow() {
        let mut inputs = BTreeMap::new();
        inputs.insert("name".to_string(), "Ada".to_string());
        let action = call_workflow_action("call1", "child-wf", inputs.clone(), RiskClass::Read);

        let step = as_call_workflow(&action).expect("recognized as a callWorkflow step");
        assert_eq!(step.action_id, "call1");
        assert_eq!(step.callee, "child-wf");
        assert_eq!(step.inputs, inputs);
    }

    #[test]
    fn as_call_workflow_ignores_unrelated_actions() {
        let mut plain = call_workflow_action("call1", "child-wf", BTreeMap::new(), RiskClass::Read);
        // Not every adapter_call is a workflow call: a different namespace
        // must not be misread as one.
        plain
            .params
            .insert("namespace".to_string(), Value::String("fs".to_string()));
        assert!(as_call_workflow(&plain).is_none());
    }

    #[test]
    fn unmet_requirements_is_empty_when_the_effective_set_covers_the_child() {
        let effective = caps(&["notepad.exe"], false, RiskClass::Write);
        let required = caps(&["notepad.exe"], false, RiskClass::Write);
        assert!(unmet_requirements(&effective, &required).is_empty());
    }

    #[test]
    fn unmet_requirements_reports_every_missing_dimension() {
        let effective = caps(&["notepad.exe"], false, RiskClass::Read);
        let required = caps(&["notepad.exe", "chrome.exe"], true, RiskClass::Write);
        let reasons = unmet_requirements(&effective, &required);
        assert_eq!(reasons.len(), 3); // chrome.exe, network, risk ceiling
        assert!(reasons.iter().any(|r| r.contains("chrome.exe")));
        assert!(reasons.iter().any(|r| r.contains("network")));
        assert!(reasons.iter().any(|r| r.contains("risk ceiling")));
    }

    #[test]
    fn app_matches_by_basename() {
        assert!(app_matches("notepad.exe", "C:/Windows/System32/notepad.exe"));
        assert!(app_matches("notepad.exe", "NOTEPAD.EXE"));
        assert!(!app_matches("notepad.exe", "chrome.exe"));
    }

    #[test]
    fn path_within_is_case_insensitive_and_slash_normalized() {
        assert!(path_within("C:/Downloads", "C:\\Downloads\\invoice.pdf"));
        assert!(!path_within("C:/Downloads", "C:/Other/invoice.pdf"));
        assert!(!path_within("", "C:/Downloads/invoice.pdf"));
    }
}
