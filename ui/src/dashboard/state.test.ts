import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY, RUN_MODE_EXPLORE } from "../bus/types.ts";
import { createDashboard, SPARKLINE_WIDTH, SPARKLINE_HEIGHT } from "./state.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { dashboardStrings } from "../strings/default.ts";
import { dashboardCopyStrings } from "./strings.ts";
import { WEEKLY_METRICS_FIXTURE } from "./mockMetrics.ts";

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
