import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY, RUN_MODE_EXPLORE } from "../bus/types.ts";
import { createDashboard, SPARKLINE_WIDTH, SPARKLINE_HEIGHT } from "./state.ts";
import type { DashboardSource } from "./source.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { commonStrings, dashboardStrings } from "../strings/default.ts";
import { dashboardCopyStrings } from "./strings.ts";
import { WEEKLY_METRICS_FIXTURE } from "./mockMetrics.ts";

/** A fake real data source: empty by default (the honest empty state), overridable per method. */
function fakeSource(over: Partial<DashboardSource> = {}): DashboardSource {
  return {
    getWeeklyMetrics: async () => [],
    getRecentRuns: async () => [],
    getUpcomingRuns: async () => [],
    ...over,
  };
}

test("hero line and sparkline render from the fixture metrics: design.md's own '3.2 hours this week' example", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus);
  const snap = dashboard.getSnapshot();

  assert.equal(snap.heroLine, "Operant saved you 3.2 hours this week");
  assert.deepEqual(
    snap.sparklineValues,
    WEEKLY_METRICS_FIXTURE.map((m) => m.minutesSaved),
  );
  assert.equal(snap.sparklinePoints.length, 8);
  assert.equal(snap.sparklineSummary, dashboardStrings.sparklineSummary("45, 60, 30, 90, 120, 80, 150, 192"));

  dashboard.dispose();
});

test("hero line formatting: rounds to one decimal, drops a trailing .0, and gets hour/hours right", () => {
  const bus = createMockBusClient();

  const zero = createDashboard(bus, { weeklyMetrics: [{ week: "this week", minutesSaved: 0 }] });
  assert.equal(zero.getSnapshot().heroLine, "Operant saved you 0 hours this week");
  zero.dispose();

  const oneHour = createDashboard(bus, { weeklyMetrics: [{ week: "this week", minutesSaved: 60 }] });
  assert.equal(oneHour.getSnapshot().heroLine, "Operant saved you 1 hour this week");
  oneHour.dispose();

  const halfHour = createDashboard(bus, { weeklyMetrics: [{ week: "this week", minutesSaved: 30 }] });
  assert.equal(halfHour.getSnapshot().heroLine, "Operant saved you 0.5 hours this week");
  halfHour.dispose();
});

test("sparkline points: higher minutes-saved plots higher on the chart (a smaller y), oldest to newest reads left to right", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, {
    weeklyMetrics: [
      { week: "2 weeks ago", minutesSaved: 0 },
      { week: "this week", minutesSaved: 100 },
    ],
  });
  const [first, second] = dashboard.getSnapshot().sparklinePoints;

  assert.ok(first.x < second.x, "oldest week must plot to the left of the newest");
  assert.ok(first.y > second.y, "the lower value must plot lower on the chart (a larger y)");
  for (const p of [first, second]) {
    assert.ok(p.x >= 0 && p.x <= SPARKLINE_WIDTH);
    assert.ok(p.y >= 0 && p.y <= SPARKLINE_HEIGHT);
  }

  dashboard.dispose();
});

test("sparkline points: flat data (every week the same) plots a level line instead of dividing by zero", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, {
    weeklyMetrics: [
      { week: "a", minutesSaved: 50 },
      { week: "b", minutesSaved: 50 },
      { week: "c", minutesSaved: 50 },
    ],
  });
  const points = dashboard.getSnapshot().sparklinePoints;
  assert.equal(points.length, 3);
  assert.ok(points.every((p) => Number.isFinite(p.y)));
  assert.equal(points[0].y, points[1].y);
  assert.equal(points[1].y, points[2].y);
  dashboard.dispose();
});

test("a single week of metrics renders one point without throwing", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { weeklyMetrics: [{ week: "this week", minutesSaved: 30 }] });
  assert.equal(dashboard.getSnapshot().sparklinePoints.length, 1);
  dashboard.dispose();
});

test("Up next: the default fixture's humane times, against a fixed clock", () => {
  const bus = createMockBusClient();
  // A Wednesday: 2026-06-10 08:00 local. Chosen arbitrarily; only day-count
  // and hour/minute arithmetic are under test, not which weekday "now" is.
  const now = () => new Date(2026, 5, 10, 8, 0, 0).getTime();
  const dashboard = createDashboard(bus, { now });
  const snap = dashboard.getSnapshot();

  assert.equal(snap.upNext.length, 2);
  assert.equal(snap.upNext[0].workflowName, "weekly-report-email");
  assert.equal(snap.upNext[0].whenLabel, "tomorrow at 9 am");

  const expectedTarget = new Date(2026, 5, 13, 20, 30);
  const expectedWeekday = dashboardCopyStrings.weekdayNames[expectedTarget.getDay()];
  assert.equal(snap.upNext[1].whenLabel, `${expectedWeekday} at 8:30 pm`);

  dashboard.dispose();
});

