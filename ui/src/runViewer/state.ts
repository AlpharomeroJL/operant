// The run viewer's state (C13, FR-O1): turns run.* bus events
// (contracts/bus_events.md) into what the screen needs to show, and turns
// Stop/Pause/intervene into the right run-control commands. Pure and DOM-free, so
// it can run under plain `node --test`; main.ts binds it to the page, the
// same split used by every other module in ui/src (ui/src/bus/mockClient.ts,
// ui/src/state/mode.ts).
//
// docs/specs/ui.md: "run viewer (step list streaming, each row is the
// plain-English sentence plus a status dot, model ON/OFF indicator top
// right, Stop and Pause buttons, intervene text field when paused)".

import type { BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, type BusEvent, type EvtSidecar, type GateKind, type RunStepThumb } from "../bus/types.ts";
import { renderStepSentence, type RenderableStep } from "./sdkRender.ts";
import { runStateStrings, runViewerStrings } from "../strings/default.ts";

/** Human-language run states. No jargon: these are the only words shown for a run's state. */
export type RunState = "idle" | "running" | "paused" | "halted" | "done";

export type StepStatus = "pending" | "ok" | "failed" | "retried";

/**
 * A safety check that did not pass for a step (a failed `run.step.gated`,
 * contracts/bus_events.md). Its presence on a StepRow is what makes the run
 * viewer draw the inline card design.md section 3 calls for ("Failed gates
 * render as an inline card in the list, not a modal"); a passing check leaves
 * no trace here. `expr` is the raw check expression, kept for Advanced mode
 * only and never rendered as default-mode copy.
 */
export interface StepGate {
  kind: GateKind;
  expr?: string;
}

export interface StepRow {
  id: string;
  sentence: string;
  status: StepStatus;
  /** Wall time to run this step, from run.step.executed (docs/specs/design.md section 3: "duration in mono"). Absent until the step has actually executed. */
  durationMs?: number;
  /** Set only when a safety check failed for this step; drives the inline card. */
  gate?: StepGate;
  /**
   * The redacted screenshot thumbnail that rode this step's `evt` frame
   * (contracts/ipc.md section 7), already redacted and downscaled by the
   * producer. Absent when the frame carried none (a headless/mock core, or a
   * frame with `thumb: null`), which is when the filmstrip draws its generated
   * placeholder instead (ui/src/runViewer/thumbnails.ts).
   */
  thumb?: RunStepThumb;
}

/**
 * Which mode chip the flight recorder shows (docs/specs/design.md section 3):
 * the amber recording chip while teaching, or the quiet gray no-AI chip while
 * running a saved workflow. Null before any run has started. (`exact` is the
 * saved-workflow discriminant; the chip's user-facing wording lives in
 * ui/src/strings/default.ts.)
 */
export type RunChip = "rec" | "exact";

export interface RunViewerSnapshot {
  runId: string | null;
  runState: RunState;
  runStateLabel: string;
  steps: StepRow[];
  /** null before any run has ever started; true/false (on/off) once one has. */
  modelOn: boolean | null;
  modelIndicatorLabel: string;
  /** The mode chip to show (rec/replay), or null before any run has started. Derived from modelOn. */
  runChip: RunChip | null;
  /**
   * The step the user has scrubbed to on the filmstrip, or null while the
   * strip is auto-following the live run. Scrub sync is symmetric: this is set
   * by selecting a filmstrip frame OR a step row, and both highlight off
   * `activeStepId` below, so selecting either one highlights the other.
   */
  selectedStepId: string | null;
  /**
   * The step the filmstrip and step list highlight right now: the scrubbed-to
   * step if the user picked one, otherwise the latest step (auto-follow, so a
   * live run's strip keeps pace with the newest frame on its own,
   * docs/specs/design.md section 3). Null when there are no steps yet.
   */
  activeStepId: string | null;
  showIntervene: boolean;
  pauseButtonLabel: string;
  canPause: boolean;
  canStop: boolean;
}

