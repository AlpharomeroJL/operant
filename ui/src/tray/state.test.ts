import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE } from "../bus/types.ts";
import { createTray } from "./state.ts";

test("starts idle with no notifications and a zero-minute tooltip", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  const snap = tray.getSnapshot();

  assert.equal(snap.glyph, "idle");
  assert.equal(snap.glyphLabel, "Idle");
  assert.equal(snap.minutesSavedThisWeek, 0);
  assert.equal(snap.tooltip, "Saved about 0 minutes this week");
  assert.deepEqual(snap.notifications, []);

  tray.dispose();
});

test("run.started turns the glyph running; run.completed returns it to idle", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  assert.equal(tray.getSnapshot().glyph, "running");
  assert.equal(tray.getSnapshot().glyphLabel, "Running");

  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });
  assert.equal(tray.getSnapshot().glyph, "idle");

  tray.dispose();
});

test("run.halted turns the glyph halted-red and raises a notification asking for a human", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  bus.publish("run.halted", { run_id: "r1", reason: "killswitch" });

  const snap = tray.getSnapshot();
  assert.equal(snap.glyph, "halted-red");
  assert.equal(snap.glyphLabel, "Stopped, needs you");
  assert.equal(snap.notifications.length, 1);
  assert.equal(snap.notifications[0].title, "Operant stopped");

  tray.dispose();
});

test("run.paused keeps the glyph running rather than idle or halted", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  bus.publish("run.paused", { run_id: "r1", by: "human" });

  assert.equal(tray.getSnapshot().glyph, "running");
  tray.dispose();
});

test("metrics.week.rolled updates the saved-time tooltip and raises a weekly digest notification", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("metrics.week.rolled", { week: "2026-W28", minutes_saved_total: 192 });

  const snap = tray.getSnapshot();
  assert.equal(snap.minutesSavedThisWeek, 192);
  assert.equal(snap.tooltip, "Saved about 192 minutes this week");
  assert.equal(snap.notifications.length, 1);
  assert.equal(snap.notifications[0].title, "Your weekly time saved");
  assert.equal(snap.notifications[0].body, "Saved about 192 minutes this week");

  tray.dispose();
});

test("metrics.week.rolled's notification carries its own minutesSaved figure (F11: ui/src/tray/view.ts's restyled digest stat reads this, not the live tooltip)", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("metrics.week.rolled", { week: "2026-W28", minutes_saved_total: 192 });
  const [digest] = tray.getSnapshot().notifications;
  assert.equal(digest.minutesSaved, 192);

  // A second, later week must not rewrite the first notification's own
  // figure: each undismissed digest keeps the number it was raised with.
  bus.publish("metrics.week.rolled", { week: "2026-W29", minutes_saved_total: 5 });
  const [first, second] = tray.getSnapshot().notifications;
  assert.equal(first.minutesSaved, 192);
  assert.equal(second.minutesSaved, 5);

  tray.dispose();
});

test("run.halted's notification leaves minutesSaved undefined (not the digest kind)", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  bus.publish("run.halted", { run_id: "r1", reason: "killswitch" });

  const [halted] = tray.getSnapshot().notifications;
  assert.equal(halted.minutesSaved, undefined);

  tray.dispose();
});

test("dismissNotification removes just that one notification and notifies subscribers", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  bus.publish("run.halted", { run_id: "r1", reason: "human" });
  bus.publish("metrics.week.rolled", { week: "2026-W28", minutes_saved_total: 5 });

  const [first, second] = tray.getSnapshot().notifications;
  assert.equal(tray.getSnapshot().notifications.length, 2);

  let notified = 0;
  tray.subscribe(() => notified++);
  tray.dismissNotification(first.id);

  const remaining = tray.getSnapshot().notifications;
  assert.equal(remaining.length, 1);
  assert.equal(remaining[0].id, second.id);
  assert.equal(notified, 1);

  tray.dispose();
});

test("dismissNotification on an unknown id is a no-op (no notification fired)", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  let notified = 0;
  tray.subscribe(() => notified++);

  tray.dismissNotification("does-not-exist");

  assert.equal(notified, 0);
  tray.dispose();
});

test("subscribers see every glyph change and can unsubscribe", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  const seen: string[] = [];
  const unsubscribe = tray.subscribe((s) => seen.push(s.glyph));

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  unsubscribe();
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 1 });

  assert.deepEqual(seen, ["running"]);
  tray.dispose();
});

test("dispose stops listening to the bus", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  let notified = 0;
  tray.subscribe(() => notified++);

  tray.dispose();
  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });

  assert.equal(notified, 0);
  assert.equal(tray.getSnapshot().glyph, "idle");
});

test("independent tray instances do not share notifications or glyph state", () => {
  const busA = createMockBusClient();
  const busB = createMockBusClient();
  const trayA = createTray(busA);
  const trayB = createTray(busB);

  busA.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  busA.publish("run.halted", { run_id: "r1", reason: "human" });

  assert.equal(trayA.getSnapshot().glyph, "halted-red");
  assert.equal(trayB.getSnapshot().glyph, "idle");
  assert.equal(trayB.getSnapshot().notifications.length, 0);

  trayA.dispose();
  trayB.dispose();
});
