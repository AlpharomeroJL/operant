//! The Action IR executor: dispatches an [`operant_ir::Action`] by `kind`
//! through a [`Synthesizer`], honoring the `pace` and `retry` fields, and
//! refusing destructive-risk actions that arrive without an [`Approval`].
//!
//! Target resolution (selector chain / anchor / last-known coords -> a
//! screen point) is a perception concern (C2/C3) and happens upstream of
//! this crate; the executor takes the already-[`ResolvedPoint`] as an
//! argument, exactly as the L3A brief specifies.

use std::time::Duration;

use operant_ir::{Action, ActionKind, Coords, Pace, RiskClass};
use rand::Rng;
use thiserror::Error;

use crate::adapter::{AdapterError, AdapterRegistry};
use crate::synth::{ModifierReleaseGuard, ScrollDirection, Synthesizer, SynthesizerError};

/// A target point already resolved by perception (UIA bounds, a vision
/// anchor match, or a cached coordinate). Reuses the IR's own `Coords`
/// shape rather than inventing a parallel type.
pub type ResolvedPoint = Coords;

/// Upper bound on how long a `wait` action will actually sleep. `wait`'s
/// `timeout_ms` in the IR is a perception poll budget (`scope` +
/// `timeout_ms`); without a `Perceiver` this executor cannot poll a scope,
/// so it degrades to a plain sleep capped here so a bad fixture (or a
/// misconfigured workflow) cannot hang a run for hours.
const MAX_WAIT_MS: u64 = 30_000;

/// Minimal safety-seam token. `risk_class: destructive` actions refuse to
/// execute without one bound to that exact action id.
///
/// This is deliberately a placeholder: the real capability-grant engine
/// with scopes, a hash-chained audit log, and human escalation
/// (`docs/specs/gates.md`, C10 / `operant-safety`) is owned by L6A. Until
/// that lands, whatever calls this executor is responsible for deciding
/// when an `Approval` is warranted and who `approved_by` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    action_id: String,
    approved_by: String,
}

impl Approval {
    pub fn for_action(action_id: impl Into<String>, approved_by: impl Into<String>) -> Self {
        Self {
            action_id: action_id.into(),
            approved_by: approved_by.into(),
        }
    }

    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    pub fn approved_by(&self) -> &str {
        &self.approved_by
    }

    /// An approval only covers the exact action it was issued for; one
    /// step's approval can never be reused to bless a different step.
    fn covers(&self, action: &Action) -> bool {
        self.action_id == action.id
    }
}

