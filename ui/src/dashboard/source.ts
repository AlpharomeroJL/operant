// @advanced
// Exempt from scripts/microcopy_lint.mjs (same reason ui/src/bus/realClient.ts and
// ui/src/bus/commands.ts are): a wire adapter, not shipped UI copy. Its string
// literals are contracts/ipc.md command names and trigger-spec keys ("cron",
// "next", ...), protocol vocabulary that is never rendered as UI text.
// The Home dashboard's real data source (B6 dashboard-live). The dashboard
// reads four request/response commands at mount, contracts/ipc.md's
// get_metrics, list_runs, get_run, and list_triggers. Those are correlated
// req/res commands over the sidecar IPC bridge, NOT bus events, so they ride
// the Tauri invoke channel rather than the pub/sub BusClient the dashboard
// keeps for live run.started/run.completed updates. This file is the seam
// between the two: a small async port (DashboardSource) that
// ui/src/dashboard/state.ts consumes, plus createIpcDashboardSource, the
// adapter that maps the port onto those wire commands.
//
// Same swap-seam discipline as ui/src/bus/mockClient.ts (BusClient) and
// ui/src/undo/realJournal.ts: state.ts depends only on the DashboardSource
// shape below. With no source it falls back to ./mockMetrics.ts fixtures for
// dev/Demo; the moment a real invoke transport is present (createTauriDashboardSource
// resolves one), the real numbers win with no further change in state.ts.
//
// Honesty rule (campaign-critical): a command that is unavailable, errors, or
// answers not_implemented (contracts/ipc.md section 5e: list_triggers has no
// persistent trigger store yet) resolves to the EMPTY result here, never to a
// fabricated one. The dashboard renders the honest empty state from that, not
// invented numbers or fake scheduled runs.

import type { WeeklyMetric } from "./mockMetrics.ts";

/** One finished run as the Recent runs list needs it, distilled from get_run's `{run, steps}`. */
export interface RecentRunData {
  runId: string;
  /**
   * Plain-language description of what ran: the run row's own `goal`
   * (crates/recorder/src/runs.rs RunRecord), already human text, so a real
   * run needs no registry lookup for its title. Falls back to the run id if a
   * run somehow has no goal.
   */
  title: string;
  outcome: "ok" | "failed";
  steps: number;
  completedAtMs: number;
}

/** One upcoming scheduled run, distilled from list_triggers. */
export interface UpcomingRunData {
  workflowName: string;
  /**
   * Already-formatted humane time. The source owns turning a trigger spec into
   * a next-run label because only it knows the schedule; see
   * createIpcDashboardSource. Not reachable today (list_triggers answers
   * not_implemented), so this is the shape the dashboard will consume once a
   * trigger store exists.
   */
  whenLabel: string;
}

/**
 * The commands the dashboard reads at mount (and again on a core restart,
 * contracts/ipc.md section 8b: "re-queries durable state (list_runs, get_run,
 * ...)"). Every method resolves to a value: an unavailable, failed, or
 * not-yet-implemented command resolves to the empty result rather than
 * rejecting, so one missing command never blanks the whole dashboard and the
 * honest empty state is what shows.
 */
export interface DashboardSource {
  /** get_metrics `{weeks}`: the weekly time-saved series, oldest first. `[]` when none recorded (honest empty hero, no fabricated hours). */
  getWeeklyMetrics(weeks: number): Promise<readonly WeeklyMetric[]>;
  /** list_runs + get_run: the most recent finished runs, newest first, at most `limit`. `[]` when nothing has run. */
  getRecentRuns(limit: number): Promise<readonly RecentRunData[]>;
  /** list_triggers: the upcoming scheduled runs. `[]` when the scheduler has none, or answers not_implemented (contracts/ipc.md section 5e). */
  getUpcomingRuns(): Promise<readonly UpcomingRunData[]>;
}

/**
 * A Tauri-style command invoker. Resolves with the command `result` and
 * rejects with the core's typed error (contracts/ipc.md section 2c's
 * `{code, message, retryable}`) on failure. Injected so the adapter and its
 * tests share one shape independent of `@tauri-apps/api`.
 */
export type IpcInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

// --- Wire shapes (defensive: every field is read as unknown and narrowed) ---

/** get_metrics row: `[{week, minutes_saved_total, ...}]` (contracts/ipc.md section 5f; maps to Recorder::get_weekly_system_metrics). */
interface MetricsRowWire {
  week?: unknown;
  minutes_saved_total?: unknown;
}

/** get_run result: `{run, steps}`. `run` mirrors crates/recorder/src/runs.rs RunRecord (serde rename_all = "lowercase"). */
interface RunDetailWire {
  run?: {
    id?: unknown;
    goal?: unknown;
    status?: unknown; // "running" | "completed" | "failed" | "aborted"
    started?: unknown;
    ended?: unknown; // ms since epoch, or null while running
  };
  steps?: unknown; // an array; its length is the step count
}

/** list_triggers row: `{trigger_id, kind, workflow_name, spec, enabled}` (contracts/ipc.md section 5e). */
interface TriggerWire {
  workflow_name?: unknown;
  enabled?: unknown;
  spec?: unknown;
}

