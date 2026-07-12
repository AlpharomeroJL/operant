// The teach client: the shell-side seam for the two IPC commands that make
// "teach a task from the app" real -- start_explore and compile_run
// (contracts/ipc.md sections 5b and 5c). A teach entry point (the command
// palette's submit, the wizard's guided first task, a completed teach run's
// Save as workflow) names one of those commands through this client and
// nothing else, so wiring a real Tauri transport onto the core later
// (docs/specs/ipc-bridge.md section 2, ui/src/bus/tauriClient.ts) is a
// same-shape swap of this one factory, not an edit at every call site. This
// is the same seam ui/src/bus/mockClient.ts is for the event bus, scoped to
// the two request/response commands teaching depends on.
//
// The whole present-tense teach flow is: a goal plus the foreground window
// go to start_explore, the produced run streams onto the bus and into the
// flight recorder (ui/src/runViewer), and when it is done compile_run turns
// that run into a saved workflow that shows up in the library
// (ui/src/library, which already upserts on workflow.compiled). "Describe it
// and it does it" -- not "watch you do it once," which is the demonstration
// recorder that does not exist yet (docs/roadmap/demonstration-capture.md).
//
// createMockTeachClient is the demo-build implementation: there is no core
// process, so start_explore streams a canned trajectory onto the same bus a
// live run would (contracts/bus_events.md's run.* topics) and compile_run
// publishes workflow.compiled, so goal -> explore -> watch -> a workflow in
// the library renders end to end offline, the same "renders with no backend
// running" promise the mock bus makes. Determinism stays intact: only teach
// (which legitimately uses a model) is behind this seam. Replay never is --
// it stays model-free by crate graph, and the demonstration recorder, when
// it lands, will feed the same compile step with no model at all.

import type { BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, GROUNDING_UIA, type ActionIR, type RunMode } from "../bus/types.ts";

/**
 * The foreground window a teach run is taught against, named by its process
 * (contracts/ipc.md's start_explore window_process arg). The palette submits
 * the current foreground window as context (docs/specs/ui.md); the wizard's
 * guided task names the bundled practice page instead (see GUIDED_TASK_WINDOW
 * in ui/src/wizard/guidedTask.ts).
 */
export interface StartExploreRequest {
  goal: string;
  windowProcess: string;
  /**
   * Demo-build only: the canned Action IR the mock streams as the explored
   * trajectory. A real core ignores this entirely -- the model produces the
   * trajectory live from the goal and the live window -- so it carries no
   * meaning past the mock. Defaults to a small generic trajectory when
   * omitted, so a caller that only has a goal (the palette) still streams
   * something honest to watch.
   */
  script?: ReadonlyArray<Pick<ActionIR, "kind" | "target" | "params">>;
  /** Overrides the generated run id (tests want a stable id to assert against). */
  runId?: string;
  /** Delay between streamed steps, ms. Tests pass something tiny. */
  stepDelayMs?: number;
}

export interface TeachRun {
  runId: string;
  /** Cancels any steps not yet streamed (the caller left the screen, or started another run). */
  stop(): void;
}

export interface CompileRunOptions {
  /**
   * The saved workflow's name. A real compile_run derives the name from the
   * recorded trajectory core-side and ignores this; the mock has no compiler,
   * so a caller passes the name it wants the library card to show (the wizard
   * passes a fixed id; the shell derives one from the goal via
   * workflowNameFromGoal below). Defaults to a stable id off the run id.
   */
  name?: string;
  version?: string;
}

/**
 * What compile_run hands back: the compiled workflow's identity. A real
 * compile_run returns the full manifest DTO (contracts/ipc.md 5c); the mock
 * returns only what the shell needs to name and cross-reference it, since the
 * library re-reads the manifest from the registry on the workflow.compiled
 * event, not from this result.
 */
export interface CompiledWorkflow {
  name: string;
  version: string;
  sourceRunId: string;
}

export interface TeachClient {
  /**
   * Maps to the start_explore command (contracts/ipc.md 5b): a goal plus the
   * foreground window start a real model-driven explore run, streamed onto
   * the bus as run.* events. Returns the run id (also canonical on
   * run.started) and a stop handle.
   */
  startExplore(req: StartExploreRequest): TeachRun;
  /**
   * Maps to the compile_run command (contracts/ipc.md 5c): the produced run
   * becomes a compiled, saved workflow. Echoes workflow.compiled, which the
   * library listens for, so the workflow appears there with no further
   * wiring.
   */
  compileRun(runId: string, opts?: CompileRunOptions): CompiledWorkflow;
}

