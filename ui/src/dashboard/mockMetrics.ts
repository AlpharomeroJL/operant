// Seed data for the Home dashboard (docs/specs/design.md section 3): the
// same render-end-to-end-with-nothing-else-running goal
// ui/src/bus/mockClient.ts and ui/src/library/mockRegistry.ts state in their
// own header comments. Two things live here that no bus topic can supply:
//
//   - A weekly time-saved history. contracts/bus_events.md's
//     metrics.week.rolled only ever carries the *current* week's running
//     total (week, minutes_saved_total), never a trailing series, so there
//     is no live source for "the last 8 weeks" design.md's hero sparkline
//     needs. This fixture stands in until a real metrics store can answer
//     "give me the last N weeks."
//   - A schedule of upcoming runs. contracts/bus_events.md's scheduler
//     topics (trigger.fired, schedule.enqueued) only ever describe a
//     trigger that already fired, never a future one; nothing in the wire
//     protocol models "what's scheduled next" yet. This fixture stands in
//     for that until a real scheduler exists to ask.
//
// Swap either for a real data source later; ui/src/dashboard/state.ts only
// ever consumes the shapes below, the same same-shape-swap seam
// mockRegistry.ts documents for the workflow registry.

export interface WeeklyMetric {
  /** Plain label for the week, oldest to newest order in the array below; not parsed, only ever shown as-is if a future UI needs per-point labels. */
  week: string;
  minutesSaved: number;
}

/**
 * Last 8 weeks, oldest first, this week last. The last entry (192 minutes =
 * 3.2 hours) is deliberately the literal example in docs/specs/design.md
 * section 3: "Operant saved you 3.2 hours this week."
 */
export const WEEKLY_METRICS_FIXTURE: readonly WeeklyMetric[] = [
  { week: "7 weeks ago", minutesSaved: 45 },
  { week: "6 weeks ago", minutesSaved: 60 },
  { week: "5 weeks ago", minutesSaved: 30 },
  { week: "4 weeks ago", minutesSaved: 90 },
  { week: "3 weeks ago", minutesSaved: 120 },
  { week: "2 weeks ago", minutesSaved: 80 },
  { week: "Last week", minutesSaved: 150 },
  { week: "This week", minutesSaved: 192 },
];

export interface UpcomingRunFixture {
  workflowName: string;
  /** 0 = today, 1 = tomorrow, and so on; combined with hour/minute by ui/src/dashboard/state.ts's own now()-relative clock so "tomorrow" is always actually tomorrow, whenever "now" is. */
  daysFromNow: number;
  /** 0-23, local time (this shell has no server-side timezone to reconcile against; see state.ts's formatHumaneTime). */
  hour: number;
  minute: number;
}

/**
 * Two of these three names match ui/src/library/mockRegistry.ts's seed
 * workflows exactly, so when ui/src/main.ts wires the dashboard to the same
 * MockRegistry instance the library uses, "Up next" shows the real plain-
 * language title instead of the raw workflow name. A dashboard created
 * without that registry (or with a different one) still renders correctly:
 * ui/src/dashboard/state.ts falls back to the raw name.
 */
export const UP_NEXT_FIXTURE: readonly UpcomingRunFixture[] = [
  { workflowName: "weekly-report-email", daysFromNow: 1, hour: 9, minute: 0 },
  { workflowName: "backup-photos", daysFromNow: 3, hour: 20, minute: 30 },
];
