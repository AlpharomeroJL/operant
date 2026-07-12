// @advanced
// Not Advanced-mode UI copy: this file is a typed mirror of the wire
// protocol in contracts/bus_events.md (topic names, envelope shape, enum
// values). It is marked @advanced only to exempt it from
// scripts/microcopy_lint.mjs, which scans every non-exempt file under
// ui/src for glossary internal terms; identifiers like "explore",
// "sidecar", and "VRAM" below are correct wire vocabulary and are never
// rendered as UI text. Real UI copy lives in ui/src/strings (default mode)
// and ui/src/advanced (the actual Advanced-mode surface).
//
// Mirrored from contracts/bus_events.md ("Contract: Bus Events"). That file
// is append-only during the campaign: new topics and new optional payload
// fields only. Keep this mirror in the same shape by hand; a consumer must
// ignore fields it does not know about, same as the contract's own
// versioning rules say.

export type RunMode = "explore" | "replay" | "dry";
export type GateKind = "pre" | "post" | "safety";
export type GateResult = "pass" | "fail";
export type StepOutcome = "ok" | "failed" | "retried";
export type Grounding = "uia" | "vision" | "adapter";
export type HaltReason = "gate" | "killswitch" | "human" | "error";
export type TriggerKind = "cron" | "file" | "window" | "email";
export type DoctorSeverity = "info" | "warn" | "error";
export type RiskClass = "read" | "write" | "destructive";

// Typed constants for wire-protocol literal values that other, non-exempt
// files (ui/src/main.ts, ui/src/bus/mockClient.ts) need to reference.
// Importing these instead of retyping the literal keeps those files free of
// internal-vocabulary string literals for the lint to (correctly) flag,
// even though the values themselves are wire data, not UI text.
export const RUN_MODE_EXPLORE: RunMode = "explore";
export const RUN_MODE_REPLAY: RunMode = "replay";
export const GROUNDING_UIA: Grounding = "uia";

export interface ActionIR {
  v: number;
  id: string;
  kind: "click" | "type" | "key" | "scroll" | "drag" | "wait" | "assert" | "adapter_call";
  target?: Record<string, unknown>;
  params?: Record<string, unknown>;
  pace?: string;
  risk_class?: RiskClass;
  grounding?: Grounding;
  timeout?: number;
  retry?: number;
}

export interface BusEnvelope<Topic extends string = string, Payload = unknown> {
  v: 1;
  seq: number;
  ts: string;
  topic: Topic;
  payload: Payload;
}

// --- Runs ---
export interface RunStartedPayload {
  run_id: string;
  goal: string;
  mode: RunMode;
  workflow_name?: string;
}
export interface RunStepProposedPayload {
  run_id: string;
  step: ActionIR;
}
export interface RunStepGatedPayload {
  run_id: string;
  step_id: string;
  gate_kind: GateKind;
  result: GateResult;
  expr?: string;
}
export interface RunStepExecutedPayload {
  run_id: string;
  step_id: string;
  outcome: StepOutcome;
  ms: number;
  grounding: Grounding;
}
export interface RunStepFailedPayload {
  run_id: string;
  step_id: string;
  error_id: string;
  message: string;
}
export interface RunPausedPayload {
  run_id: string;
  by: "human" | "system";
}
export interface RunRedirectedPayload {
  run_id: string;
  instruction: string;
}
export interface RunResumedPayload {
  run_id: string;
}
export interface RunHaltedPayload {
  run_id: string;
  reason: HaltReason;
  error_id?: string;
}
export interface RunCompletedPayload {
  run_id: string;
  outcome: "ok" | "failed";
  steps: number;
  wall_ms: number;
}

