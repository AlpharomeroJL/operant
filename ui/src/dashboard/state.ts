// The Home dashboard (docs/specs/design.md section 3): "the new default
// window view." A hero line in plain language plus a sparkline of the last
// 8 weeks, an Up next list of scheduled runs, a Recent runs list, and a
// quiet empty state inviting teaching the first workflow. Turns
// ./mockMetrics.ts's fixtures plus run.started/run.completed bus events
// (contracts/bus_events.md) into the snapshot ./view.ts renders. Pure and
// DOM-free, same split as ui/src/library/state.ts and ui/src/tray/state.ts.

import type { BusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { dashboardStrings } from "../strings/default.ts";
import { dashboardCopyStrings } from "./strings.ts";
import { WEEKLY_METRICS_FIXTURE, UP_NEXT_FIXTURE, type WeeklyMetric, type UpcomingRunFixture } from "./mockMetrics.ts";
import { createMockRegistry, type MockRegistry } from "../library/mockRegistry.ts";

export interface UpNextRow {
  workflowName: string;
  title: string;
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
  recentRunsTitle: string;
  recentRuns: readonly RecentRunRow[];
  recentRunsEmptyLabel: string;
  /** True only when there is nothing scheduled and nothing has run yet at all: design.md's "quiet empty state that invites teaching the first workflow." */
  empty: boolean;
  emptyLabel: string;
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
  /** Defaults to ./mockMetrics.ts's fixture. Pass [] to exercise the empty Up next state. */
  upNext?: readonly UpcomingRunFixture[];
  /** Oldest recent run dropped once this many are held. */
  recentRunsLimit?: number;
}

const RECENT_RUNS_DEFAULT_LIMIT = 5;
/** The sparkline's SVG viewBox, in user units; ui/src/styles/base.css sizes the element itself on screen. */
export const SPARKLINE_WIDTH = 160;
export const SPARKLINE_HEIGHT = 40;
const SPARKLINE_PADDING = 4;
const MS_PER_DAY = 86_400_000;

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

/** Local midnight for the given instant; see formatHumaneTime's header note on why this shell uses local time throughout rather than reconciling a server timezone. */
function startOfDayMs(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** Resolves a fixture's relative-to-now schedule (daysFromNow/hour/minute) into an absolute instant, so "tomorrow at 9" is always actually tomorrow relative to whatever now() returns. */
function targetMsFor(fixture: UpcomingRunFixture, nowMs: number): number {
  const d = new Date(nowMs);
  d.setDate(d.getDate() + fixture.daysFromNow);
  d.setHours(fixture.hour, fixture.minute, 0, 0);
  return d.getTime();
}

/**
 * "tomorrow at 9 am" (design.md section 3's own example, plus an am/pm
 * suffix design.md's prose elides but a real schedule should not: "9" alone
 * is genuinely ambiguous between morning and evening). This shell has no
 * server-side timezone to reconcile against (single-user desktop app,
 * ui/src-tauri's own machine), so, like the rest of this codebase (e.g.
 * ui/src/library/state.ts's formatWhen), this reads plain local Date
 * getters throughout rather than normalizing to UTC first.
 */
function formatHumaneTime(targetMs: number, nowMs: number): string {
  const dayDiff = Math.round((startOfDayMs(targetMs) - startOfDayMs(nowMs)) / MS_PER_DAY);
  const target = new Date(targetMs);

  let dayWord: string;
  if (dayDiff === 0) dayWord = dashboardCopyStrings.today;
  else if (dayDiff === 1) dayWord = dashboardCopyStrings.tomorrow;
  else if (dayDiff > 1 && dayDiff < 7) dayWord = dashboardCopyStrings.weekdayNames[target.getDay()];
  else if (dayDiff >= 7) dayWord = dashboardCopyStrings.inDays(dayDiff);
  else dayWord = dashboardCopyStrings.weekdayNames[target.getDay()]; // defensive: a past instant is not expected from this fixture's always-future offsets

  const hours24 = target.getHours();
  const minutes = target.getMinutes();
  const ampm = hours24 < 12 ? "am" : "pm";
  const hour12raw = hours24 % 12;
  const hour12 = hour12raw === 0 ? 12 : hour12raw;
  const minutePart = minutes === 0 ? "" : `:${String(minutes).padStart(2, "0")}`;
  return `${dayWord} at ${hour12}${minutePart} ${ampm}`;
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
  const upNextFixture = opts.upNext ?? UP_NEXT_FIXTURE;
  const recentRunsLimit = opts.recentRunsLimit ?? RECENT_RUNS_DEFAULT_LIMIT;

  const pendingRuns = new Map<string, { workflowName: string }>();
  let recentRunRecords: RecentRunRecord[] = [];
  const listeners = new Set<(snap: DashboardSnapshot) => void>();

  function titleFor(workflowName: string): string {
    return registry.get(workflowName)?.manifest.description || workflowName;
  }

  function upNextRows(nowMs: number): UpNextRow[] {
    return upNextFixture.map((f) => ({
      workflowName: f.workflowName,
      title: titleFor(f.workflowName),
      whenLabel: formatHumaneTime(targetMsFor(f, nowMs), nowMs),
    }));
  }

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
    const upNext = upNextRows(nowMs);
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
      recentRunsTitle: dashboardStrings.recentRunsTitle,
      recentRuns,
      recentRunsEmptyLabel: dashboardStrings.recentRunsEmpty,
      empty: upNext.length === 0 && recentRuns.length === 0,
      emptyLabel: dashboardStrings.emptyInvite,
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
      unsubscribe();
      listeners.clear();
    },
  };
}
