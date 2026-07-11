// The run viewer's state (C13, FR-O1): turns run.* bus events
// (contracts/bus_events.md) into what the screen needs to show, and turns
// Stop/Pause/intervene into the right bus publishes. Pure and DOM-free, so
// it can run under plain `node --test`; main.ts binds it to the page, the
// same split used by every other module in ui/src (ui/src/bus/mockClient.ts,
// ui/src/state/mode.ts).
//
// docs/specs/ui.md: "run viewer (step list streaming, each row is the
// plain-English sentence plus a status dot, model ON/OFF indicator top
// right, Stop and Pause buttons, intervene text field when paused)".

import type { BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, type BusEvent } from "../bus/types.ts";
import { renderStepSentence } from "./sdkRender.ts";
import { runStateStrings, runViewerStrings } from "../strings/default.ts";

/** Human-language run states. No jargon: these are the only words shown for a run's state. */
export type RunState = "idle" | "running" | "paused" | "halted" | "done";

export type StepStatus = "pending" | "ok" | "failed" | "retried";

export interface StepRow {
  id: string;
  sentence: string;
  status: StepStatus;
}

export interface RunViewerSnapshot {
  runId: string | null;
  runState: RunState;
  runStateLabel: string;
  steps: StepRow[];
  /** null before any run has ever started; true/false (on/off) once one has. */
  modelOn: boolean | null;
  modelIndicatorLabel: string;
  showIntervene: boolean;
  pauseButtonLabel: string;
  canPause: boolean;
  canStop: boolean;
}

export interface RunViewer {
  getSnapshot(): RunViewerSnapshot;
  /** Notified with a fresh snapshot after every state-changing bus event. */
  subscribe(fn: (snapshot: RunViewerSnapshot) => void): () => void;
  /** Ends the current run (publishes run.halted, reason human). No-op with no run under way. */
  stop(): void;
  /** Pauses a running run, or resumes a paused one. No-op with no run under way. */
  togglePause(): void;
  /**
   * Sends a redirect instruction and resumes the run. Only valid while
   * paused with non-blank text; returns whether it actually did anything.
   */
  intervene(instruction: string): boolean;
  dispose(): void;
}

interface InternalState {
  runId: string | null;
  runState: RunState;
  steps: StepRow[];
  modelOn: boolean | null;
}

const INITIAL_STATE: InternalState = { runId: null, runState: "idle", steps: [], modelOn: null };

function upsertStep(steps: StepRow[], id: string, status: StepStatus, sentence?: string): StepRow[] {
  const idx = steps.findIndex((row) => row.id === id);
  if (idx === -1) {
    const fallback = sentence ?? runViewerStrings.stepFallback(steps.length + 1);
    return [...steps, { id, status, sentence: fallback }];
  }
  const next = steps.slice();
  next[idx] = sentence ? { ...next[idx], status, sentence } : { ...next[idx], status };
  return next;
}

function toSnapshot(s: InternalState): RunViewerSnapshot {
  const paused = s.runState === "paused";
  return {
    runId: s.runId,
    runState: s.runState,
    runStateLabel: runStateStrings[s.runState],
    steps: s.steps,
    modelOn: s.modelOn,
    modelIndicatorLabel: s.modelOn === null ? "" : s.modelOn ? runViewerStrings.modelOn : runViewerStrings.modelOff,
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

  function handle(event: BusEvent): void {
    switch (event.topic) {
      case "run.started": {
        state = {
          runId: event.payload.run_id,
          runState: "running",
          steps: [],
          modelOn: event.payload.mode === RUN_MODE_EXPLORE,
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
        const sentence = renderStepSentence(irStep, runViewerStrings.stepFallback(state.steps.length + 1));
        state = { ...state, steps: upsertStep(state.steps, irStep.id, "pending", sentence) };
        break;
      }
      case "run.step.executed": {
        if (event.payload.run_id !== state.runId) return;
        state = { ...state, steps: upsertStep(state.steps, event.payload.step_id, event.payload.outcome) };
        break;
      }
      case "run.step.failed": {
        if (event.payload.run_id !== state.runId) return;
        state = { ...state, steps: upsertStep(state.steps, event.payload.step_id, "failed") };
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

  function stop(): void {
    if (!state.runId) return;
    bus.publish("run.halted", { run_id: state.runId, reason: "human" });
  }

  function togglePause(): void {
    if (!state.runId) return;
    if (state.runState === "running") {
      bus.publish("run.paused", { run_id: state.runId, by: "human" });
    } else if (state.runState === "paused") {
      bus.publish("run.resumed", { run_id: state.runId });
    }
  }

  function intervene(instruction: string): boolean {
    const trimmed = instruction.trim();
    if (!trimmed || !state.runId || state.runState !== "paused") return false;
    bus.publish("run.redirected", { run_id: state.runId, instruction: trimmed });
    bus.publish("run.resumed", { run_id: state.runId });
    return true;
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
    dispose,
  };
}