test("Up next: today, and an on-the-hour time with no minutes shown", () => {
  const bus = createMockBusClient();
  const now = () => new Date(2026, 5, 10, 8, 0, 0).getTime();
  const dashboard = createDashboard(bus, {
    now,
    upNext: [{ workflowName: "copy-invoice-total", daysFromNow: 0, hour: 15, minute: 0 }],
  });
  assert.equal(dashboard.getSnapshot().upNext[0].whenLabel, "today at 3 pm");
  dashboard.dispose();
});

test("Up next: midnight and noon both format as 12, not 0", () => {
  const bus = createMockBusClient();
  const now = () => new Date(2026, 5, 10, 8, 0, 0).getTime();
  const dashboard = createDashboard(bus, {
    now,
    upNext: [
      { workflowName: "copy-invoice-total", daysFromNow: 0, hour: 0, minute: 0 },
      { workflowName: "backup-photos", daysFromNow: 0, hour: 12, minute: 0 },
    ],
  });
  const [midnight, noon] = dashboard.getSnapshot().upNext;
  assert.equal(midnight.whenLabel, "today at 12 am");
  assert.equal(noon.whenLabel, "today at 12 pm");
  dashboard.dispose();
});

test("Up next: a scheduled workflow's title comes from the shared registry, falling back to the raw name", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry([]);
  const dashboard = createDashboard(bus, { registry, upNext: [{ workflowName: "not-in-registry", daysFromNow: 1, hour: 9, minute: 0 }] });
  assert.equal(dashboard.getSnapshot().upNext[0].title, "not-in-registry");
  dashboard.dispose();
});

test("Recent runs: a completed saved-workflow run appears with its outcome, status, and a visually-hidden status label", () => {
  const bus = createMockBusClient();
  let tick = 1_000_000;
  const dashboard = createDashboard(bus, { now: () => tick });

  bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 4, wall_ms: 400 });

  const snap = dashboard.getSnapshot();
  assert.equal(snap.recentRuns.length, 1);
  const [row] = snap.recentRuns;
  assert.equal(row.workflowName, "copy-invoice-total");
  assert.equal(row.title, "Copy the invoice total into the spreadsheet");
  assert.equal(row.outcomeLabel, "Run complete, 4 steps");
  assert.equal(row.whenLabel, "just now");
  assert.equal(row.status, "ok");
  assert.equal(row.statusLabel, "Completed");

  dashboard.dispose();
});

test("Recent runs: a failed run gets the failed outcome text and status", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { now: () => 1_000_000 });

  bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "backup-photos" });
  bus.publish("run.completed", { run_id: "r1", outcome: "failed", steps: 1, wall_ms: 50 });

  const [row] = dashboard.getSnapshot().recentRuns;
  assert.equal(row.outcomeLabel, "Run did not finish");
  assert.equal(row.status, "failed");
  assert.equal(row.statusLabel, "Did not finish");

  dashboard.dispose();
});

test("Recent runs: newest first, and capped at the configured limit", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { now: () => 1_000_000, recentRunsLimit: 2 });

  for (const name of ["copy-invoice-total", "weekly-report-email", "backup-photos"]) {
    bus.publish("run.started", { run_id: `r-${name}`, goal: "run", mode: RUN_MODE_REPLAY, workflow_name: name });
    bus.publish("run.completed", { run_id: `r-${name}`, outcome: "ok", steps: 1, wall_ms: 10 });
  }

  const snap = dashboard.getSnapshot();
  assert.equal(snap.recentRuns.length, 2);
  assert.equal(snap.recentRuns[0].workflowName, "backup-photos");
  assert.equal(snap.recentRuns[1].workflowName, "weekly-report-email");

  dashboard.dispose();
});

test("Recent runs: a run.started with no workflow_name (a teach run not yet saved as a workflow) is never tracked", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { now: () => 1_000_000 });

  bus.publish("run.started", { run_id: "e1", goal: "teach", mode: RUN_MODE_EXPLORE });
  bus.publish("run.completed", { run_id: "e1", outcome: "ok", steps: 4, wall_ms: 400 });

  assert.equal(dashboard.getSnapshot().recentRuns.length, 0);
  dashboard.dispose();
});

