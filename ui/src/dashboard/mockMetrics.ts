// Seed data for the Home dashboard (docs/specs/design.md section 3): the
// same render-end-to-end-with-nothing-else-running goal
// ui/src/bus/mockClient.ts and ui/src/library/mockRegistry.ts state in their
// own header comments. One thing lives here that no bus topic can supply:
//
//   - A weekly time-saved history. contracts/bus_events.md's
//     metrics.week.rolled only ever carries the *current* week's running
//     total (week, minutes_saved_total), never a trailing series, so there
//     is no live source for "the last 8 weeks" design.md's hero sparkline
//     needs. This fixture stands in until a real metrics store can answer
//     "give me the last N weeks."
//
// Up next used to be seeded here too, from a fabricated list of "upcoming
// runs." It no longer is: scheduling has a real command (list_triggers,
// contracts/ipc.md section 5e) that the dashboard now calls, and the honest
// answer today is that the core has no trigger store, so nothing is upcoming.
// Inventing future runs a scheduler cannot actually fire would be exactly the
// dishonesty docs/roadmap/scheduler-live.md exists to avoid.
//
// Swap this for a real data source later; ui/src/dashboard/state.ts only
// ever consumes the shape below, the same same-shape-swap seam
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
