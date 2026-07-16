// @advanced
// Wire vocabulary, not UI copy: this module names the contracts/ipc.md section
// 5 commands (start_explore, start_replay, compile_run, ...) in string
// literals, so it is exempt from scripts/microcopy_lint.mjs the same way
// ui/src/bus/realClient.ts and ui/src/bus/commands.ts are.
//
// THE ONE command layer (the Phase 2 integration reconciliation). Every Phase 2
// lane invented an injectable seam for UI->core commands/queries, each
// mock-backed off-Tauri and meant to be real invoke-backed in Tauri. This file
// provides the single real implementation of each, all backed by ONE primitive:
// coreCall, which is B2's `core_call` Tauri command (ui/src-tauri/src/bridge/mod.rs).
// ui/src/main.ts builds one coreCall when B3's coreGate reports the core is real
// (isTauri + capabilities ok) and constructs these seams from it, injecting the
// same instances into every screen; off-Tauri/Demo the screens keep their mock
// seams. There is one coreCall and one of each seam, never N.
//
// Command names are audited against contracts/ipc.md section 5 here, at the one
// place the real command strings are written. The known UI/contract mismatch is
// fixed here: running a saved workflow, which the UI lanes called
// `run_saved_workflow` (docs/specs/ipc-bridge.md's older name), issues the
// frozen contract command `start_replay` (section 5b). B1's serve loop accepts
// only the contract names, so this is the source of truth for what goes on the
// wire.

import type { CommandClient, CommandError, CommandResult } from "../bus/commandClient.ts";
import type { CoreCommands, WorkflowSummary } from "../bus/commands.ts";
import type { MockRegistry } from "../library/mockRegistry.ts";
import type { TargetWindow } from "../palette/targetApp.ts";
import type {
  CompileRunOptions,
  CompiledWorkflow,
  StartExploreRequest,
  TeachClient,
  TeachRun,
} from "../teach/client.ts";
import { workflowNameFromGoal } from "../teach/client.ts";
import type { UndoCommands } from "../undo/state.ts";
import type { PanicClient } from "../safety/panic.ts";
import type {
  SchedulerCommands,
  TriggerRecord,
  UpsertTriggerArgs,
  UpsertTriggerResult,
} from "../scheduler/commands.ts";

/**
 * The one request/response primitive: one call is one `req` frame over B2's
 * `core_call` Tauri command, resolving with the `res` result on ok:true and
 * rejecting with the core's typed error (contracts/ipc.md 2c `{code, message,
 * retryable}`) on ok:false, exactly as B2 maps `Result<Value, CoreError>`.
 */
export type CoreCall = <T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/**
 * Wraps a raw Tauri invoke into coreCall: every command rides `core_call`. The
 * generic mirrors the seams' own invoke shape (ui/src/settings/liveStore.ts's
 * InvokeFn); the result is whatever the core's `res` carries, narrowed by the
 * caller exactly as the existing adapters already do defensively.
 */
export function makeCoreCall(invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>): CoreCall {
  return <T,>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
    invoke("core_call", { cmd, args: args ?? {} }) as Promise<T>;
}

/**
 * Normalize a caught core_call rejection into the contract error shape. B2
 * rejects with `{code, message, retryable}` for both a core-level "no"
 * (not_implemented, refused, ...) and a transport fault (core_unavailable,
 * core_timeout); either way the seams below surface it as a resolved
 * `{ok:false}` so callers branch on `res.ok` and never need a try/catch (the
 * CommandClient/SchedulerCommands contract), and the honest-state UI shows it.
 */
export function toCommandError(err: unknown): CommandError {
  if (err && typeof err === "object") {
    const o = err as Record<string, unknown>;
    if (typeof o.code === "string") {
      return { code: o.code, message: typeof o.message === "string" ? o.message : "", retryable: Boolean(o.retryable) };
    }
  }
  return { code: "internal", message: typeof err === "string" ? err : "core call failed", retryable: true };
}

async function callResult<T>(coreCall: CoreCall, cmd: string, args?: Record<string, unknown>): Promise<CommandResult<T>> {
  try {
    return { ok: true, result: await coreCall<T>(cmd, args) };
  } catch (err) {
    return { ok: false, error: toCommandError(err) };
  }
}

/** B5's request/response bridge (library list_workflows/start_replay/explain_workflow), real over coreCall. */
export function createRealCommandClient(coreCall: CoreCall): CommandClient {
  return {
    request<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<CommandResult<T>> {
      return callResult<T>(coreCall, cmd, args);
    },
  };
}

export interface RealCoreCommandsOptions {
  /** The shared registry (kept in sync with the core by B5's library list_workflows load); listWorkflows reads it, staying synchronous. */
  registry: MockRegistry;
}

/**
 * B7's palette command seam, real over coreCall. start_explore/dry_run are the
 * contract names as-is; running a saved workflow issues `start_replay` (the
 * name fix). The real core streams the started run's events onto the bus
 * itself, so these NEVER synthesize canned events (that is the mock's job).
 * listWorkflows reads the shared registry, so it stays synchronous like the
 * palette expects.
 *
 * start_explore's window_process is now the process the target-app picker
 * resolved (ADR 0003, A1: ui/src/palette/targetApp.ts), passed in by main.ts,
 * not a foreground-window guess. listWindows fetches the list that picker shows.
 */
