// Same test shape as ui/src/tray/state.test.ts: a bus-driven state module,
// exercised through bus.publish rather than by calling an internal setter,
// since createToasts exposes no public "show" (see state.ts's own header).

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createToasts } from "./state.ts";
import { dashboardStrings } from "../strings/default.ts";

test("starts with no toast", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);

  assert.equal(toasts.getSnapshot().toast, null);

  toasts.dispose();
});

test("run.completed (ok) raises a verb-first toast inviting Undo this run, amber-eligible", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);

  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 14, wall_ms: 100 });

  const { toast } = toasts.getSnapshot();
  assert.ok(toast);
  assert.equal(toast!.message, "Run complete, 14 steps");
  assert.equal(toast!.message, dashboardStrings.outcomeOk(14));
  assert.equal(toast!.action?.label, "Undo this run");
  assert.equal(toast!.runId, "r1");

  toasts.dispose();
});

test("run.completed (failed) still invites Undo this run: a partial run can still have steps worth reversing", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);

  bus.publish("run.completed", { run_id: "r2", outcome: "failed", steps: 3, wall_ms: 40 });

  const { toast } = toasts.getSnapshot();
  assert.ok(toast);
  assert.equal(toast!.message, dashboardStrings.outcomeFailed);
  assert.equal(toast!.action?.label, "Undo this run");
  assert.equal(toast!.runId, "r2");

  toasts.dispose();
});

test("a second run.completed replaces the first toast: one at a time, same as design.md's single bottom-right line", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);

  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });
  const first = toasts.getSnapshot().toast;
  bus.publish("run.completed", { run_id: "r2", outcome: "ok", steps: 2, wall_ms: 10 });
  const second = toasts.getSnapshot().toast;

  assert.notEqual(first!.id, second!.id);
  assert.equal(second!.runId, "r2");

  toasts.dispose();
});

test("dismiss clears the toast and notifies subscribers", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });

  let notified = 0;
  toasts.subscribe(() => notified++);
  toasts.dismiss();

  assert.equal(toasts.getSnapshot().toast, null);
  assert.equal(notified, 1);

  toasts.dispose();
});

test("dismiss on an already-empty toast is a no-op (no notification fired)", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);
  let notified = 0;
  toasts.subscribe(() => notified++);

  toasts.dismiss();

  assert.equal(notified, 0);
  toasts.dispose();
});

test("subscribers see every toast change and can unsubscribe", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);
  const seen: Array<string | null> = [];
  const unsubscribe = toasts.subscribe((s) => seen.push(s.toast?.message ?? null));

  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });
  unsubscribe();
  toasts.dismiss();

  assert.deepEqual(seen, [dashboardStrings.outcomeOk(1)]);
  toasts.dispose();
});

test("dispose stops listening to the bus", () => {
  const bus = createMockBusClient();
  const toasts = createToasts(bus);

  toasts.dispose();
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });

  assert.equal(toasts.getSnapshot().toast, null);
});

test("independent toasts instances do not share state", () => {
  const busA = createMockBusClient();
  const busB = createMockBusClient();
  const toastsA = createToasts(busA);
  const toastsB = createToasts(busB);

  busA.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });

  assert.ok(toastsA.getSnapshot().toast);
  assert.equal(toastsB.getSnapshot().toast, null);

  toastsA.dispose();
  toastsB.dispose();
});
