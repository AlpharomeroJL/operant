import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY, RUN_MODE_EXPLORE } from "../bus/types.ts";
import { createDashboard, SPARKLINE_WIDTH, SPARKLINE_HEIGHT } from "./state.ts";
import type { DashboardSource } from "./source.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { commonStrings, dashboardStrings } from "../strings/default.ts";
import { WEEKLY_METRICS_FIXTURE } from "./mockMetrics.ts";
import { TRIGGER_KIND_CRON, type SchedulerCommands, type TriggerRecord } from "../scheduler/commands.ts";

// A scheduler surface whose list_triggers returns a fixed set of triggers, for
// exercising the (future) available path. upsert_trigger is not used by the
// dashboard, so it just mirrors the not-implemented default.
function schedulerWithTriggers(triggers: TriggerRecord[]): SchedulerCommands {
  return {
    listTriggers: async () => ({ ok: true, result: triggers }),
    upsertTrigger: async () => ({ ok: false, error: { code: "not_implemented", message: "x", retryable: false } }),
  };
}

/** Lets a fire-and-forget list_triggers probe settle before asserting on the snapshot. */
function flush(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

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

test("Up next: with no core trigger store, the default surface reports scheduling unavailable and never fabricates a run", () => {
  const bus = createMockBusClient();
  // The default scheduler answers list_triggers with not_implemented
  // (contracts/ipc.md section 5g). The synchronous snapshot is already honest:
  // unavailable, with zero rows, before the probe even resolves.
  const dashboard = createDashboard(bus);
  const snap = dashboard.getSnapshot();

  assert.equal(snap.upNextUnavailable, true, "scheduling is not wired, so Up next is unavailable");
  assert.equal(snap.upNext.length, 0, "no upcoming runs are ever invented");
  assert.equal(snap.upNextUnavailableLabel, dashboardStrings.upNextUnavailable);

  dashboard.dispose();
});

test("Up next: once list_triggers is wired, it fills in real rows from the returned triggers (no fabricated times)", async () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  const dashboard = createDashboard(bus, {
    registry,
    scheduler: schedulerWithTriggers([
      { trigger_id: "t1", kind: TRIGGER_KIND_CRON, workflow_name: "weekly-report-email", spec: "0 9 * * 1-5", enabled: true },
    ]),
  });

  await flush();
  const snap = dashboard.getSnapshot();

  assert.equal(snap.upNextUnavailable, false, "list_triggers succeeded, so scheduling is available");
  assert.equal(snap.upNext.length, 1);
  assert.equal(snap.upNext[0].workflowName, "weekly-report-email");
  // The title comes from the shared registry; the when-label is the trigger's
  // own spec verbatim, not a next-fire time the shell invented.
  assert.equal(snap.upNext[0].title, "Email the weekly report");
  assert.equal(snap.upNext[0].whenLabel, "0 9 * * 1-5");

  dashboard.dispose();
});

test("Up next: a returned trigger's title falls back to the raw workflow name when the registry has no match", async () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry([]);
  const dashboard = createDashboard(bus, {
    registry,
    scheduler: schedulerWithTriggers([
      { trigger_id: "t1", kind: TRIGGER_KIND_CRON, workflow_name: "not-in-registry", spec: "0 9 * * *", enabled: true },
    ]),
  });

  await flush();
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
  // With no core trigger store there are no upcoming runs, and no run has
  // happened, so the default dashboard is genuinely empty and shows the invite.
  const dashboard = createDashboard(bus);

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

test("not empty: a completed run shows the sections (with Up next honestly unavailable) instead of the invite", () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus, { now: () => 1_000_000 });

  bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });

  const snap = dashboard.getSnapshot();
  assert.equal(snap.empty, false, "a real run lifts the dashboard out of the empty invite");
  assert.equal(snap.recentRuns.length, 1);
  // Scheduling is still unavailable: the run does not fabricate an upcoming entry.
  assert.equal(snap.upNext.length, 0);
  assert.equal(snap.upNextUnavailable, true);
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

test("real source: hero, sparkline, and Recent runs come from the source; Up next is scheduler-sourced, not the source", async () => {
  const bus = createMockBusClient();
  const source = fakeSource({
    getWeeklyMetrics: async () => [
      { week: "a", minutesSaved: 30 },
      { week: "b", minutesSaved: 120 },
    ],
    getRecentRuns: async () => [{ runId: "run_1", title: "Copy the invoice total", outcome: "ok", steps: 4, completedAtMs: 1_000_000 }],
    // Even though this source would return an upcoming run, the reconciled
    // dashboard reads Up next from the scheduler surface, never the source, so
    // this value is deliberately ignored.
    getUpcomingRuns: async () => [{ workflowName: "weekly-report-email", whenLabel: "tomorrow at 9 am" }],
  });
  const dashboard = createDashboard(bus, { source, now: () => 1_000_000, registry: createMockRegistry([]) });
  await dashboard.refresh();

  const snap = dashboard.getSnapshot();
  assert.deepEqual(snap.sparklineValues, [30, 120]);
  assert.equal(snap.heroLine, "Operant saved you 2 hours this week");
  // Up next comes from the (default unavailable) scheduler, not the source.
  assert.equal(snap.upNext.length, 0);
  assert.equal(snap.upNextUnavailable, true);
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

test("dev fallback: with no source the dashboard uses the ./mockMetrics weekly fixture and refresh() is a no-op", async () => {
  const bus = createMockBusClient();
  const dashboard = createDashboard(bus);
  const before = dashboard.getSnapshot();
  assert.equal(before.heroLine, "Operant saved you 3.2 hours this week");

  await dashboard.refresh(); // no source: must not change the metrics
  const after = dashboard.getSnapshot();
  assert.equal(after.heroLine, before.heroLine);
  assert.deepEqual(after.sparklineValues, WEEKLY_METRICS_FIXTURE.map((m) => m.minutesSaved));
  // Up next is scheduler-sourced (no fixture up-next anymore); with no scheduler
  // wired it defaults to unavailable, so no rows show.
  assert.equal(after.upNext.length, 0);
  assert.equal(after.upNextUnavailable, true);
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