export interface RunViewer {
  getSnapshot(): RunViewerSnapshot;
  /** Notified with a fresh snapshot after every state-changing bus event. */
  subscribe(fn: (snapshot: RunViewerSnapshot) => void): () => void;
  /** Ends the current run (sends the stop command; the core closes it as halted). No-op with no run under way. */
  stop(): void;
  /** Pauses a running run, or resumes a paused one, via the pause/resume commands. No-op with no run under way. */
  togglePause(): void;
  /**
   * Sends a redirect command with the instruction; the core captures the
   * correction and resumes the run on its own. Only valid while paused with
   * non-blank text; returns whether it actually did anything.
   */
  intervene(instruction: string): boolean;
  /**
   * Scrub the filmstrip/step list to a step (or null to hand control back to
   * auto-follow). No-op for a step id that is not in the current run. DOM-free:
   * main.ts and view.ts key their highlight off the resulting snapshot.
   */
  select(stepId: string | null): void;
  dispose(): void;
}

interface InternalState {
  runId: string | null;
  runState: RunState;
  steps: StepRow[];
  modelOn: boolean | null;
  selectedStepId: string | null;
}

const INITIAL_STATE: InternalState = { runId: null, runState: "idle", steps: [], modelOn: null, selectedStepId: null };

/** Fields of a StepRow a bus event can set or update; `id` is fixed once the row exists. */
type StepPatch = Partial<Omit<StepRow, "id">>;

function upsertStep(steps: StepRow[], id: string, patch: StepPatch): StepRow[] {
  const idx = steps.findIndex((row) => row.id === id);
  if (idx === -1) {
    const row: StepRow = {
      id,
      sentence: patch.sentence ?? runViewerStrings.stepFallback(steps.length + 1),
      status: patch.status ?? "pending",
    };
    if (patch.durationMs !== undefined) row.durationMs = patch.durationMs;
    if (patch.gate !== undefined) row.gate = patch.gate;
    if (patch.thumb !== undefined) row.thumb = patch.thumb;
    return [...steps, row];
  }
  const next = steps.slice();
  next[idx] = { ...next[idx], ...patch };
  return next;
}

function toSnapshot(s: InternalState): RunViewerSnapshot {
  const paused = s.runState === "paused";
  const lastStepId = s.steps.length ? s.steps[s.steps.length - 1].id : null;
  // Auto-follow: with nothing scrubbed to, the highlight tracks the newest
  // step, so a live run's filmstrip keeps pace on its own (design.md section
  // 3). A scrubbed-to step pins the highlight until the user clears it.
  const activeStepId = s.selectedStepId ?? lastStepId;
  return {
    runId: s.runId,
    runState: s.runState,
    runStateLabel: runStateStrings[s.runState],
    steps: s.steps,
    modelOn: s.modelOn,
    modelIndicatorLabel: s.modelOn === null ? "" : s.modelOn ? runViewerStrings.modelOn : runViewerStrings.modelOff,
    runChip: s.modelOn === null ? null : s.modelOn ? "rec" : "exact",
    selectedStepId: s.selectedStepId,
    activeStepId,
    showIntervene: paused,
    pauseButtonLabel: paused ? runViewerStrings.resume : runViewerStrings.pause,
    canPause: s.runState === "running" || paused,
    canStop: s.runState === "running" || paused,
  };
}

