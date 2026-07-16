//! The EXPLORE loop (C6): goal -> perceive -> element digest -> plan ->
//! propose an Action -> the safety gate (operant-safety) -> execute ->
//! observe -> record -> repeat until the planner signals done.
//!
//! Ties together a [`Perceiver`] (C2, `operant-core`; use
//! `operant_perception_uia::FixturePerceiver` in tests), a [`ModelBackend`]
//! planner (this crate's own [`crate::backends`]; use
//! [`crate::backends::MockPlannerBackend`] in tests),
//! [`operant_safety::RunGuard`] (C10's hard safety invariants -- the gate
//! every proposed action always passes through, unconditionally, before it
//! may execute), an [`operant_action::Executor`] (C4; use
//! `operant_action::MockSynthesizer` in tests), and an
//! [`operant_recorder::Recorder`] (C7). Every attempted step -- executed or
//! gate-blocked -- is recorded; every proposed step is gated; the loop
//! publishes `run.*` bus events (C1) at each stage, including HITL
//! pause/redirect/resume (see [`control`]).
//!
//! The planner never sees pixels: every prompt is built from a
//! [`digest::ElementDigest`], a plain-text summary of the perception
//! snapshot's normalized element tree.
//!
//! # Planner tool contract (this crate's own; not part of `contracts/model_backend.md`)
//! The planner is offered exactly two tools: `propose_action`, whose
//! arguments are one Action IR object, and `done`, which signals the goal
//! is achieved. A single `complete()` call may return a batch of several
//! `propose_action` calls (`docs/ARCHITECTURE.md` C6: "propose action
//! batch"); the loop executes them one at a time, re-perceiving, gating,
//! executing, observing, and recording each individually before moving to
//! the next. A response with no tool calls at all is treated as an
//! implicit `done`.

pub mod control;
pub mod digest;

use std::time::{Duration, Instant};

use futures::StreamExt;
use operant_action::{Executor, Synthesizer};
use operant_core::bus::events::{
    GateEscalation, HaltReason, PausedBy, RunCompleted, RunHalted, RunMode as BusRunMode,
    RunOutcome as BusRunOutcome, RunPaused, RunRedirected, RunResumed, RunStarted,
    RunStepExecuted, RunStepFailed, RunStepGated, RunStepProposed, StepOutcome,
};
use operant_core::perceive::{PerceptionError, Perceiver, Resolved};
use operant_core::Bus;
use operant_ir::{Action, ActionKind, Coords, Element, GateKind, GateResult, Role, Selector, Snapshot};
use operant_recorder::{NewStep, Recorder, RunMode as RecorderRunMode, RunStatus};
use operant_safety::{Disposition, RunGuard, SafetyVerdict};
use serde_json::json;

pub use control::{BusControl, HitlControl, NoControl, RunControl, ScriptedControl};
pub use digest::ElementDigest;

use crate::backends::{
    BackendEvent, CompletionRequest, ContentPart, Message, MessageRole, ModelBackend,
    RequestRole, ToolSchema,
};

/// Tool name the planner calls to signal that the goal is complete and no
/// further actions are needed.
pub const DONE_TOOL: &str = "done";
/// Tool name the planner calls to propose one Action IR step. `arguments`
/// on that call must deserialize as an [`operant_ir::Action`].
pub const PROPOSE_ACTION_TOOL: &str = "propose_action";