/// Outcome of a successful [`Executor::execute`] call.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionOutcome {
    pub action_id: String,
    /// Total dispatch attempts made, including the first (always >= 1).
    pub attempts: u32,
    /// Raw adapter result, present only for a successful `adapter_call`.
    pub adapter_result: Option<serde_json::Value>,
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("action `{action_id}` is risk_class destructive and requires an approval token for that exact action id")]
    ApprovalRequired { action_id: String },

    #[error("action kind `{kind:?}` is not dispatched by the action executor")]
    Unsupported { kind: ActionKind },

    #[error("action `{action_id}` of kind `{kind:?}` requires a resolved target point but none was given")]
    MissingResolvedPoint { action_id: String, kind: ActionKind },

    #[error("adapter_call is missing required param `{0}`")]
    MissingParam(&'static str),

    #[error(transparent)]
    Adapter(#[from] AdapterError),

    #[error(transparent)]
    Synthesizer(#[from] SynthesizerError),

    #[error("action `{action_id}` failed after {attempts} attempt(s): {source}")]
    RetriesExhausted {
        action_id: String,
        attempts: u32,
        #[source]
        source: Box<ActionError>,
    },
}

impl ActionError {
    /// Only a transient synthesizer failure is worth retrying. Approval
    /// refusals, unsupported kinds, missing params, and schema violations
    /// are all permanent for the given Action IR and retrying wastes the
    /// backoff window without changing the outcome.
    fn is_retryable(&self) -> bool {
        matches!(self, ActionError::Synthesizer(_))
    }
}

/// Injectable "wait this long" hook. Production uses [`RealSleeper`];
/// tests use [`NoopSleeper`] so human-paced fixtures (and the retry
/// backoff loop) do not make `cargo test` slow.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, duration: Duration);
}

/// Real wall-clock sleeper. Default for every [`Executor`].
#[derive(Default)]
pub struct RealSleeper;

impl Sleeper for RealSleeper {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// No-op sleeper for tests: exercises the exact same pacing/retry branches
/// as production without spending real wall-clock time.
#[derive(Default)]
pub struct NoopSleeper;

impl Sleeper for NoopSleeper {
    fn sleep(&self, _duration: Duration) {}
}

/// Dispatches Action IR steps through a [`Synthesizer`], honoring pace and
/// retry, validating `adapter_call` params against a registered adapter's
/// schema, and refusing destructive actions without an [`Approval`].
pub struct Executor<S: Synthesizer> {
    synth: S,
    adapters: AdapterRegistry,
    sleeper: Box<dyn Sleeper>,
}

impl<S: Synthesizer> Executor<S> {
    pub fn new(synth: S) -> Self {
        Self {
            synth,
            adapters: AdapterRegistry::new(),
            sleeper: Box::new(RealSleeper),
        }
    }

    pub fn with_adapters(synth: S, adapters: AdapterRegistry) -> Self {
        Self {
            synth,
            adapters,
            sleeper: Box::new(RealSleeper),
        }
    }

    /// Swap the pacing/backoff sleeper. Tests use this to install a
    /// [`NoopSleeper`]; production leaves the default [`RealSleeper`].
    pub fn with_sleeper(mut self, sleeper: Box<dyn Sleeper>) -> Self {
        self.sleeper = sleeper;
        self
    }

    pub fn adapters_mut(&mut self) -> &mut AdapterRegistry {
        &mut self.adapters
    }

    pub fn synthesizer(&self) -> &S {
        &self.synth
    }

    /// Execute one Action IR step to completion (including retries).
    ///
    /// `resolved` is the screen point perception already resolved for a
    /// `click` step; it is unused for kinds that do not need one.
    /// `approval` must cover `action.id` when `action.risk_class` is
    /// `destructive`, or execution refuses with
    /// [`ActionError::ApprovalRequired`] before anything is dispatched.
    pub fn execute(
        &self,
        action: &Action,
        resolved: Option<&ResolvedPoint>,
        approval: Option<&Approval>,
    ) -> Result<ActionOutcome, ActionError> {
        if action.risk_class == RiskClass::Destructive {
            let covered = approval.map(|a| a.covers(action)).unwrap_or(false);
            if !covered {
                return Err(ActionError::ApprovalRequired {
                    action_id: action.id.clone(),
                });
            }
        }

        let max_attempts = action.retry.attempts.saturating_add(1);
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            // Held for the whole dispatch attempt: if `dispatch_once`
            // panics, this guard's Drop still runs during unwind and
            // sweeps every modifier key (docs/specs/action.md). Disarmed
            // right below on any normal return (Ok or Err) so an ordinary,
            // typed failure does not also trigger the panic-time sweep;
            // ordinary failures are the retry loop's job.
            let mut guard = ModifierReleaseGuard::new(&self.synth);
            let result = self.dispatch_once(action, resolved);
            guard.disarm();
            match result {
                Ok(adapter_result) => {
                    return Ok(ActionOutcome {
                        action_id: action.id.clone(),
                        attempts: attempt,
                        adapter_result,
                    })
                }
                Err(e) => {
                    if attempt < max_attempts && e.is_retryable() {
                        self.sleeper
                            .sleep(Duration::from_millis(action.retry.backoff_ms));
                        continue;
                    }
                    return Err(if attempt > 1 {
                        ActionError::RetriesExhausted {
                            action_id: action.id.clone(),
                            attempts: attempt,
                            source: Box::new(e),
                        }
                    } else {
                        e
                    });
                }
            }
        }
    }

    fn dispatch_once(
        &self,
        action: &Action,
        resolved: Option<&ResolvedPoint>,
    ) -> Result<Option<serde_json::Value>, ActionError> {
        // Kinds this layer does not own get refused before any side
        // effect: `assert` is evaluated against a perception snapshot by
        // the gate engine (C9, operant-gates); `drag` needs a second
        // resolved point and a mouse-down/move/up primitive the
        // Synthesizer trait does not expose yet (see FOLLOWUPS).
        if matches!(action.kind, ActionKind::Assert | ActionKind::Drag) {
            return Err(ActionError::Unsupported { kind: action.kind });
        }

        self.apply_human_pacing(action);

        // "focus the target window... BEFORE any keystroke" applies to
        // every kind that carries a window target; kinds without one
        // (adapter_call, global key, wait) simply have no window to focus.
        if let Some(window) = action.target.as_ref().and_then(|t| t.window.as_ref()) {
            self.synth.focus_window(window)?;
        }

        match action.kind {
            ActionKind::Click => {
                let point = resolved
                    .cloned()
                    .ok_or_else(|| ActionError::MissingResolvedPoint {
                        action_id: action.id.clone(),
                        kind: action.kind,
                    })?;
                self.synth.click_point(point)?;
                Ok(None)
            }
            ActionKind::Type => {
                let text = param_str(action, "text")?;
                self.synth.type_text(&text)?;
                Ok(None)
            }
            ActionKind::Key => {
                let combo = param_str(action, "combo")?;
                self.synth.key(&combo)?;
                Ok(None)
            }
            ActionKind::Scroll => {
                let direction_raw = param_str(action, "direction")?;
                let direction = ScrollDirection::parse(&direction_raw)
                    .ok_or(ActionError::MissingParam("direction"))?;
                let amount = action
                    .params
                    .get("amount")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);
                self.synth.scroll(direction, amount)?;
                Ok(None)
            }
            ActionKind::AdapterCall => self.dispatch_adapter_call(action).map(Some),
            ActionKind::Wait => {
                self.sleeper
                    .sleep(Duration::from_millis(action.timeout_ms.min(MAX_WAIT_MS)));
                Ok(None)
            }
            ActionKind::Assert | ActionKind::Drag => unreachable!("refused above"),
        }
    }

    fn dispatch_adapter_call(&self, action: &Action) -> Result<serde_json::Value, ActionError> {
        let namespace = param_str(action, "namespace")?;
        let verb = param_str(action, "verb")?;
        let args = action
            .params
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        Ok(self.adapters.call(&namespace, &verb, &args)?)
    }

    /// `docs/specs/action.md`: human pace is a 150-400 ms pre-click hover
    /// before a click, or a 40-120 ms pause before a keystroke-shaped
    /// action. One jittered sleep per action (not per character) keeps
    /// exactly one Synthesizer call per dispatched kind, which is what
    /// both the fixture-replay test and `SynthCall` recording expect.
    fn apply_human_pacing(&self, action: &Action) {
        if action.pace != Pace::Human {
            return;
        }
        let range_ms = match action.kind {
            ActionKind::Click => 150..=400,
            ActionKind::Type | ActionKind::Key => 40..=120,
            _ => return,
        };
        let delay_ms = rand::thread_rng().gen_range(range_ms);
        self.sleeper.sleep(Duration::from_millis(delay_ms));
    }
}