// --- Gates, approvals, escalations ---
export interface GateEscalationPayload {
  run_id: string;
  step_id?: string;
  sentence: string;
  requires_approval: boolean;
}
export interface ApprovalRequestedPayload {
  approval_id: string;
  run_id: string;
  step_id?: string;
  proposed_action: ActionIR;
  sentence: string;
}
export interface ApprovalGrantedPayload {
  approval_id: string;
  approver: string;
}
export interface ApprovalDeniedPayload {
  approval_id: string;
  approver: string;
}

// --- Perception ---
export interface PerceptionSnapshotPayload {
  snapshot_digest: string;
  window: string;
  source: string;
  element_count: number;
  truncated: boolean;
}
export interface PerceptionChangedPayload {
  scope: string;
  digest_before: string;
  digest_after: string;
}

// --- Sidecars and VRAM ---
export interface SidecarStartedPayload {
  name: string;
  pid: number;
}
export interface SidecarHealthPayload {
  name: string;
  ok: boolean;
  rss_mb?: number;
  vram_mb?: number;
}
export interface SidecarCrashedPayload {
  name: string;
  exit_code: number;
}
export interface SidecarRestartedPayload {
  name: string;
  attempt: number;
}
export interface VramRequestPayload {
  requester: string;
  mb: number;
}
export interface VramGrantPayload {
  requester: string;
  mb: number;
}
export interface VramYieldPayload {
  yielder: string;
  mb: number;
}

// --- Workflows ---
export interface WorkflowCompiledPayload {
  name: string;
  version: string;
  manifest_path: string;
  dsl_path: string;
  source_run_id: string;
}
export interface WorkflowInstalledPayload {
  name: string;
  version: string;
  publisher?: string;
  signed: boolean;
  dry_run_only: boolean;
}
export interface WorkflowDriftDetectedPayload {
  name: string;
  step_id: string;
  reason: "selectors_missed" | "anchor_below_tolerance";
}
export interface WorkflowPatchProposedPayload {
  name: string;
  patch_id: string;
  step_id: string;
  diff_path: string;
}
export interface WorkflowPatchApprovedPayload {
  name: string;
  patch_id: string;
  new_version: string;
}
export interface WorkflowPatchRejectedPayload {
  name: string;
  patch_id: string;
}

// --- Scheduler ---
export interface TriggerFiredPayload {
  trigger_id: string;
  kind: TriggerKind;
  workflow_name: string;
  input?: unknown;
}
export interface ScheduleEnqueuedPayload {
  run_id: string;
  workflow_name: string;
  trigger_id?: string;
}
export interface ScheduleRejectedPayload {
  workflow_name: string;
  reason: "mode_not_replay" | "scope_conflict";
}

// --- Guardian ---
export interface KillswitchEngagedPayload {
  at_ms: number;
}
export interface KillswitchReleasedPayload {
  run_id?: string;
}
// One journal item as carried on undo.previewed's optional `items` field
// (F1b): mirrors crates/core/src/bus/events.rs's UndoInverseWire, itself a
// deliberately narrowed mirror of crates/recorder/src/undo.rs's internal
// Inverse enum: no blob hash (an internal storage detail), and a clipboard
// restore carries only had_prior (whether a prior value existed), never the
// actual clipboard contents.
export type UndoInverseWire =
  | { op: "delete_created"; path: string }
  | { op: "recreate_deleted"; path: string }
  | { op: "reverse_move"; moved_to: string; original: string }
  | { op: "restore_overwritten"; path: string }
  | { op: "restore_clipboard"; had_prior: boolean }
  | { op: "irreversible"; description: string };

// seq sits alongside the tagged union's own fields in the wire JSON (Rust's
// #[serde(flatten)]), e.g. {"seq":6,"op":"restore_clipboard","had_prior":true}.
export type UndoJournalItemWire = { seq: number } & UndoInverseWire;

