// @advanced
// Not Advanced-mode UI copy: this file names the shell-to-core IPC commands
// the command palette issues to start real work (contracts/ipc.md section 5:
// start_explore, dry_run, run_saved_workflow, list_workflows). It is marked
// @advanced only to exempt it from scripts/microcopy_lint.mjs the same way
// ui/src/bus/types.ts is: identifiers like "explore" and "replay" here are
// wire vocabulary, never rendered as UI text.
//
// The seam split mirrors ui/src/bus/mockClient.ts exactly. A BusClient carries
// events core -> shell; a CoreCommands carries commands shell -> core. Today
// createMockCoreCommands drives the same canned bus stream simulateDemoRun
// always produced, so the shell starts runs end to end with no backend process
// (npm run dev / Demo mode) and the flight recorder (lane B3/B4) still fills. A
// real createTauriCoreCommands that invoke()s the sidecar's req/res channel
// (contracts/ipc.md section 2) is a drop-in for it later, the same way
// createTauriBusClient replaces createMockBusClient (docs/specs/ipc-bridge.md
// section 1); the command names and arg shapes here are the frozen contract's,
// so that swap is a transport change, not a rewrite.
//
// Determinism (docs/specs/ipc-bridge.md section 0): start_explore is the
// model-driven teach path (contracts/ipc.md section 5b) and legitimately uses a
// model; dry_run and run_saved_workflow are the offline replay paths and never
// do. This module only STARTS a run; the run streams back over the bus.

import type { BusClient } from "./mockClient.ts";
import { simulateDemoRun } from "./mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY, type RunMode } from "./types.ts";
import type { MockRegistry } from "../library/mockRegistry.ts";

/** The four commands the palette issues (contracts/ipc.md section 5). */
export type CoreCommandName = "start_explore" | "dry_run" | "run_saved_workflow" | "list_workflows";

// dry_run's mode is not a wire constant in ui/src/bus/types.ts (only explore
// and replay are, since main.ts/mockClient.ts need those); the literal is kept
// local here, where this @advanced file may carry wire vocabulary freely.
const RUN_MODE_DRY: RunMode = "dry";

/**
 * One saved-workflow row as list_workflows returns it (contracts/ipc.md
 * section 5c: `[{id,name,version,...}]`). The palette matches over these to
 * build its Workflows group. Shaped so a real list_workflows result drops in
 * unchanged.
 */
export interface WorkflowSummary {
  id: string;
  name: string;
  version: string;
  description: string;
}

/**
 * Supplies start_explore's `window_process` argument: the process name of the
 * OS window that was in front when the palette was summoned, so a teach goal
 * "submits to explore with the current foreground window as context"
 * (docs/specs/ui.md, and ui/src/palette/palette.ts's own header). Resolving the
 * real foreground window is an OS call (GetForegroundWindow ->
 * GetWindowThreadProcessId -> process name) the shell (ui/src-tauri) owns, out
 * of this ui/src lane's path, exactly like the real OS-wide Ctrl+K hotkey. This
 * seam lets ui/src/main.ts inject that shell-provided value and lets dev/Demo,
 * which has no real foreground app, supply a deterministic stub.
 */
export type ForegroundWindowProvider = () => string;

export interface CoreCommands {
  /**
   * start_explore `{goal, window_process}`: the model-driven teach path. The
   * foreground window process rides along as context. Returns a canceller for
   * the dev/Demo canned stream, or null for a blank goal (and, on the real
   * transport, once a run is stopped with the stop command rather than by
   * cancelling local timers). ui/src/main.ts keeps whatever it returns as its
   * stopDemo handle, exactly as it did for submitGoal.
   */
  startExplore(goal: string): (() => void) | null;
  /** dry_run `{path}`: a deterministic, offline preview of a saved workflow. */
  dryRunWorkflow(name: string): void;
  /** run_saved_workflow `{path}`: replay a saved workflow for real (offline). */
  runSavedWorkflow(name: string): void;
  /** list_workflows: the saved-workflow rows the palette matches over. */
  listWorkflows(): WorkflowSummary[];
}