fn param_str(action: &Action, key: &'static str) -> Result<String, ActionError> {
    action
        .params
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or(ActionError::MissingParam(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Adapter, Idempotency, VerbSpec};
    use crate::synth::MockSynthesizer;
    use operant_ir::{Grounding, Retry, Target, WindowMatch};
    use serde_json::json;
    use std::panic::{self, AssertUnwindSafe};

    fn base_action(kind: ActionKind, risk_class: RiskClass) -> Action {
        Action {
            v: 1,
            id: "step-1".into(),
            kind,
            intent: None,
            target: None,
            params: serde_json::Map::new(),
            pace: Pace::Instant,
            risk_class,
            irreversible: false,
            grounding: Grounding::Uia,
            timeout_ms: 5000,
            retry: Retry {
                attempts: 2,
                backoff_ms: 250,
            },
        }
    }

    fn executor_with(mock: MockSynthesizer) -> Executor<MockSynthesizer> {
        Executor::new(mock).with_sleeper(Box::new(NoopSleeper))
    }

    #[test]
    fn destructive_action_without_approval_is_refused() {
        let exec = executor_with(MockSynthesizer::new());
        let mut action = base_action(ActionKind::Key, RiskClass::Destructive);
        action
            .params
            .insert("combo".into(), json!("ctrl+shift+delete"));

        let err = exec.execute(&action, None, None).unwrap_err();
        assert!(
            matches!(err, ActionError::ApprovalRequired { ref action_id } if action_id == "step-1")
        );
        // Refused before any synthesizer call was made.
        assert_eq!(exec.synthesizer().call_count(), 0);
    }

    #[test]
    fn approval_for_a_different_action_does_not_count() {
        let exec = executor_with(MockSynthesizer::new());
        let mut action = base_action(ActionKind::Key, RiskClass::Destructive);
        action
            .params
            .insert("combo".into(), json!("ctrl+shift+delete"));
        let mismatched = Approval::for_action("some-other-step", "josef");

        let err = exec.execute(&action, None, Some(&mismatched)).unwrap_err();
        assert!(matches!(err, ActionError::ApprovalRequired { .. }));
    }

    #[test]
    fn matching_approval_lets_a_destructive_action_run() {
        let exec = executor_with(MockSynthesizer::new());
        let mut action = base_action(ActionKind::Key, RiskClass::Destructive);
        action
            .params
            .insert("combo".into(), json!("ctrl+shift+delete"));
        let approval = Approval::for_action("step-1", "josef");

        let outcome = exec.execute(&action, None, Some(&approval)).unwrap();
        assert_eq!(outcome.attempts, 1);
        assert_eq!(exec.synthesizer().call_count(), 1);
    }

    #[test]
    fn read_and_write_risk_actions_need_no_approval() {
        let exec = executor_with(MockSynthesizer::new());
        let mut action = base_action(ActionKind::Key, RiskClass::Write);
        action.params.insert("combo".into(), json!("ctrl+s"));
        assert!(exec.execute(&action, None, None).is_ok());
    }

    #[test]
    fn click_without_a_resolved_point_is_a_typed_error() {
        let exec = executor_with(MockSynthesizer::new());
        let action = base_action(ActionKind::Click, RiskClass::Read);
        let err = exec.execute(&action, None, None).unwrap_err();
        assert!(matches!(err, ActionError::MissingResolvedPoint { .. }));
    }

    #[test]
    fn click_focuses_then_clicks_the_resolved_point() {
        let exec = executor_with(MockSynthesizer::new());
        let mut action = base_action(ActionKind::Click, RiskClass::Read);
        action.target = Some(Target {
            window: Some(WindowMatch {
                process: Some("notepad.exe".into()),
                title_pattern: None,
            }),
            ..Default::default()
        });
        let point = Coords {
            x: 12.0,
            y: 34.0,
            monitor: None,
            dpi_scale: None,
        };

        let outcome = exec.execute(&action, Some(&point), None).unwrap();
        assert_eq!(outcome.attempts, 1);
        assert_eq!(exec.synthesizer().call_count(), 2); // focus + click
    }

    #[test]
    fn drag_and_assert_are_refused_without_touching_the_synthesizer() {
        let exec = executor_with(MockSynthesizer::new());
        for kind in [ActionKind::Drag, ActionKind::Assert] {
            let action = base_action(kind, RiskClass::Read);
            let err = exec.execute(&action, None, None).unwrap_err();
            assert!(matches!(err, ActionError::Unsupported { kind: k } if k == kind));
        }
        assert_eq!(exec.synthesizer().call_count(), 0);
    }

    #[test]
    fn wait_sleeps_without_calling_the_synthesizer() {
        let exec = executor_with(MockSynthesizer::new());
        let action = base_action(ActionKind::Wait, RiskClass::Read);
        let outcome = exec.execute(&action, None, None).unwrap();
        assert_eq!(outcome.attempts, 1);
        assert_eq!(exec.synthesizer().call_count(), 0);
    }

    #[test]
    fn transient_synthesizer_failure_is_retried_per_the_retry_field() {
        let mock = MockSynthesizer::new();
        mock.fail_next_call(SynthesizerError::Input("glitch".into()));
        let exec = executor_with(mock);
        let mut action = base_action(ActionKind::Key, RiskClass::Read);
        action.params.insert("combo".into(), json!("ctrl+s"));
        action.retry = Retry {
            attempts: 2,
            backoff_ms: 1,
        };

        let outcome = exec.execute(&action, None, None).unwrap();
        assert_eq!(
            outcome.attempts, 2,
            "first attempt failed, second succeeded"
        );
        assert_eq!(exec.synthesizer().call_count(), 2);
    }

    #[test]
    fn retries_exhausted_wraps_the_final_error_typed() {
        let mock = MockSynthesizer::new();
        mock.fail_next_call(SynthesizerError::Input("a".into()));
        mock.fail_next_call(SynthesizerError::Input("b".into()));
        let exec = executor_with(mock);
        let mut action = base_action(ActionKind::Key, RiskClass::Read);
        action.params.insert("combo".into(), json!("ctrl+s"));
        action.retry = Retry {
            attempts: 1,
            backoff_ms: 1,
        }; // 1 retry => 2 attempts total

        let err = exec.execute(&action, None, None).unwrap_err();
        match err {
            ActionError::RetriesExhausted { attempts, .. } => assert_eq!(attempts, 2),
            other => panic!("expected RetriesExhausted, got {other:?}"),
        }
    }

    #[test]
    fn non_retryable_errors_do_not_consume_the_retry_budget() {
        let exec = executor_with(MockSynthesizer::new());
        // Missing `combo` param: permanent, should fail on attempt 1 with
        // the raw error, not a retry-exhausted wrapper.
        let action = base_action(ActionKind::Key, RiskClass::Read);
        let err = exec.execute(&action, None, None).unwrap_err();
        assert!(matches!(err, ActionError::MissingParam("combo")));
        assert_eq!(exec.synthesizer().call_count(), 0);
    }

    struct FsAdapter {
        verbs: Vec<VerbSpec>,
    }

    impl FsAdapter {
        fn new() -> Self {
            Self {
                verbs: vec![VerbSpec::new(
                    "read",
                    json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": { "type": "string", "minLength": 1 } }
                    }),
                    RiskClass::Read,
                    Idempotency::Idempotent,
                )],
            }
        }
    }

    impl Adapter for FsAdapter {
        fn namespace(&self) -> &str {
            "fs"
        }
        fn verbs(&self) -> &[VerbSpec] {
            &self.verbs
        }
        fn call(
            &self,
            verb: &str,
            args: &serde_json::Value,
        ) -> Result<serde_json::Value, AdapterError> {
            Ok(json!({ "verb": verb, "ok": true, "args": args }))
        }
    }

    fn executor_with_fs_adapter() -> Executor<MockSynthesizer> {
        let mut adapters = AdapterRegistry::new();
        adapters.register(Box::new(FsAdapter::new()));
        Executor::with_adapters(MockSynthesizer::new(), adapters)
            .with_sleeper(Box::new(NoopSleeper))
    }

    fn adapter_call_action(args: serde_json::Value) -> Action {
        let mut action = base_action(ActionKind::AdapterCall, RiskClass::Read);
        action.params.insert("namespace".into(), json!("fs"));
        action.params.insert("verb".into(), json!("read"));
        action.params.insert("args".into(), args);
        action
    }

    #[test]
    fn adapter_call_validates_params_before_dispatch_good_payload() {
        let exec = executor_with_fs_adapter();
        let action = adapter_call_action(json!({ "path": "C:/tmp/a.txt" }));
        let outcome = exec.execute(&action, None, None).unwrap();
        assert_eq!(
            outcome.adapter_result,
            Some(json!({ "verb": "read", "ok": true, "args": { "path": "C:/tmp/a.txt" } }))
        );
    }

    #[test]
    fn adapter_call_rejects_a_bad_payload() {
        let exec = executor_with_fs_adapter();
        let action = adapter_call_action(json!({})); // missing required `path`
        let err = exec.execute(&action, None, None).unwrap_err();
        assert!(
            matches!(
                err,
                ActionError::Adapter(AdapterError::SchemaValidation { .. })
            ),
            "got {err:?}"
        );
        // Schema validation is a permanent failure: no synthesizer calls,
        // no retry consumed.
        assert_eq!(exec.synthesizer().call_count(), 0);
    }

    #[test]
    fn modifier_sweep_still_fires_when_a_dispatch_panics() {
        let mock = MockSynthesizer::new();
        mock.panic_next_call(); // the upcoming `key` call panics mid-flight
        let exec = executor_with(mock);
        let mut action = base_action(ActionKind::Key, RiskClass::Read);
        action.params.insert("combo".into(), json!("ctrl+s"));

        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let result = panic::catch_unwind(AssertUnwindSafe(|| exec.execute(&action, None, None)));
        panic::set_hook(prev_hook);

        assert!(
            result.is_err(),
            "the simulated panic should unwind out of execute()"
        );
        let calls = exec.synthesizer().calls();
        assert!(
            calls.contains(&crate::synth::SynthCall::ReleaseAllModifiers),
            "release-all sweep must fire on the panic/kill path, got {calls:?}"
        );
    }
}