/// How many times the loop re-polls control while explicitly paused before
/// giving up and halting the run. Paired with [`PAUSE_POLL_INTERVAL`].
const MAX_PAUSE_POLLS: u32 = 50;
/// Wait between re-polls while explicitly paused (see [`MAX_PAUSE_POLLS`]).
/// A timer-poll loop, not a wake-on-event wait: fine for a single
/// supervised run; a production build with many concurrent runs would
/// want an async notify wired to the bus subscription instead.
const PAUSE_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Errors that stop the loop before it can even finish its own bookkeeping.
/// Ordinary run-time failures (perception misses, gate blocks, action
/// failures) are NOT modeled here: they end the run normally, as a halted
/// [`RunSummary`], because the recorder and bus still need to hear about
/// them. This type is for bookkeeping itself breaking (e.g. the recorder's
/// own database failing).
#[derive(Debug, thiserror::Error)]
pub enum ExploreError {
    #[error(transparent)]
    Recorder(#[from] operant_recorder::RecorderError),
    #[error("failed to serialize a step: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// What happened over the course of one [`ExploreLoop::run`] call.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub run_id: String,
    pub outcome: BusRunOutcome,
    /// Count of actions attempted (executed or gate-blocked), matching
    /// `steps` on the published `run.completed` event.
    pub steps: u32,
    /// Real count of model (planner) calls this run made -- one per loop round
    /// that consulted the planner -- matching `model_calls` on the published
    /// `run.completed` event. An EXPLORE run is nonzero; the replay path (which
    /// never enters this loop) is structurally zero.
    pub model_calls: u64,
    /// Set when the run ended via [`RunHalted`] rather than reaching the
    /// planner's own `done` signal.
    pub halted: Option<HaltReason>,
}

/// Ties perception, planning, the safety gate, action execution, and
/// recording into the EXPLORE loop. Generic over the [`Synthesizer`] the
/// caller's [`Executor`] was built with; `Perceiver` and `ModelBackend` are
/// boxed trait objects since callers construct them once per run and this
/// loop never needs to downcast them.
pub struct ExploreLoop<S: Synthesizer> {
    perceiver: Box<dyn Perceiver>,
    planner: Box<dyn ModelBackend>,
    executor: Executor<S>,
    guard: RunGuard,
    window_process: String,
}

impl<S: Synthesizer> ExploreLoop<S> {
    /// Build a loop targeting one window process (e.g. `"notepad.exe"`).
    /// The safety guard's "unexpected window" invariant expects exactly
    /// this process to stay foreground for the run's duration.
    pub fn new(
        perceiver: Box<dyn Perceiver>,
        planner: Box<dyn ModelBackend>,
        executor: Executor<S>,
        window_process: impl Into<String>,
    ) -> Self {
        let window_process = window_process.into();
        ExploreLoop {
            perceiver,
            planner,
            executor,
            guard: RunGuard::new([window_process.clone()]),
            window_process,
        }
    }

    /// The action executor this loop drives, e.g. for a test to inspect
    /// `.executor().synthesizer().calls()` after a run.
    pub fn executor(&self) -> &Executor<S> {
        &self.executor
    }