export function createRealCoreCommands(coreCall: CoreCall, opts: RealCoreCommandsOptions): CoreCommands {
  const { registry } = opts;
  function pathOf(name: string): string | null {
    const record = registry.get(name);
    if (!record) return null;
    return record.path ?? record.manifest.dsl.path;
  }
  return {
    startExplore(goal: string, windowProcess?: string): (() => void) | null {
      const trimmed = goal.trim();
      if (!trimmed) return null;
      void coreCall("start_explore", { goal: trimmed, window_process: windowProcess });
      // The run streams from the core over the bus; stopping is the `stop`
      // command (the run viewer's Stop button), not a local timer cancel.
      return null;
    },
    listWindows(): Promise<TargetWindow[]> {
      // list_windows returns the open windows z-ordered topmost-first with
      // Operant's own window already excluded (contracts/ipc.md); the picker
      // pre-selects windows[0] as the app the person was last in.
      return coreCall<{ windows?: TargetWindow[] }>("list_windows").then((res) => res?.windows ?? []);
    },
    dryRunWorkflow(name: string): void {
      const path = pathOf(name);
      if (path === null) return;
      void coreCall("dry_run", { path });
    },
    runSavedWorkflow(name: string): void {
      const path = pathOf(name);
      if (path === null) return;
      // NAME FIX: UI `run_saved_workflow` -> contract `start_replay` (section 5b).
      void coreCall("start_replay", { path });
    },
    listWorkflows(): WorkflowSummary[] {
      return registry.list().map((r) => ({
        id: r.manifest.name,
        name: r.manifest.name,
        version: r.manifest.version,
        description: r.manifest.description,
      }));
    },
  };
}

let realTeachRunCounter = 0;

/** B16's teach seam (start_explore + compile_run), real over coreCall. */
export function createRealTeachClient(coreCall: CoreCall): TeachClient {
  return {
    startExplore(req: StartExploreRequest): TeachRun {
      void coreCall("start_explore", { goal: req.goal, window_process: req.windowProcess });
      // run.started is the canonical source of the run id (the UI reads it off
      // the bus); this best-effort id is only a handle for a caller that has
      // nothing better. stop is the `stop` command, not a local canceller.
      const runId = req.runId ?? `teach-${Date.now()}-${++realTeachRunCounter}`;
      return { runId, stop: () => {} };
    },
    compileRun(runId: string, opts: CompileRunOptions = {}): CompiledWorkflow {
      void coreCall("compile_run", { run_id: runId });
      // The core echoes workflow.compiled with the real manifest; the library
      // reads that from the event, not from this best-effort identity.
      const name = opts.name ?? workflowNameFromGoal("");
      return { name, version: opts.version ?? "1.0.0", sourceRunId: runId };
    },
  };
}

/** B10's undo seam (preview_undo + undo_run), real over coreCall. The core echoes undo.previewed/undo.applied on the bus, which the undo screen renders. */
export function createRealUndoCommands(coreCall: CoreCall): UndoCommands {
  return {
    previewUndo(runId: string): void {
      void coreCall("preview_undo", { run_id: runId });
    },
    undoRun(runId: string): void {
      void coreCall("undo_run", { run_id: runId });
    },
  };
}

/**
 * B11's two-path kill switch, real over coreCall. `stop` is the cooperative
 * close; `kill` fires BOTH path 1 (the core's `kill` command: in-process freeze
 * plus the killswitch.engaged echo) AND path 2 (B2's `core_kill` Tauri command:
 * the unblockable hard terminate of the child). Both run because a wedged core
 * can swallow the cooperative path and only the hard terminate is guaranteed
 * (contracts/ipc.md section 5b).
 */
export function createRealPanicClient(coreCall: CoreCall, coreKill: () => Promise<unknown>): PanicClient {
  return {
    stop(runId?: string): void {
      void coreCall("stop", runId ? { run_id: runId } : {});
    },
    kill(): void {
      void coreCall("kill", {});
      void coreKill();
    },
  };
}

/**
 * B13's scheduler seam (list_triggers + upsert_trigger), real over coreCall.
 * Both commands are reserved-but-unwired in the contract (section 5g), so the
 * real core answers `not_implemented`; that arrives here as a resolved
 * `{ok:false, error:{code:"not_implemented"}}`, which the library and dashboard
 * already surface as the honest "scheduling isn't available yet" state. No
 * fabrication: this genuinely asks the core rather than assuming the answer.
 */
export function createRealScheduler(coreCall: CoreCall): SchedulerCommands {
  return {
    listTriggers(): Promise<CommandResult<TriggerRecord[]>> {
      return callResult<TriggerRecord[]>(coreCall, "list_triggers");
    },
    upsertTrigger(args: UpsertTriggerArgs): Promise<CommandResult<UpsertTriggerResult>> {
      return callResult<UpsertTriggerResult>(coreCall, "upsert_trigger", { ...args });
    },
  };
}
