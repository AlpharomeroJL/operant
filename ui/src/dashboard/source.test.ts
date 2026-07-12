// The IPC adapter (./source.ts createIpcDashboardSource): maps contracts/ipc.md's
// get_metrics / list_runs / get_run / list_triggers onto the DashboardSource
// port, and turns an unavailable or not_implemented command into the empty
// result rather than a fabricated one. Driven against a fake invoke so both
// paths are proven today, independent of a live core.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createIpcDashboardSource, type IpcInvoke } from "./source.ts";

type Handlers = Record<string, (args?: Record<string, unknown>) => Promise<unknown>>;

/** A fake Tauri invoke: dispatches on command name, rejecting an unhandled command the way the core would (contracts/ipc.md section 2c). */
function fakeInvoke(handlers: Handlers): IpcInvoke {
  return async (cmd, args) => {
    const handler = handlers[cmd];
    if (!handler) throw { code: "unknown_command", message: `no handler for ${cmd}`, retryable: false };
    return handler(args);
  };
}

test("get_metrics: maps {week, minutes_saved_total} rows to the weekly series, order preserved, extra fields ignored", async () => {
  let seenArgs: Record<string, unknown> | undefined;
  const source = createIpcDashboardSource(
    fakeInvoke({
      get_metrics: async (args) => {
        seenArgs = args;
        return [
          { week: "2 weeks ago", minutes_saved_total: 30, total_runs: 3 },
          { week: "last week", minutes_saved_total: 60 },
          { week: "this week", minutes_saved_total: 192 },
        ];
      },
    }),
  );

  const metrics = await source.getWeeklyMetrics(8);
  assert.deepEqual(seenArgs, { weeks: 8 });
  assert.deepEqual(metrics, [
    { week: "2 weeks ago", minutesSaved: 30 },
    { week: "last week", minutesSaved: 60 },
    { week: "this week", minutesSaved: 192 },
  ]);
});

test("get_metrics: an unavailable metrics command resolves to empty (honest empty hero), never throws", async () => {
  const source = createIpcDashboardSource(
    fakeInvoke({
      get_metrics: async () => {
        throw { code: "internal", message: "boom", retryable: true };
      },
    }),
  );
  assert.deepEqual(await source.getWeeklyMetrics(8), []);
});

test("list_runs + get_run: finished runs map to Recent runs (goal as title, status as outcome, step count, ended time); running runs are skipped", async () => {
  const source = createIpcDashboardSource(
    fakeInvoke({
      list_runs: async () => ["run_c", "run_running", "run_a"],
      get_run: async (args) => {
        switch (args?.run_id) {
          case "run_c":
            return { run: { id: "run_c", goal: "Copy the invoice total", status: "completed", started: 1000, ended: 5000 }, steps: [1, 2, 3, 4] };
          case "run_running":
            return { run: { id: "run_running", goal: "teach something", status: "running", started: 2000, ended: null }, steps: [] };
          case "run_a":
            return { run: { id: "run_a", goal: "Back up photos", status: "failed", started: 500, ended: 900 }, steps: [1] };
          default:
            throw { code: "not_found", message: "no such run", retryable: false };
        }
      },
    }),
  );

  const runs = await source.getRecentRuns(5);
  assert.deepEqual(runs, [
    { runId: "run_c", title: "Copy the invoice total", outcome: "ok", steps: 4, completedAtMs: 5000 },
    { runId: "run_a", title: "Back up photos", outcome: "failed", steps: 1, completedAtMs: 900 },
  ]);
});

test("list_runs + get_run: honors the limit, counting only finished runs", async () => {
  const source = createIpcDashboardSource(
    fakeInvoke({
      list_runs: async () => ["run_1", "run_2", "run_3"],
      get_run: async (args) => ({
        run: { id: args?.run_id, goal: `did ${String(args?.run_id)}`, status: "completed", started: 1, ended: 2 },
        steps: [1],
      }),
    }),
  );
  const runs = await source.getRecentRuns(1);
  assert.equal(runs.length, 1);
  assert.equal(runs[0].runId, "run_1");
});

test("list_runs: an unavailable command resolves to empty, never throws", async () => {
  const source = createIpcDashboardSource(
    fakeInvoke({
      list_runs: async () => {
        throw { code: "internal", message: "boom", retryable: true };
      },
    }),
  );
  assert.deepEqual(await source.getRecentRuns(5), []);
});

test("list_triggers: a not_implemented error resolves to empty Up next (no fake scheduled runs)", async () => {
  const source = createIpcDashboardSource(
    fakeInvoke({
      list_triggers: async () => {
        // contracts/ipc.md section 5e: the reserved-but-unwired command answer.
        throw { code: "not_implemented", message: "no persistent trigger store", retryable: false };
      },
    }),
  );
  assert.deepEqual(await source.getUpcomingRuns(), []);
});

test("list_triggers: an empty trigger list is empty Up next", async () => {
  const source = createIpcDashboardSource(fakeInvoke({ list_triggers: async () => [] }));
  assert.deepEqual(await source.getUpcomingRuns(), []);
});

test("list_triggers: enabled triggers map to upcoming rows; disabled ones are dropped", async () => {
  const source = createIpcDashboardSource(
    fakeInvoke({
      list_triggers: async () => [
        { trigger_id: "t1", kind: "cron", workflow_name: "weekly-report-email", spec: "Mondays at 9am", enabled: true },
        { trigger_id: "t2", kind: "cron", workflow_name: "backup-photos", spec: "nightly", enabled: false },
      ],
    }),
  );
  assert.deepEqual(await source.getUpcomingRuns(), [{ workflowName: "weekly-report-email", whenLabel: "Mondays at 9am" }]);
});