    /// Run one EXPLORE session to completion (or a halt). Publishes
    /// `run.*` events on `bus` and records every attempted step -- executed
    /// or gate-blocked -- to `recorder`.
    pub async fn run(
        &self,
        bus: &Bus,
        recorder: &Recorder,
        goal: &str,
        control: &mut dyn HitlControl,
    ) -> Result<RunSummary, ExploreError> {
        let run_id = recorder.start_run(goal, RecorderRunMode::Explore, None)?;
        let _ = bus.publish_event(&RunStarted {
            run_id: run_id.clone(),
            goal: goal.to_string(),
            mode: BusRunMode::Explore,
            workflow_name: None,
        });

        let started_at = Instant::now();
        let mut seq: u32 = 0;
        // Real per-run model-call counter (D5): incremented once for every loop
        // round that consults the planner, i.e. once per `planner.complete(..)`.
        // This is what makes an explore run's `MODEL CALLS <n>` a measured value;
        // the replay path never enters this loop, so its count stays a structural
        // zero (set at the replay wrapper, `cli/src/commands/serve.rs`).
        let mut model_calls: u64 = 0;
        let mut history: Vec<String> = Vec::new();

        'rounds: loop {
            let snapshot = match self.perceiver.snapshot(&self.window_process) {
                Ok(s) => s,
                Err(e) => {
                    return self.halt(
                        bus,
                        recorder,
                        run_id,
                        seq,
                        started_at,
                        model_calls,
                        HaltCause::error(format!("perceive failed: {e}")),
                    );
                }
            };

            let request = build_request(goal, &ElementDigest::build(&snapshot), &history);
            let events: Vec<BackendEvent> = self.planner.complete(request).collect().await;
            // One round consulted the planner: one real model call. Counted here,
            // before inspecting the response, so a round that errors or returns an
            // implicit `done` still counts the call that was actually made.
            model_calls += 1;

            if let Some((error_id, message)) = events.iter().find_map(|e| match e {
                BackendEvent::Error { error_id, message, .. } => {
                    Some((error_id.clone(), message.clone()))
                }
                _ => None,
            }) {
                return self.halt(
                    bus,
                    recorder,
                    run_id,
                    seq,
                    started_at,
                    model_calls,
                    HaltCause::error(format!("{error_id}: {message}")),
                );
            }

            let tool_calls: Vec<(String, serde_json::Value)> = events
                .into_iter()
                .filter_map(|e| match e {
                    BackendEvent::ToolCall { name, arguments, .. } => Some((name, arguments)),
                    _ => None,
                })
                .collect();

            if tool_calls.is_empty() {
                break 'rounds; // planner proposed nothing further: implicit done
            }

            for (name, arguments) in tool_calls {
                if name == DONE_TOOL {
                    break 'rounds;
                }

                let mut action: Action = match serde_json::from_value(arguments) {
                    Ok(a) => a,
                    Err(e) => {
                        return self.halt(
                            bus,
                            recorder,
                            run_id,
                            seq,
                            started_at,
                            model_calls,
                            HaltCause::error(format!(
                                "planner proposed an unparseable action: {e}"
                            )),
                        );
                    }
                };

                seq += 1;

                // Fill a deterministic per-run step id when the planner omitted
                // it (the id is internal bookkeeping; a real model leaves it
                // blank rather than inventing one). Unique within the run via
                // the seq counter, so the compiled workflow still has stable ids.
                if action.id.trim().is_empty() {
                    action.id = format!("planner-step-{seq}");
                }

                let control_outcome = handle_control(bus, &run_id, seq, control).await;
                let pending_correction = match control_outcome {
                    ControlOutcome::Continue(correction) => correction,
                    ControlOutcome::GiveUp => {
                        return self.halt(
                            bus,
                            recorder,
                            run_id,
                            seq,
                            started_at,
                            model_calls,
                            HaltCause::human(),
                        );
                    }
                };

                let step_snapshot = match self.perceiver.snapshot(&self.window_process) {
                    Ok(s) => s,
                    Err(e) => {
                        return self.halt(
                            bus,
                            recorder,
                            run_id,
                            seq,
                            started_at,
                            model_calls,
                            HaltCause::error(format!("perceive failed: {e}")),
                        );
                    }
                };

                let _ = bus.publish_event(&RunStepProposed {
                    run_id: run_id.clone(),
                    step: action.clone(),
                });

                // ---- the safety gate (operant-safety hard invariants) ----
                let target_el = find_target_element(&step_snapshot, &action);
                let action_json = serde_json::to_value(&action)?;
                let verdict = self.guard.evaluate(target_el, &step_snapshot, &action_json);
                let safety_result = if verdict.is_blocked() {
                    GateResult::Fail
                } else {
                    GateResult::Pass
                };
                let _ = bus.publish_event(&RunStepGated {
                    run_id: run_id.clone(),
                    step_id: action.id.clone(),
                    gate_kind: GateKind::Safety,
                    result: safety_result,
                    expr: None,
                });

                if let SafetyVerdict::Blocked(escalation) = &verdict {
                    let requires_approval =
                        matches!(escalation.disposition, Disposition::RequireApproval);
                    let _ = bus.publish_event(&GateEscalation {
                        run_id: run_id.clone(),
                        step_id: Some(action.id.clone()),
                        sentence: escalation.sentence.clone(),
                        requires_approval,
                    });
                    let mut blocked_step =
                        NewStep::new(seq, action.clone(), action.grounding, "blocked", 0)
                            .with_digests(
                                Some(step_snapshot.digest.clone()),
                                Some(step_snapshot.digest.clone()),
                            )
                            .with_note(escalation.sentence.clone());
                    if let Some(c) = pending_correction.clone() {
                        blocked_step = blocked_step.with_human_correction(c);
                    }
                    recorder.record_step(&run_id, blocked_step)?;
                    return self.halt(
                        bus,
                        recorder,
                        run_id,
                        seq,
                        started_at,
                        model_calls,
                        HaltCause::gate(),
                    );
                }

                // ---- resolve + execute ----
                let resolved = match self.resolve_target(&step_snapshot, &action) {
                    Ok(r) => r,
                    Err(e) => {
                        return self.halt(
                            bus,
                            recorder,
                            run_id,
                            seq,
                            started_at,
                            model_calls,
                            HaltCause::error(format!("target resolution failed: {e}")),
                        );
                    }
                };
                let resolved_point = resolved.map(|r| Coords {
                    x: r.x,
                    y: r.y,
                    monitor: r.monitor,
                    dpi_scale: Some(step_snapshot.window.dpi_scale),
                });

                let t0 = Instant::now();
                let exec_result = self.executor.execute(&action, resolved_point.as_ref(), None);
                let ms = t0.elapsed().as_millis() as u64;

                match exec_result {
                    Ok(action_outcome) => {
                        // Observe: a fresh snapshot after acting, both for the
                        // recorder's after-digest and so the NEXT action in this
                        // batch (if any) is gated/resolved against current state.
                        let after = self
                            .perceiver
                            .snapshot(&self.window_process)
                            .unwrap_or_else(|_| step_snapshot.clone());
                        let step_outcome = if action_outcome.attempts > 1 {
                            StepOutcome::Retried
                        } else {
                            StepOutcome::Ok
                        };
                        let mut new_step = NewStep::new(
                            seq,
                            action.clone(),
                            action.grounding,
                            outcome_label(step_outcome),
                            ms,
                        )
                        .with_digests(Some(step_snapshot.digest.clone()), Some(after.digest.clone()));
                        if let Some(c) = pending_correction {
                            new_step = new_step.with_human_correction(c);
                        }
                        recorder.record_step(&run_id, new_step)?;
                        history.push(format!("{:?} (id={}) -> ok", action.kind, action.id));
                        let _ = bus.publish_event(&RunStepExecuted {
                            run_id: run_id.clone(),
                            step_id: action.id.clone(),
                            outcome: step_outcome,
                            ms,
                            grounding: action.grounding,
                        });
                    }
                    Err(e) => {
                        let mut new_step =
                            NewStep::new(seq, action.clone(), action.grounding, "failed", ms)
                                .with_digests(Some(step_snapshot.digest.clone()), None);
                        if let Some(c) = pending_correction {
                            new_step = new_step.with_human_correction(c);
                        }
                        recorder.record_step(&run_id, new_step)?;
                        let _ = bus.publish_event(&RunStepFailed {
                            run_id: run_id.clone(),
                            step_id: action.id.clone(),
                            error_id: "action_execution_failed".to_string(),
                            message: e.to_string(),
                        });
                        return self.halt(
                            bus,
                            recorder,
                            run_id,
                            seq,
                            started_at,
                            model_calls,
                            HaltCause::error(e.to_string()),
                        );
                    }
                }
            }
        }