test("Recent runs: run.completed for an untracked run id is ignored", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { now: () => 1_000_000 });

  bus.publish("run.completed", { run_id: "mystery", outcome: "ok", steps: 1, wall_ms: 10 });

  assert.equal(dashboard.getSnapshot().recentRuns.length, 0);
  dashboard.dispose();
});

test("empty state: nothing scheduled and nothing has run yet shows the quiet invite instead of both lists", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { upNext: [] });

  const snap = dashboard.getSnapshot();
  assert.equal(snap.empty, true);
  assert.equal(snap.upNext.length, 0);
  assert.equal(snap.recentRuns.length, 0);
  assert.equal(snap.emptyLabel, dashboardStrings.emptyInvite);
  // H1 (docs/specs/design.md section 4: "Empty states invite one specific
  // action", section 3's Wizard finish screen: "a single amber 'Teach your
  // first workflow' button"). ui/src/dashboard/view.ts renders this as a
  // button alongside emptyLabel's sentence above.
  assert.equal(snap.emptyActionLabel, commonStrings.teachFirstWorkflow);

  dashboard.dispose();
});

test("not empty: Up next alone (no recent runs yet) is not the empty state", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus);
  assert.equal(dashboard.getSnapshot().recentRuns.length, 0);
  assert.equal(dashboard.getSnapshot().empty, false, "the default fixture always has Up next entries");
  dashboard.dispose();
});

test("subscribe notifies on a new recent run; dispose stops both the bus subscription and notifications", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { now: () => 1_000_000 });
  let notified = 0;
  dashboard.subscribe(() => notified++);

  bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });
  assert.ok(notified >= 1);

  dashboard.dispose();
  const before = notified;
  bus.publish("run.started", { run_id: "r2", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "backup-photos" });
  bus.publish("run.completed", { run_id: "r2", outcome: "ok", steps: 1, wall_ms: 10 });
  assert.equal(notified, before);
});

// --- B6 real data source (contracts/ipc.md get_metrics / list_runs / get_run / list_triggers) ---

test("real source: hero, sparkline, Up next, and Recent runs all come from the source, not the ./mockMetrics fixtures", async () => {
  const bus = createMockBusClient();
  const source = fakeSource({
    getWeeklyMetrics: async () => [
      { week: "a", minutesSaved: 30 },
      { week: "b", minutesSaved: 120 },
    ],
    getRecentRuns: async () => [{ runId: "run_1", title: "Copy the invoice total", outcome: "ok", steps: 4, completedAtMs: 1_000_000 }],
    getUpcomingRuns: async () => [{ workflowName: "weekly-report-email", whenLabel: "tomorrow at 9 am" }],
  });
  // An empty registry: Up next falls back to the raw workflow name, proving the source (not a fixture) drove the row.
  const dashboard = createDashboard(bus, { source, now: () => 1_000_000, registry: createMockRegistry([]) });
  await dashboard.refresh();

  const snap = dashboard.getSnapshot();
  assert.deepEqual(snap.sparklineValues, [30, 120]);
  assert.equal(snap.heroLine, "Operant saved you 2 hours this week");
  assert.equal(snap.upNext.length, 1);
  assert.equal(snap.upNext[0].title, "weekly-report-email");
  assert.equal(snap.upNext[0].whenLabel, "tomorrow at 9 am");
  assert.equal(snap.recentRuns.length, 1);
  assert.equal(snap.recentRuns[0].title, "Copy the invoice total");
  assert.equal(snap.recentRuns[0].outcomeLabel, "Run complete, 4 steps");
  assert.equal(snap.recentRuns[0].whenLabel, "just now");
  assert.equal(snap.empty, false);
  dashboard.dispose();
});

test("real source with nothing recorded: honest EMPTY hero and empty state, never the fixture's 3.2 hours", async () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { source: fakeSource(), now: () => 1_000_000 });
  await dashboard.refresh();

  const snap = dashboard.getSnapshot();
  assert.equal(snap.heroLine, dashboardStrings.heroEmpty);
  assert.notEqual(snap.heroLine, "Operant saved you 3.2 hours this week");
  assert.deepEqual(snap.sparklineValues, []);
  assert.equal(snap.sparklinePoints.length, 0);
  assert.equal(snap.sparklineSummary, dashboardStrings.sparklineEmpty);
  assert.equal(snap.upNext.length, 0);
  assert.equal(snap.recentRuns.length, 0);
  assert.equal(snap.empty, true);
  dashboard.dispose();
});