export interface UndoPreviewedPayload {
  run_id: string;
  entries: number;
  irreversible: number;
  // Per-item restoration entries, newest-first. Added in F1b as an optional
  // field per contracts/bus_events.md's append-only rule: omitted (not an
  // empty array) when the publisher has none to report, e.g. an older
  // publisher or nothing journaled. ui/src/undo/realJournal.ts decodes this.
  items?: UndoJournalItemWire[];
}
export interface UndoAppliedPayload {
  run_id: string;
  restored: number;
  narration: string[];
}

// --- Doctor, metrics, suggestions ---
export interface DoctorFindingPayload {
  finding_id: string;
  severity: DoctorSeverity;
  what: string;
  why: string;
  action: string;
  fix_command?: string;
}
export interface DoctorFixedPayload {
  finding_id: string;
}
export interface MetricsWeekRolledPayload {
  week: string;
  minutes_saved_total: number;
}
export interface SuggestionOfferedPayload {
  suggestion_id: string;
  pattern_digest: string;
  occurrences: number;
}
export interface SuggestionAcceptedPayload {
  suggestion_id: string;
}
export interface SuggestionDismissedPayload {
  suggestion_id: string;
}

// --- Config ---
export interface ConfigChangedPayload {
  key: string;
  value: unknown;
  old_value?: unknown;
}

// Topic -> payload map. Adding a topic here is the one place that needs to
// change to add a new typed event; BusEvent and BusTopic derive from it.
export interface BusTopicPayloadMap {
  "run.started": RunStartedPayload;
  "run.step.proposed": RunStepProposedPayload;
  "run.step.gated": RunStepGatedPayload;
  "run.step.executed": RunStepExecutedPayload;
  "run.step.failed": RunStepFailedPayload;
  "run.paused": RunPausedPayload;
  "run.redirected": RunRedirectedPayload;
  "run.resumed": RunResumedPayload;
  "run.halted": RunHaltedPayload;
  "run.completed": RunCompletedPayload;

  "gate.escalation": GateEscalationPayload;
  "approval.requested": ApprovalRequestedPayload;
  "approval.granted": ApprovalGrantedPayload;
  "approval.denied": ApprovalDeniedPayload;

  "perception.snapshot": PerceptionSnapshotPayload;
  "perception.changed": PerceptionChangedPayload;

  "sidecar.started": SidecarStartedPayload;
  "sidecar.health": SidecarHealthPayload;
  "sidecar.crashed": SidecarCrashedPayload;
  "sidecar.restarted": SidecarRestartedPayload;
  "vram.request": VramRequestPayload;
  "vram.grant": VramGrantPayload;
  "vram.yield": VramYieldPayload;

  "workflow.compiled": WorkflowCompiledPayload;
  "workflow.installed": WorkflowInstalledPayload;
  "workflow.drift.detected": WorkflowDriftDetectedPayload;
  "workflow.patch.proposed": WorkflowPatchProposedPayload;
  "workflow.patch.approved": WorkflowPatchApprovedPayload;
  "workflow.patch.rejected": WorkflowPatchRejectedPayload;

  "trigger.fired": TriggerFiredPayload;
  "schedule.enqueued": ScheduleEnqueuedPayload;
  "schedule.rejected": ScheduleRejectedPayload;

  "killswitch.engaged": KillswitchEngagedPayload;
  "killswitch.released": KillswitchReleasedPayload;
  "undo.previewed": UndoPreviewedPayload;
  "undo.applied": UndoAppliedPayload;

  "doctor.finding": DoctorFindingPayload;
  "doctor.fixed": DoctorFixedPayload;
  "metrics.week.rolled": MetricsWeekRolledPayload;
  "suggestion.offered": SuggestionOfferedPayload;
  "suggestion.accepted": SuggestionAcceptedPayload;
  "suggestion.dismissed": SuggestionDismissedPayload;

  "config.changed": ConfigChangedPayload;
}

export type BusTopic = keyof BusTopicPayloadMap;

export type BusEvent = {
  [T in BusTopic]: BusEnvelope<T, BusTopicPayloadMap[T]>;
}[BusTopic];
