// The Home dashboard (docs/specs/design.md section 3): "the new default
// window view." A hero line in plain language plus a sparkline of the last
// 8 weeks, an Up next list of scheduled runs, a Recent runs list, and a
// quiet empty state inviting teaching the first workflow. Turns
// ./mockMetrics.ts's weekly-metrics fixture plus run.started/run.completed bus
// events (contracts/bus_events.md) into the snapshot ./view.ts renders. Pure
// and DOM-free, same split as ui/src/library/state.ts and ui/src/tray/state.ts.
//
// Up next is NOT fabricated: it is sourced from the list_triggers command
// (contracts/ipc.md section 5e) via the injected scheduler surface. The core
// has no trigger store yet, so list_triggers answers `not_implemented`
// (section 5g) and Up next honestly reports scheduling is not available rather
// than inventing future runs. See docs/roadmap/scheduler-live.md for what the
// core needs before this list can carry real entries.

import type { BusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { commonStrings, dashboardStrings } from "../strings/default.ts";
import { dashboardCopyStrings } from "./strings.ts";
import { WEEKLY_METRICS_FIXTURE, type WeeklyMetric } from "./mockMetrics.ts";
import { createMockRegistry, type MockRegistry } from "../library/mockRegistry.ts";
import { createUnavailableSchedulerCommands, isNotImplemented, type SchedulerCommands, type TriggerRecord } from "../scheduler/commands.ts";

export interface UpNextRow {
  workflowName: string;
  title: string;
  /**
   * A plain description of when this trigger runs. Sourced from the trigger's
   * own spec as returned by list_triggers (contracts/ipc.md section 5e); the
   * shell never computes a next-fire time itself (that is the core scheduler's
   * job, docs/roadmap/scheduler-live.md). No row is ever shown in this build:
   * the core has no trigger store, so list_triggers returns nothing.
   */
  whenLabel: string;
}

export type RunOutcome = "ok" | "failed";

export interface RecentRunRow {
  workflowName: string;
  title: string;
  outcomeLabel: string;
  whenLabel: string;
  status: RunOutcome;
  /** The status dot's visually-hidden text equivalent (design.md section 3: "status dot" alongside plain text). */
  statusLabel: string;
}

export interface SparklinePoint {
  x: number;
  y: number;
}

export interface DashboardSnapshot {
  title: string;
  heroLine: string;
  /** Raw minutes-saved values, oldest to newest, for anything ./view.ts or a test needs beyond the plotted points. */
  sparklineValues: readonly number[];
  /** Pre-computed plot coordinates (an SVG viewBox of SPARKLINE_WIDTH x SPARKLINE_HEIGHT): view.ts only ever joins these into a points="" attribute, no layout math of its own. */
  sparklinePoints: readonly SparklinePoint[];
  /** The sparkline is aria-hidden (a trend glyph, not the sole carrier of the numbers: the hero line already states this week's total in words); this is its visually-hidden text equivalent. */
  sparklineSummary: string;
  upNextTitle: string;
  upNext: readonly UpNextRow[];
  upNextEmptyLabel: string;
  /**
   * True when scheduling itself is not wired: the list_triggers command
   * answered `not_implemented` (contracts/ipc.md section 5g). ./view.ts shows
   * upNextUnavailableLabel instead of upNextEmptyLabel in this case, so the
   * screen says "scheduling is not available yet" rather than the weaker
   * "nothing scheduled yet," which would imply scheduling works and is merely
   * empty. Always true in this build.
   */
  upNextUnavailable: boolean;
  upNextUnavailableLabel: string;
  recentRunsTitle: string;
  recentRuns: readonly RecentRunRow[];
  recentRunsEmptyLabel: string;
  /** True only when there is nothing scheduled and nothing has run yet at all: design.md's "quiet empty state that invites teaching the first workflow." */
  empty: boolean;
  emptyLabel: string;
  /**
   * H1 (docs/specs/design.md section 3's Wizard finish screen, reused here
   * for the dashboard's own first-run invite: "a single amber 'Teach your
   * first workflow' button"). Only meaningful while `empty` is true;
   * ./view.ts renders it as the one specific action the empty state's copy
   * rule calls for, alongside `emptyLabel`'s quiet explanatory sentence.
   */
  emptyActionLabel: string;
}

export interface Dashboard {
  getSnapshot(): DashboardSnapshot;
  subscribe(fn: (snap: DashboardSnapshot) => void): () => void;
  dispose(): void;
}

export interface CreateDashboardOptions {
  /** Shared with ui/src/library/state.ts's registry in ui/src/main.ts, so Up next/Recent runs show the same plain-language titles Library does. Defaults to its own seeded registry so the dashboard still renders end to end standalone (tests, a lone accessibility scan). */
  registry?: MockRegistry;
  now?: () => number;
  /** Last 8 weeks, oldest first. Defaults to ./mockMetrics.ts's fixture. */
  weeklyMetrics?: readonly WeeklyMetric[];
  /**
   * The scheduler command surface (contracts/ipc.md section 5e) that feeds Up
   * next. Defaults to the honest not-yet-wired implementation, which answers
   * list_triggers with `not_implemented` because the core has no trigger store
   * yet (see ../scheduler/commands.ts). Up next is driven entirely by this: it
   * is never fabricated from a fixture.
   */
  scheduler?: SchedulerCommands;
  /** Oldest recent run dropped once this many are held. */
  recentRunsLimit?: number;
}

const RECENT_RUNS_DEFAULT_LIMIT = 5;
/** The sparkline's SVG viewBox, in user units; ui/src/styles/base.css sizes the element itself on screen. */
export const SPARKLINE_WIDTH = 160;
export const SPARKLINE_HEIGHT = 40;
const SPARKLINE_PADDING = 4;

function formatMinutesList(values: readonly number[]): string {
  return values.join(", ");
}

/** "3.2 hours" / "1 hour" / "0 hours": rounds to one decimal, drops a trailing ".0", and gets "hour" vs "hours" right. */
function formatHoursPhrase(minutes: number): string {
  const hours = minutes / 60;
  const rounded = Math.round(hours * 10) / 10;
  const text = Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
  const unit = rounded === 1 ? "hour" : "hours";
  return `${text} ${unit}`;
}

/** Same shape as ui/src/library/state.ts's formatWhen (kept local rather than shared: both are small, screen-owned pure functions, not shared infrastructure). */
function formatRelative(atMs: number, nowMs: number): string {
  const diffMs = Math.max(0, nowMs - atMs);
  const minutes = Math.round(diffMs / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.round(hours / 24);
  return `${days} day${days === 1 ? "" : "s"} ago`;
}

function computeSparklinePoints(values: readonly number[]): SparklinePoint[] {
  if (values.length === 0) return [];
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min;
  const usableWidth = SPARKLINE_WIDTH - SPARKLINE_PADDING * 2;
  const usableHeight = SPARKLINE_HEIGHT - SPARKLINE_PADDING * 2;
  return values.map((v, i) => {
    const x = values.length === 1 ? SPARKLINE_WIDTH / 2 : SPARKLINE_PADDING + (usableWidth * i) / (values.length - 1);
    // Flat data (span === 0, including a single point) plots as a level
    // mid-height line rather than dividing by zero.
    const normalized = span === 0 ? 0.5 : (v - min) / span;
    const y = SPARKLINE_PADDING + usableHeight * (1 - normalized);
    return { x: Math.round(x * 100) / 100, y: Math.round(y * 100) / 100 };
  });
}

interface RecentRunRecord {
  workflowName: string;
  title: string;
  outcome: RunOutcome;
  steps: number;
  completedAtMs: number;
}

export function createDashboard(bus: BusClient, opts: CreateDashboardOptions = {}): Dashboard {
  const now = opts.now ?? (() => Date.now());
  const registry = opts.registry ?? createMockRegistry();
  const weeklyMetrics = opts.weeklyMetrics ?? WEEKLY_METRICS_FIXTURE;
  const scheduler = opts.scheduler ?? createUnavailableSchedulerCommands();
  const recentRunsLimit = opts.recentRunsLimit ?? RECENT_RUNS_DEFAULT_LIMIT;

  const pendingRuns = new Map<string, { workflowName: string }>();
  let recentRunRecords: RecentRunRecord[] = [];
  // Up next state, sourced from the list_triggers command, never a fixture.
  // Defaults to "unavailable" (not merely empty): the honest starting point is
  // that scheduling is off until list_triggers proves otherwise, so no snapshot
  // ever momentarily implies a working-but-empty scheduler. The probe below
  // confirms it (or, once a real store exists, fills in rows).
  let upNext: UpNextRow[] = [];
  let scheduleUnavailable = true;
  let disposed = false;
  const listeners = new Set<(snap: DashboardSnapshot) => void>();

  function titleFor(workflowName: string): string {
    return registry.get(workflowName)?.manifest.description || workflowName;
  }

  function upNextRowFor(trigger: TriggerRecord): UpNextRow {
    return {
      workflowName: trigger.workflow_name,
      title: titleFor(trigger.workflow_name),
      // The configured spec, shown verbatim: the shell does not compute a
      // next-fire time (that is the core scheduler's job). A friendlier
      // "tomorrow at 9 am" label needs a core-supplied next-fire field on
      // list_triggers, tracked in docs/roadmap/scheduler-live.md.
      whenLabel: trigger.spec,
    };
  }

  // Wire Up next to list_triggers (contracts/ipc.md section 5e). The core has
  // no trigger store yet, so this resolves `not_implemented` and Up next stays
  // unavailable; when a real store lands, the same call fills in real rows with
  // no change here. Fire-and-forget: the constructor stays synchronous and the
  // result arrives through emit(), which ui/src/main.ts already re-renders on.
  void scheduler
    .listTriggers()
    .then((res) => {
      if (disposed) return;
      if (res.ok) {
        upNext = res.result.map(upNextRowFor);
        scheduleUnavailable = false;
      } else {
        upNext = [];
        scheduleUnavailable = isNotImplemented(res);
      }
      emit();
    })
    .catch(() => {
      // A transport-level failure is not a schedule; leave Up next unavailable
      // rather than inventing entries.
    });

  function recentRunRows(nowMs: number): RecentRunRow[] {
    return recentRunRecords.map((r) => ({
      workflowName: r.workflowName,
      title: r.title,
      outcomeLabel: r.outcome === "ok" ? dashboardStrings.outcomeOk(r.steps) : dashboardStrings.outcomeFailed,
      whenLabel: formatRelative(r.completedAtMs, nowMs),
      status: r.outcome,
      statusLabel: r.outcome === "ok" ? dashboardCopyStrings.statusOkLabel : dashboardCopyStrings.statusFailedLabel,
    }));
  }

  function snapshot(): DashboardSnapshot {
    const nowMs = now();
    const values = weeklyMetrics.map((m) => m.minutesSaved);
    const thisWeekMinutes = values[values.length - 1] ?? 0;
    const recentRuns = recentRunRows(nowMs);
    return {
      title: dashboardStrings.title,
      heroLine: dashboardStrings.heroLine(formatHoursPhrase(thisWeekMinutes)),
      sparklineValues: values,
      sparklinePoints: computeSparklinePoints(values),
      sparklineSummary: dashboardStrings.sparklineSummary(formatMinutesList(values)),
      upNextTitle: dashboardStrings.upNextTitle,
      upNext,
      upNextEmptyLabel: dashboardStrings.upNextEmpty,
      upNextUnavailable: scheduleUnavailable,
      upNextUnavailableLabel: dashboardStrings.upNextUnavailable,
      recentRunsTitle: dashboardStrings.recentRunsTitle,
      recentRuns,
      recentRunsEmptyLabel: dashboardStrings.recentRunsEmpty,
      // The quiet first-run invite shows only when there is genuinely nothing
      // to show: no upcoming triggers and no runs. Scheduling being unavailable
      // does not force the invite (a person with runs still sees their runs);
      // it just means Up next carries no rows, which it never does in this
      // build.
      empty: upNext.length === 0 && recentRuns.length === 0,
      emptyLabel: dashboardStrings.emptyInvite,
      emptyActionLabel: commonStrings.teachFirstWorkflow,
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function handle(event: BusEvent): void {
    switch (event.topic) {
      case "run.started": {
        // Same guard as ui/src/library/state.ts: an explore/teach run not
        // yet tied to a saved workflow has no stable name to show in a
        // "Recent runs" row, so it is not tracked at all here.
        if (!event.payload.workflow_name) return;
        pendingRuns.set(event.payload.run_id, { workflowName: event.payload.workflow_name });
        return;
      }
      case "run.completed": {
        const pending = pendingRuns.get(event.payload.run_id);
        if (!pending) return;
        pendingRuns.delete(event.payload.run_id);
        const record: RecentRunRecord = {
          workflowName: pending.workflowName,
          title: titleFor(pending.workflowName),
          outcome: event.payload.outcome === "ok" ? "ok" : "failed",
          steps: event.payload.steps,
          completedAtMs: now(),
        };
        recentRunRecords = [record, ...recentRunRecords].slice(0, recentRunsLimit);
        emit();
        return;
      }
      default:
        return;
    }
  }

  const unsubscribe = bus.subscribe("*", handle);

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    dispose() {
      disposed = true;
      unsubscribe();
      listeners.clear();
    },
  };
}
