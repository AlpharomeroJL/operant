// The wizard's guided first task (docs/specs/zero-code.md screen 4): "a
// suggested goal against the fixture web app, narrated explore, ending on
// one button labeled 'Save as workflow'". contracts/fixtures/webapp/index.html
// is the fixture: a one-page invoice form (Customer, Amount, Date, "Save
// invoice"). This publishes the same run.* bus events
// (contracts/bus_events.md) a live run would, the same "renders end to end
// with no backend process running" promise ui/src/bus/mockClient.ts's own
// simulateDemoRun makes for the command palette; this is that same idea
// scoped to the wizard's own goal and steps, not a copy of that function
// (ui/src/bus/mockClient.ts is out of this lane's owned path).
//
// Pure aside from the bus publishes it makes on a timer: no DOM. Runs under
// plain `node --test`.

import type { BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, GROUNDING_UIA, type ActionIR, type RunMode } from "../bus/types.ts";

export const GUIDED_TASK_GOAL = "Fill out a sample invoice on the practice page";

function field(name: string): ActionIR["target"] {
  return { selectors: [{ kind: "name_role_path", path: [{ role: "textbox", name }] }] };
}

function button(name: string): ActionIR["target"] {
  return { selectors: [{ kind: "name_role_path", path: [{ role: "button", name }] }] };
}

/**
 * Action IR fragments (contracts/action_ir.schema.json shape), not
 * hand-written sentences: sdk/ts/src/render turns these into the plain
 * English the guided-task screen and the run viewer both show, from the
 * same renderer every other run in this shell uses.
 */
export const GUIDED_TASK_STEPS: ReadonlyArray<Pick<ActionIR, "kind" | "target" | "params">> = [
  { kind: "type", target: field("Customer"), params: { text: "Acme Co" } },
  { kind: "type", target: field("Amount"), params: { text: "420.00" } },
  { kind: "type", target: field("Date"), params: { text: "2026-01-15" } },
  { kind: "click", target: button("Save invoice") },
];

export interface RunGuidedTaskOptions {
  /** demo mode publishes no capabilities on completion; real mode is otherwise identical. */
  demo?: boolean;
  runId?: string;
  mode?: RunMode;
  stepDelayMs?: number;
}

/**
 * Streams GUIDED_TASK_STEPS onto the bus on a timer, exactly like a live
 * teaching run would. Returns a stop function that cancels any steps not
 * yet published (the wizard calls this if the user leaves the screen
 * mid-run).
 */
export function runGuidedTask(bus: BusClient, opts: RunGuidedTaskOptions = {}): { runId: string; stop: () => void } {
  const delay = opts.stepDelayMs ?? 450;
  const mode = opts.mode ?? RUN_MODE_EXPLORE;
  const runId = opts.runId ?? `wizard-${opts.demo ? "demo" : "task"}-${Date.now()}`;
  let cancelled = false;
  const timers: ReturnType<typeof setTimeout>[] = [];

  function schedule(fn: () => void, at: number): void {
    timers.push(
      setTimeout(() => {
        if (!cancelled) fn();
      }, at),
    );
  }

  bus.publish("run.started", { run_id: runId, goal: GUIDED_TASK_GOAL, mode });

  GUIDED_TASK_STEPS.forEach((step, i) => {
    const stepId = `t${i + 1}`;
    schedule(() => {
      if (mode === RUN_MODE_EXPLORE) {
        bus.publish("run.step.proposed", { run_id: runId, step: { v: 1, id: stepId, ...step } });
      }
      bus.publish("run.step.gated", { run_id: runId, step_id: stepId, gate_kind: "pre", result: "pass" });
      bus.publish("run.step.executed", {
        run_id: runId,
        step_id: stepId,
        outcome: "ok",
        ms: 90 + i * 10,
        grounding: GROUNDING_UIA,
      });
    }, (i + 1) * delay);
  });

  schedule(() => {
    bus.publish("run.completed", {
      run_id: runId,
      outcome: "ok",
      steps: GUIDED_TASK_STEPS.length,
      wall_ms: GUIDED_TASK_STEPS.length * delay,
    });
  }, (GUIDED_TASK_STEPS.length + 1) * delay);

  return {
    runId,
    stop: () => {
      cancelled = true;
      for (const t of timers) clearTimeout(t);
    },
  };
}