        let wall_ms = started_at.elapsed().as_millis() as u64;
        let _ = bus.publish_event(&RunCompleted {
            run_id: run_id.clone(),
            outcome: BusRunOutcome::Ok,
            steps: seq,
            wall_ms,
            model_calls,
        });
        recorder.end_run(&run_id, RunStatus::Completed)?;
        Ok(RunSummary {
            run_id,
            outcome: BusRunOutcome::Ok,
            steps: seq,
            model_calls,
            halted: None,
        })
    }

    /// Resolve a `click` action's target to a fresh screen point. Every
    /// other action kind needs no resolved point (matches
    /// `operant_action::Executor`'s own dispatch: only `click` consumes
    /// one), so this returns `Ok(None)` for them without touching the
    /// perceiver.
    fn resolve_target(
        &self,
        snapshot: &Snapshot,
        action: &Action,
    ) -> Result<Option<Resolved>, PerceptionError> {
        if action.kind != ActionKind::Click {
            return Ok(None);
        }
        let selectors = action
            .target
            .as_ref()
            .map(|t| t.selectors.as_slice())
            .unwrap_or(&[]);
        if !selectors.is_empty() {
            return self.perceiver.resolve(snapshot, selectors).map(Some);
        }
        if let Some(coords) = action.target.as_ref().and_then(|t| t.coords_last_known.as_ref()) {
            return Ok(Some(Resolved {
                x: coords.x,
                y: coords.y,
                monitor: coords.monitor.clone(),
            }));
        }
        Ok(None)
    }