export function createRunViewer(bus: BusClient): RunViewer {
  let state: InternalState = INITIAL_STATE;
  const listeners = new Set<(snapshot: RunViewerSnapshot) => void>();

  function emit(): void {
    const snapshot = toSnapshot(state);
    for (const fn of listeners) fn(snapshot);
  }

  function handle(event: BusEvent, sidecar?: EvtSidecar): void {
    switch (event.topic) {
      case "run.started": {
        state = {
          runId: event.payload.run_id,
          runState: "running",
          steps: [],
          modelOn: event.payload.mode === RUN_MODE_EXPLORE,
          // A fresh run starts on auto-follow, whatever the last run was scrubbed to.
          selectedStepId: null,
        };
        break;
      }
      case "run.step.proposed": {
        if (event.payload.run_id !== state.runId) return;
        // Keep this typed as the Action IR (contracts/action_ir.schema.json)
        // the payload actually declares, not the renderer's RenderableStep
        // union: the SDK's own author-shape half of that union has no `id`,
        // so narrowing to it early would lose the field this code needs.
        const irStep = event.payload.step;
        // ActionIR is a fixed wire interface with no string index signature, while
        // the renderer's RenderableStep escape hatch is Record<string, unknown>.
        // Cast to bridge them: the renderer is total over every real Action IR kind
        // (sdk/ts/test/render-totality.test.js), and irStep keeps its ActionIR type
        // below for `.id`. Cast through unknown since neither type is structurally
        // assignable to the other.
        const sentence = renderStepSentence(
          irStep as unknown as RenderableStep,
          runViewerStrings.stepFallback(state.steps.length + 1),
        );
        // A proposed frame may carry a thumbnail too (contracts/ipc.md section
        // 7); attach it when present so an early frame can already show one.
        const patch: StepPatch = { status: "pending", sentence };
        if (sidecar?.thumb) patch.thumb = sidecar.thumb;
        state = { ...state, steps: upsertStep(state.steps, irStep.id, patch) };
        break;
      }
      case "run.step.gated": {
        if (event.payload.run_id !== state.runId) return;
        // A passing check is silent; only a failed one leaves a mark, which is
        // what the run viewer turns into an inline card (design.md section 3).
        if (event.payload.result !== "fail") return;
        state = {
          ...state,
          steps: upsertStep(state.steps, event.payload.step_id, {
            gate: { kind: event.payload.gate_kind, expr: event.payload.expr },
          }),
        };
        break;
      }
      case "run.step.executed": {
        if (event.payload.run_id !== state.runId) return;
        // The executed frame is the one the flight recorder renders, so it is
        // where the redacted thumbnail rides (contracts/ipc.md section 7).
        const patch: StepPatch = { status: event.payload.outcome, durationMs: event.payload.ms };
        if (sidecar?.thumb) patch.thumb = sidecar.thumb;
        state = { ...state, steps: upsertStep(state.steps, event.payload.step_id, patch) };
        break;
      }
      case "run.step.failed": {
        if (event.payload.run_id !== state.runId) return;
        state = { ...state, steps: upsertStep(state.steps, event.payload.step_id, { status: "failed" }) };
        break;
      }
      case "run.paused": {
        if (event.payload.run_id !== state.runId) return;
        state = { ...state, runState: "paused" };
        break;
      }
      case "run.resumed": {
        if (event.payload.run_id !== state.runId) return;
        state = { ...state, runState: "running" };
        break;
      }
      case "run.halted": {
        if (event.payload.run_id !== state.runId) return;
        state = { ...state, runState: "halted" };
        break;
      }
      case "run.completed": {
        if (event.payload.run_id !== state.runId) return;
        state = { ...state, runState: "done" };
        break;
      }
      default:
        return;
    }
    emit();
  }

  const unsubscribeBus = bus.subscribe("run", handle);

  // Stop/Pause/intervene SEND run-control commands to the core rather than
  // publishing core-owned run.* events themselves (contracts/ipc.md section 5b;
  // docs/specs/ipc-bridge.md section 8b). The core is the sole author of run.*;
  // it echoes the resulting event back, and the handlers above render it. The
  // mock bus stands in for that echo (ui/src/bus/mockClient.ts), so the round
  // trip works identically on the mock and against a live core.

  function stop(): void {
    if (!state.runId) return;
    bus.command({ cmd: "stop", run_id: state.runId });
  }

  function togglePause(): void {
    if (!state.runId) return;
    if (state.runState === "running") {
      bus.command({ cmd: "pause", run_id: state.runId });
    } else if (state.runState === "paused") {
      bus.command({ cmd: "resume", run_id: state.runId });
    }
  }

  function intervene(instruction: string): boolean {
    const trimmed = instruction.trim();
    if (!trimmed || !state.runId || state.runState !== "paused") return false;
    // Redirect captures the correction and resumes on its own (the core echoes
    // run.redirected then run.resumed, control.rs); no separate resume command.
    bus.command({ cmd: "redirect", run_id: state.runId, instruction: trimmed });
    return true;
  }

  function select(stepId: string | null): void {
    // Ignore a scrub to a step that is not part of this run; clearing (null)
    // is always allowed and returns to auto-follow.
    if (stepId !== null && !state.steps.some((row) => row.id === stepId)) return;
    if (state.selectedStepId === stepId) return;
    state = { ...state, selectedStepId: stepId };
    emit();
  }

  function dispose(): void {
    unsubscribeBus();
    listeners.clear();
  }

  return {
    getSnapshot: () => toSnapshot(state),
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    stop,
    togglePause,
    intervene,
    select,
    dispose,
  };
}