function asMinutes(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function asMs(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/** Distil one get_run `{run, steps}` into a Recent runs row, or null if the run is not a finished run to show. */
function toRecentRun(runId: string, detail: RunDetailWire): RecentRunData | null {
  const run = detail.run;
  if (!run) return null;
  const status = typeof run.status === "string" ? run.status : "";
  // Only finished runs belong in Recent runs; a still-running row is skipped.
  if (status === "running" || status === "") return null;
  const outcome: "ok" | "failed" = status === "completed" ? "ok" : "failed";
  const completedAtMs = asMs(run.ended) ?? asMs(run.started) ?? 0;
  const title = typeof run.goal === "string" && run.goal.length > 0 ? run.goal : runId;
  const steps = Array.isArray(detail.steps) ? detail.steps.length : 0;
  return { runId, title, outcome, steps, completedAtMs };
}

/** Best-effort next-run label from a trigger spec. Dormant today (list_triggers answers not_implemented); shows the spec text when the field is a plain string, never a fabricated time. */
function describeSpec(spec: unknown): string {
  if (typeof spec === "string") return spec;
  if (spec && typeof spec === "object") {
    const record = spec as Record<string, unknown>;
    for (const key of ["next", "when", "cron", "expr"]) {
      if (typeof record[key] === "string") return record[key] as string;
    }
  }
  return "";
}

/**
 * The real DashboardSource over a Tauri invoke channel. Faithful to
 * contracts/ipc.md's command set, and honest by construction: every command's
 * failure (including list_triggers' not_implemented) is caught and turned into
 * the empty result, so the dashboard never fabricates hours or scheduled runs.
 */
export function createIpcDashboardSource(invoke: IpcInvoke): DashboardSource {
  return {
    async getWeeklyMetrics(weeks) {
      let raw: unknown;
      try {
        raw = await invoke("get_metrics", { weeks });
      } catch {
        // Metrics unavailable: honest empty hero, never the mock 3.2 hours.
        return [];
      }
      if (!Array.isArray(raw)) return [];
      // Preserved order: contracts/ipc.md's get_metrics returns the weekly
      // series oldest first, which is exactly the left-to-right order the
      // sparkline plots (see ./state.ts computeSparklinePoints).
      return raw.map((entry) => {
        const row = entry as MetricsRowWire;
        return {
          week: typeof row.week === "string" ? row.week : "",
          minutesSaved: asMinutes(row.minutes_saved_total),
        };
      });
    },

    async getRecentRuns(limit) {
      let ids: unknown;
      try {
        ids = await invoke("list_runs");
      } catch {
        return [];
      }
      if (!Array.isArray(ids)) return [];
      const rows: RecentRunData[] = [];
      // list_runs is already most-recent-first (crates/recorder/src/runs.rs:160,
      // ORDER BY started DESC). Walk it, fetching each run's detail, until we
      // have `limit` finished runs.
      for (const id of ids) {
        if (rows.length >= limit) break;
        if (typeof id !== "string") continue;
        let detail: unknown;
        try {
          detail = await invoke("get_run", { run_id: id });
        } catch {
          continue;
        }
        const row = toRecentRun(id, (detail ?? {}) as RunDetailWire);
        if (row) rows.push(row);
      }
      return rows;
    },

    async getUpcomingRuns() {
      let raw: unknown;
      try {
        raw = await invoke("list_triggers");
      } catch {
        // contracts/ipc.md section 5e: list_triggers is NOT-YET-IMPLEMENTED and
        // answers error code `not_implemented`. Per the campaign, that (and any
        // other failure) shows the empty "nothing scheduled yet" state, never
        // fake scheduled runs.
        return [];
      }
      if (!Array.isArray(raw)) return [];
      const rows: UpcomingRunData[] = [];
      for (const entry of raw) {
        const trigger = entry as TriggerWire;
        if (trigger.enabled === false) continue;
        const workflowName = typeof trigger.workflow_name === "string" ? trigger.workflow_name : "";
        if (workflowName.length === 0) continue;
        rows.push({ workflowName, whenLabel: describeSpec(trigger.spec) });
      }
      return rows;
    },
  };
}

/**
 * Resolve the Tauri invoke channel if the shell is running inside Tauri, else
 * undefined. contracts/ipc.md / docs/specs/ipc-bridge.md section 1: the webview
 * selects the real client when the Tauri globals are present, and the mock
 * (dev/Demo) otherwise. Read defensively (no `@tauri-apps/api` dependency) so
 * dev, `vite build`, and the node test runner, none of which have these
 * globals, all resolve to undefined and fall back to fixtures.
 */
function resolveTauriInvoke(): IpcInvoke | undefined {
  const globals = globalThis as {
    __TAURI__?: { core?: { invoke?: unknown }; invoke?: unknown };
    __TAURI_INTERNALS__?: { invoke?: unknown };
  };
  const candidates: unknown[] = [
    globals.__TAURI__?.core?.invoke, // Tauri v2 with withGlobalTauri
    globals.__TAURI__?.invoke, // Tauri v1
    globals.__TAURI_INTERNALS__?.invoke, // Tauri v2 internals fallback
  ];
  for (const candidate of candidates) {
    if (typeof candidate === "function") return candidate as IpcInvoke;
  }
  return undefined;
}

/**
 * The DashboardSource for the real shell, or undefined in dev/Demo/tests. This
 * is the one line ui/src/main.ts adds: `createDashboard(bus, { registry,
 * source: createTauriDashboardSource() })`. Off-Tauri it returns undefined and
 * the dashboard stays on ./mockMetrics.ts fixtures, exactly as before.
 */
export function createTauriDashboardSource(): DashboardSource | undefined {
  const invoke = resolveTauriInvoke();
  return invoke ? createIpcDashboardSource(invoke) : undefined;
}