    /// Publish `run.halted` then `run.completed` (outcome `failed`), close
    /// out the recorder's run row, and hand back the matching
    /// [`RunSummary`]. The one place every non-success exit from
    /// [`Self::run`] converges on, so halted runs are always closed out
    /// identically regardless of which check tripped.
    fn halt(
        &self,
        bus: &Bus,
        recorder: &Recorder,
        run_id: String,
        steps: u32,
        started_at: Instant,
        model_calls: u64,
        cause: HaltCause,
    ) -> Result<RunSummary, ExploreError> {
        let HaltCause { reason, error_id } = cause;
        let _ = bus.publish_event(&RunHalted {
            run_id: run_id.clone(),
            reason,
            error_id,
        });
        let wall_ms = started_at.elapsed().as_millis() as u64;
        let _ = bus.publish_event(&RunCompleted {
            run_id: run_id.clone(),
            outcome: BusRunOutcome::Failed,
            steps,
            wall_ms,
            model_calls,
        });
        recorder.end_run(&run_id, RunStatus::Failed)?;
        Ok(RunSummary {
            run_id,
            outcome: BusRunOutcome::Failed,
            steps,
            model_calls,
            halted: Some(reason),
        })
    }
}

/// Why a run halted, bundled into one value so [`ExploreLoop::halt`] takes
/// one parameter instead of two at every call site.
struct HaltCause {
    reason: HaltReason,
    error_id: Option<String>,
}

impl HaltCause {
    fn error(message: impl Into<String>) -> Self {
        HaltCause {
            reason: HaltReason::Error,
            error_id: Some(message.into()),
        }
    }

    fn gate() -> Self {
        HaltCause {
            reason: HaltReason::Gate,
            error_id: None,
        }
    }

    fn human() -> Self {
        HaltCause {
            reason: HaltReason::Human,
            error_id: None,
        }
    }
}

/// Outcome of one [`handle_control`] call.
enum ControlOutcome {
    /// Proceed with the step about to run, optionally carrying a
    /// human-correction JSON value to attach to its recorded row.
    Continue(Option<serde_json::Value>),
    /// Explicitly paused and never resumed within [`MAX_PAUSE_POLLS`]; the
    /// caller halts the run with [`HaltReason::Human`].
    GiveUp,
}

/// Poll `control` once and react. A bare [`RunControl::Redirect`] (no
/// prior [`RunControl::Pause`]) is handled as an atomic pause-correct-resume:
/// all three `run.*` events publish back to back and the step about to run
/// carries the correction. An explicit [`RunControl::Pause`] instead blocks
/// (bounded, polling) until a [`RunControl::Resume`] or
/// [`RunControl::Redirect`] arrives.
async fn handle_control(
    bus: &Bus,
    run_id: &str,
    seq: u32,
    control: &mut dyn HitlControl,
) -> ControlOutcome {
    match control.poll() {
        None | Some(RunControl::Resume) => ControlOutcome::Continue(None),
        Some(RunControl::Redirect(instruction)) => {
            let _ = bus.publish_event(&RunPaused {
                run_id: run_id.to_string(),
                by: PausedBy::Human,
            });
            let correction = build_correction(&instruction, seq);
            let _ = bus.publish_event(&RunRedirected {
                run_id: run_id.to_string(),
                instruction,
            });
            let _ = bus.publish_event(&RunResumed {
                run_id: run_id.to_string(),
            });
            ControlOutcome::Continue(Some(correction))
        }
        Some(RunControl::Pause) => {
            let _ = bus.publish_event(&RunPaused {
                run_id: run_id.to_string(),
                by: PausedBy::Human,
            });
            for _ in 0..MAX_PAUSE_POLLS {
                match control.poll() {
                    Some(RunControl::Resume) => {
                        let _ = bus.publish_event(&RunResumed {
                            run_id: run_id.to_string(),
                        });
                        return ControlOutcome::Continue(None);
                    }
                    Some(RunControl::Redirect(instruction)) => {
                        let correction = build_correction(&instruction, seq);
                        let _ = bus.publish_event(&RunRedirected {
                            run_id: run_id.to_string(),
                            instruction,
                        });
                        let _ = bus.publish_event(&RunResumed {
                            run_id: run_id.to_string(),
                        });
                        return ControlOutcome::Continue(Some(correction));
                    }
                    _ => tokio::time::sleep(PAUSE_POLL_INTERVAL).await,
                }
            }
            ControlOutcome::GiveUp
        }
    }
}