/**
 * A placeholder foreground-window name for the demo build, where there is no
 * real OS to ask which window is in front (that query is a Tauri/src-tauri
 * responsibility, out of this lane's owned paths). A real client resolves the
 * actual foreground process before calling start_explore; until then a caller
 * that has no window of its own (the palette) can pass this so the request is
 * still well formed.
 */
export const PLACEHOLDER_FOREGROUND_WINDOW = "operant-foreground";

/**
 * The mock's default explored trajectory, streamed when a caller supplies a
 * goal but no script of its own. Action IR fragments
 * (contracts/action_ir.schema.json), rendered to plain English by the same
 * renderer every run in this shell uses (ui/src/runViewer/sdkRender.ts), so
 * the watched steps read as sentences, never raw data.
 */
const DEFAULT_EXPLORE_SCRIPT: ReadonlyArray<Pick<ActionIR, "kind" | "target" | "params">> = [
  { kind: "click", target: { selectors: [{ kind: "name_role_path", path: [{ role: "button", name: "New" }] }] } },
  { kind: "type", params: { text: "Draft" } },
  { kind: "key", params: { combo: "ctrl+s" } },
];

/**
 * Turns a goal sentence into a stable, DOM-id-safe workflow name for the
 * library card (the mock has no compiler to name the workflow the way a real
 * compile_run would). Lowercased, non-alphanumeric runs collapsed to single
 * hyphens, trimmed, and capped so a long goal does not become an unwieldy id.
 * Falls back to a generic name for a goal that has no usable characters.
 */
export function workflowNameFromGoal(goal: string): string {
  const slug = goal
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48)
    .replace(/-+$/g, "");
  return slug.length > 0 ? slug : "taught-workflow";
}

let runCounter = 0;

export function createMockTeachClient(bus: BusClient): TeachClient {
  function startExplore(req: StartExploreRequest): TeachRun {
    const delay = req.stepDelayMs ?? 450;
    const mode: RunMode = RUN_MODE_EXPLORE;
    const script = req.script ?? DEFAULT_EXPLORE_SCRIPT;
    // A per-call counter keeps two runs started in the same millisecond from
    // colliding on a Date.now()-only id (the wizard can start a demo run and
    // then a real one in quick succession).
    const runId = req.runId ?? `teach-${Date.now()}-${++runCounter}`;
    let cancelled = false;
    const timers: ReturnType<typeof setTimeout>[] = [];

    function schedule(fn: () => void, at: number): void {
      timers.push(
        setTimeout(() => {
          if (!cancelled) fn();
        }, at),
      );
    }

    // run.started fires synchronously (not on the timer) so the flight
    // recorder shows the run as running the instant the caller switches to
    // it, before the first step arrives -- the same shape simulateDemoRun and
    // the old runGuidedTask both used.
    bus.publish("run.started", { run_id: runId, goal: req.goal, mode });

    script.forEach((step, i) => {
      const stepId = `s${i + 1}`;
      schedule(() => {
        // run.step.proposed carries the Action IR and, per
        // contracts/bus_events.md, is only published while teaching (explore),
        // before the checkpoint; that is the frame the run viewer turns into a
        // plain-English sentence.
        bus.publish("run.step.proposed", { run_id: runId, step: { v: 1, id: stepId, ...step } });
        bus.publish("run.step.gated", { run_id: runId, step_id: stepId, gate_kind: "pre", result: "pass" });
        bus.publish("run.step.executed", { run_id: runId, step_id: stepId, outcome: "ok", ms: 90 + i * 10, grounding: GROUNDING_UIA });
      }, (i + 1) * delay);
    });

    schedule(
      () => {
        bus.publish("run.completed", { run_id: runId, outcome: "ok", steps: script.length, wall_ms: script.length * delay });
      },
      (script.length + 1) * delay,
    );

    return {
      runId,
      stop: () => {
        cancelled = true;
        for (const t of timers) clearTimeout(t);
      },
    };
  }

  function compileRun(runId: string, opts: CompileRunOptions = {}): CompiledWorkflow {
    const name = opts.name ?? `taught-${runId.replace(/[^a-z0-9]+/gi, "-").slice(-12).replace(/^-+/, "")}`;
    const version = opts.version ?? "1.0.0";
    // Echo workflow.compiled exactly as a real compile_run would
    // (contracts/bus_events.md WorkflowCompiledPayload); ui/src/library
    // upserts the registry on this event, so the compiled workflow appears as
    // a card with no further wiring.
    bus.publish("workflow.compiled", {
      name,
      version,
      manifest_path: `workflows/${name}.ts`,
      dsl_path: `workflows/${name}.ts`,
      source_run_id: runId,
    });
    return { name, version, sourceRunId: runId };
  }

  return { startExplore, compileRun };
}