/**
 * dev/Demo default: there is no real OS foreground window when the shell runs
 * in a browser (npm run dev) or under jsdom, so a saved constant stands in. The
 * real value comes from ui/src-tauri via a ForegroundWindowProvider.
 */
export const DEV_FOREGROUND_WINDOW = "explorer.exe";

/**
 * The default provider ui/src/main.ts wires in. In dev/Demo it returns the
 * stub above; the real shell replaces createMockCoreCommands wholesale with a
 * Tauri-backed CoreCommands whose provider reads the live foreground window.
 */
export const readForegroundWindowProcess: ForegroundWindowProvider = () => DEV_FOREGROUND_WINDOW;

export interface MockCoreCommandsOptions {
  /** Source for dry_run/run_saved_workflow/list_workflows in dev/Demo. */
  registry?: MockRegistry;
  /** Supplies start_explore's window_process; defaults to the dev stub. */
  foregroundWindow?: ForegroundWindowProvider;
  stepDelayMs?: number;
  /**
   * The seam where a real transport would invoke() the command onto the
   * sidecar's req/res channel. In dev the mock also drives the canned bus
   * stream so the flight recorder fills; a test can pass this to assert the
   * exact command and args (for example that start_explore carries the goal and
   * the foreground window_process) with no live core.
   */
  onCommand?: (name: CoreCommandName, args: Record<string, unknown>) => void;
}

/**
 * The dev/Demo CoreCommands: issues the contract commands and, because there is
 * no core behind them here, drives the canned bus stream itself so every screen
 * updates exactly as it did before this seam existed. Drop-in replaceable by a
 * real Tauri-backed CoreCommands (see the file header).
 */
export function createMockCoreCommands(bus: BusClient, opts: MockCoreCommandsOptions = {}): CoreCommands {
  const foreground = opts.foregroundWindow ?? readForegroundWindowProcess;

  function issue(name: CoreCommandName, args: Record<string, unknown>): void {
    opts.onCommand?.(name, args);
  }

  function startExplore(goal: string): (() => void) | null {
    const trimmed = goal.trim();
    if (!trimmed) return null;
    issue("start_explore", { goal: trimmed, window_process: foreground() });
    // dev/Demo: no core, so stream the canned teach run (the same one
    // simulateDemoRun always produced) into the flight recorder. On the real
    // transport the run streams back over the bus from the core instead.
    return simulateDemoRun(bus, { goal: trimmed, mode: RUN_MODE_EXPLORE, stepDelayMs: opts.stepDelayMs });
  }

  function replay(name: string, command: "dry_run" | "run_saved_workflow", mode: RunMode): void {
    const record = opts.registry?.get(name);
    if (!record) return;
    issue(command, { path: record.manifest.dsl.path });
    // dev/Demo stream, byte-identical to the run.* pair library.run /
    // previewWorkflow published before this seam existed, so Library's cards,
    // the tray glyph, and the run viewer update exactly as they did. A real
    // core would emit these itself over the bus.
    const runId = `${command}-${name}-${Date.now()}`;
    bus.publish("run.started", { run_id: runId, goal: record.manifest.description, mode, workflow_name: name });
    bus.publish("run.completed", { run_id: runId, outcome: "ok", steps: record.steps.length, wall_ms: 400 });
  }

  return {
    startExplore,
    dryRunWorkflow(name) {
      replay(name, "dry_run", RUN_MODE_DRY);
    },
    runSavedWorkflow(name) {
      replay(name, "run_saved_workflow", RUN_MODE_REPLAY);
    },
    listWorkflows() {
      const records = opts.registry?.list() ?? [];
      return records.map((r) => ({
        id: r.manifest.name,
        name: r.manifest.name,
        version: r.manifest.version,
        description: r.manifest.description,
      }));
    },
  };
}