/// Build the `human_correction` value recorded on the corrected step when a
/// human redirects mid-run.
///
/// A live redirect fires at the boundary *before* the step it rides on, so
/// that step (`seq`) is the corrected branch and the step recorded
/// immediately before it (`seq - 1`) is the misstep the human is steering
/// away from. The correction therefore records `supersedes_seq = seq - 1`,
/// the exact field the compiler's normalize pass collapses on and the exact
/// shape the hand-authored `contracts/fixtures/trajectory_notepad.json`
/// correction uses (its step 4 supersedes step 3). This is what lets a live
/// correction fold into the compiled workflow identically to the fixture
/// (KI-2). Earlier builds recorded `at_seq`, a name the compiler never read,
/// so a live redirect annotated a step but never collapsed one.
///
/// When the redirect lands before the very first step there is no prior step
/// to supersede, so the correction carries only its instruction; nothing
/// collapses, which is correct.
fn build_correction(instruction: &str, seq: u32) -> serde_json::Value {
    let mut correction = serde_json::Map::new();
    correction.insert("instruction".to_string(), json!(instruction));
    if seq >= 2 {
        correction.insert("supersedes_seq".to_string(), json!(seq - 1));
    }
    serde_json::Value::Object(correction)
}

fn outcome_label(outcome: StepOutcome) -> &'static str {
    match outcome {
        StepOutcome::Ok => "ok",
        StepOutcome::Failed => "failed",
        StepOutcome::Retried => "retried",
    }
}

/// Best-effort lookup of the snapshot element an action's target selectors
/// name, for the safety guard's credential-field invariant. Deliberately
/// simpler than `operant_perception_uia::resolve_in_snapshot` (automation
/// id, then the leaf of a name+role path; no topology/ordinal matching):
/// this is a secondary classification input, not the click-resolution
/// path, and a miss here just means invariant 1 is skipped for this step,
/// same as any action with no element target at all.
fn find_target_element<'a>(snapshot: &'a Snapshot, action: &Action) -> Option<&'a Element> {
    let selectors = action
        .target
        .as_ref()
        .map(|t| t.selectors.as_slice())
        .unwrap_or(&[]);
    for selector in selectors {
        let hit = match selector {
            Selector::AutomationId { value } if !value.is_empty() => snapshot
                .elements
                .iter()
                .find(|e| e.automation_id.as_deref() == Some(value.as_str())),
            Selector::NameRolePath { path } => path.last().and_then(|seg| {
                snapshot
                    .elements
                    .iter()
                    .find(|e| e.name == seg.name && role_matches(e.role, &seg.role))
            }),
            _ => None,
        };
        if hit.is_some() {
            return hit;
        }
    }
    None
}

fn role_matches(role: Role, name: &str) -> bool {
    serde_json::to_value(role)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.eq_ignore_ascii_case(name)))
        .unwrap_or(false)
}

fn build_request(goal: &str, digest: &ElementDigest, history: &[String]) -> CompletionRequest {
    let mut text = format!("Goal: {goal}\n\n{}", digest.to_prompt_text());
    if !history.is_empty() {
        text.push_str("\nSteps taken so far:\n");
        for h in history {
            text.push_str("- ");
            text.push_str(h);
            text.push('\n');
        }
    }
    CompletionRequest {
        role: RequestRole::Planner,
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentPart::Text { text }],
        }],
        tools: planner_tools(),
        max_tokens: 1024,
        temperature: 0.0,
    }
}

fn planner_tools() -> Vec<ToolSchema> {
    // Hand the planner the REAL Action IR JSON Schema (the contract every
    // component speaks), not a bare {"type":"object"}. Without it a real model
    // has to guess the shape and emits actions missing required fields (kind,
    // target, params), which halts the run before a single step. Parsed from the
    // committed contract fixture so the tool schema and the executor can never
    // drift.
    let action_ir_schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../../contracts/action_ir.schema.json"))
            .expect("action_ir.schema.json is a valid committed JSON Schema");
    vec![
        ToolSchema {
            name: PROPOSE_ACTION_TOOL.to_string(),
            description: "Propose the next Action IR step toward the goal. The arguments object IS one Action IR action and must follow the schema: kind is required (one of click, type, key, scroll, drag, wait, assert, adapter_call); a type action needs params.text, a key action needs params.combo (e.g. ctrl+a); target.window selects the app to act on. Set risk_class (read, write, or destructive) and grounding (uia).".to_string(),
            input_schema: action_ir_schema,
        },
        ToolSchema {
            name: DONE_TOOL.to_string(),
            description: "Signal that the goal has been achieved; no more actions are needed."
                .to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}