test("real source: an empty Up next (list_triggers not_implemented) shows the 'nothing scheduled' state, with Recent runs still present", async () => {
  const bus = createMockBusClient();
  const source = fakeSource({
    getWeeklyMetrics: async () => [{ week: "a", minutesSaved: 60 }],
    getRecentRuns: async () => [{ runId: "r1", title: "Back up photos", outcome: "ok", steps: 2, completedAtMs: 1_000_000 }],
    getUpcomingRuns: async () => [], // the adapter already turned not_implemented into []
  });
  const dashboard = createDashboard(bus, { source, now: () => 1_000_000 });
  await dashboard.refresh();

  const snap = dashboard.getSnapshot();
  assert.equal(snap.upNext.length, 0);
  assert.equal(snap.upNextEmptyLabel, "Nothing scheduled yet.");
  assert.equal(snap.recentRuns.length, 1);
  // Recent runs present, so this is not the whole-dashboard first-run invite.
  assert.equal(snap.empty, false);
  dashboard.dispose();
});

test("real source: a metrics-only dashboard (runs recorded, nothing scheduled or recent) still shows the real hero, never fixtures", async () => {
  const bus = createMockBusClient();
  const source = fakeSource({ getWeeklyMetrics: async () => [{ week: "this week", minutesSaved: 90 }] });
  const dashboard = createDashboard(bus, { source, now: () => 1_000_000 });
  await dashboard.refresh();

  const snap = dashboard.getSnapshot();
  assert.equal(snap.heroLine, "Operant saved you 1.5 hours this week");
  assert.deepEqual(snap.sparklineValues, [90]);
  dashboard.dispose();
});

test("dev fallback: with no source the dashboard uses ./mockMetrics fixtures and refresh() is a no-op", async () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus);
  const before = dashboard.getSnapshot();
  assert.equal(before.heroLine, "Operant saved you 3.2 hours this week");

  await dashboard.refresh(); // no source: must not change anything
  const after = dashboard.getSnapshot();
  assert.equal(after.heroLine, before.heroLine);
  assert.deepEqual(after.sparklineValues, WEEKLY_METRICS_FIXTURE.map((m) => m.minutesSaved));
  assert.equal(after.upNext.length, 2); // the fixture Up next
  dashboard.dispose();
});

test("real source: a run completing live on the bus prepends to the seeded Recent runs, deduped by run id", async () => {
  const bus = createMockBusClient();
  const source = fakeSource({
    getRecentRuns: async () => [{ runId: "seed1", title: "Seeded run", outcome: "ok", steps: 4, completedAtMs: 900_000 }],
  });
  const registry = createMockRegistry(); // default seed so titleFor resolves the live run's name
  const dashboard = createDashboard(bus, { source, registry, now: () => 1_000_000 });
  await dashboard.refresh();
  assert.equal(dashboard.getSnapshot().recentRuns.length, 1);

  bus.publish("run.started", { run_id: "live1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "backup-photos" });
  bus.publish("run.completed", { run_id: "live1", outcome: "ok", steps: 2, wall_ms: 100 });
  const afterLive = dashboard.getSnapshot();
  assert.equal(afterLive.recentRuns.length, 2);
  assert.equal(afterLive.recentRuns[0].workflowName, "backup-photos"); // newest first

  // A live completion for the already-seeded run must replace it, not duplicate it.
  bus.publish("run.started", { run_id: "seed1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "seed1", outcome: "ok", steps: 4, wall_ms: 100 });
  const deduped = dashboard.getSnapshot();
  assert.equal(deduped.recentRuns.length, 2, "seed1 is replaced in place, not added a second time");
  assert.equal(deduped.recentRuns.filter((r) => r.title === "Seeded run").length, 0, "the seeded row is superseded by its live completion");
  dashboard.dispose();
});

test("refresh: a newer refresh supersedes an older one still in flight (latest query wins)", async () => {
  const bus = createMockBusClient();
  let release: () => void = () => {};
  const slowFirst = new Promise<void>((resolve) => {
    release = resolve;
  });
  let call = 0;
  const source = fakeSource({
    getWeeklyMetrics: async () => {
      call += 1;
      if (call === 1) {
        await slowFirst; // hold the first load open until the second has resolved
        return [{ week: "stale", minutesSaved: 999 }];
      }
      return [{ week: "fresh", minutesSaved: 60 }];
    },
  });
  const dashboard = createDashboard(bus, { source, now: () => 1_000_000 });

  const first = dashboard.refresh(); // token 1, blocked
  await dashboard.refresh(); // token 2, resolves now
  assert.deepEqual(dashboard.getSnapshot().sparklineValues, [60]);

  release();
  await first; // token 1 finishes but is stale, so it must not overwrite token 2
  assert.deepEqual(dashboard.getSnapshot().sparklineValues, [60]);
  dashboard.dispose();
});
