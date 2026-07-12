import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY } from "../bus/types.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { createTray } from "./state.ts";

test("starts idle with no notifications, a zero-minute tooltip, and a closed, empty menu", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  const snap = tray.getSnapshot();

  assert.equal(snap.glyph, "idle");
  assert.equal(snap.glyphLabel, "Idle");
  assert.equal(snap.minutesSavedThisWeek, 0);
  assert.equal(snap.tooltip, "Saved about 0 minutes this week");
  assert.deepEqual(snap.notifications, []);
  assert.equal(snap.menuOpen, false);
  assert.deepEqual(snap.quickRuns, []);
  assert.equal(snap.canPauseAll, false);

  tray.dispose();
});

test("a scripted run cycles the glyph through every design.md state: idle, recording, replaying, kill", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  const seen: string[] = [];
  tray.subscribe((snap) => seen.push(snap.glyph));

  assert.equal(tray.getSnapshot().glyph, "idle");

  // Teach a new workflow: the amber "recording" glyph.
  bus.publish("run.started", { run_id: "r1", goal: "teach it", mode: RUN_MODE_EXPLORE });
  assert.equal(tray.getSnapshot().glyph, "recording");
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 4, wall_ms: 900 });
  assert.equal(tray.getSnapshot().glyph, "idle");

  // Run the saved workflow: the gray "replaying" glyph, no AI.
  bus.publish("run.started", { run_id: "r2", goal: "run it", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  assert.equal(tray.getSnapshot().glyph, "replaying");
  bus.publish("run.completed", { run_id: "r2", outcome: "ok", steps: 4, wall_ms: 400 });
  assert.equal(tray.getSnapshot().glyph, "idle");

  // A safety check fails mid-run: the red "kill" glyph, the same one the kill switch paints.
  bus.publish("run.started", { run_id: "r3", goal: "run it again", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.halted", { run_id: "r3", reason: "error" });
  assert.equal(tray.getSnapshot().glyph, "kill");

  assert.deepEqual(seen, ["recording", "idle", "replaying", "idle", "replaying", "kill"]);

  tray.dispose();
});

test("run.started while teaching turns the glyph recording; run.completed returns it to idle", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  assert.equal(tray.getSnapshot().glyph, "recording");
  assert.equal(tray.getSnapshot().glyphLabel, "Recording");
  assert.equal(tray.getSnapshot().canPauseAll, true);

  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });
  assert.equal(tray.getSnapshot().glyph, "idle");
  assert.equal(tray.getSnapshot().canPauseAll, false);

  tray.dispose();
});

test("run.started for a saved workflow turns the glyph replaying", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  assert.equal(tray.getSnapshot().glyph, "replaying");
  assert.equal(tray.getSnapshot().glyphLabel, "Replaying");

  tray.dispose();
});

test("run.started in dry mode also turns the glyph replaying: only teaching is the amber exception", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: "dry", workflow_name: "copy-invoice-total" });
  assert.equal(tray.getSnapshot().glyph, "replaying");

  tray.dispose();
});

test("run.halted turns the glyph kill and raises a notification asking for a human", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  bus.publish("run.halted", { run_id: "r1", reason: "killswitch" });

  const snap = tray.getSnapshot();
  assert.equal(snap.glyph, "kill");
  assert.equal(snap.glyphLabel, "Stopped, needs you");
  assert.equal(snap.canPauseAll, false, "a halted run is no longer there to pause");
  assert.equal(snap.notifications.length, 1);
  assert.equal(snap.notifications[0].title, "Operant stopped");

  tray.dispose();
});

test("killswitch.engaged turns the glyph kill and raises its own notification, distinct from a halted run's", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("killswitch.engaged", { at_ms: 1234 });

  const snap = tray.getSnapshot();
  assert.equal(snap.glyph, "kill");
  assert.equal(snap.canPauseAll, false, "the kill switch froze everything, nothing left to pause");
  assert.equal(snap.notifications.length, 1);
  assert.equal(snap.notifications[0].title, "Emergency stop engaged");
  assert.equal(snap.notifications[0].body, "Every run is frozen until you resume it by hand.");

  tray.dispose();
});

test("killswitch.released returns the glyph to idle", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("killswitch.engaged", { at_ms: 1 });
  assert.equal(tray.getSnapshot().glyph, "kill");

  bus.publish("killswitch.released", {});
  assert.equal(tray.getSnapshot().glyph, "idle");

  tray.dispose();
});

test("run.paused keeps the glyph at whatever it was, not idle or kill", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  bus.publish("run.paused", { run_id: "r1", by: "human" });

  assert.equal(tray.getSnapshot().glyph, "recording");
  tray.dispose();
});

test("run.resumed repaints the glyph the run actually started with (teaching -> recording, a saved workflow -> replaying), even though the event itself carries no mode", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  bus.publish("run.started", { run_id: "r1", goal: "teach", mode: RUN_MODE_EXPLORE });
  bus.publish("run.paused", { run_id: "r1", by: "human" });
  bus.publish("run.resumed", { run_id: "r1" });
  assert.equal(tray.getSnapshot().glyph, "recording");
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 10 });

  bus.publish("run.started", { run_id: "r2", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.paused", { run_id: "r2", by: "human" });
  bus.publish("run.resumed", { run_id: "r2" });
  assert.equal(tray.getSnapshot().glyph, "replaying");

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

  assert.deepEqual(seen, ["recording"]);
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

  assert.equal(trayA.getSnapshot().glyph, "kill");
  assert.equal(trayB.getSnapshot().glyph, "idle");
  assert.equal(trayB.getSnapshot().notifications.length, 0);

  trayA.dispose();
  trayB.dispose();
});

test("toggleMenu opens and closes the menu; closeMenu on an already-closed menu is a no-op", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  assert.equal(tray.getSnapshot().menuOpen, false);

  let notified = 0;
  tray.subscribe(() => notified++);

  tray.toggleMenu();
  assert.equal(tray.getSnapshot().menuOpen, true);
  assert.equal(notified, 1);

  tray.closeMenu();
  assert.equal(tray.getSnapshot().menuOpen, false);
  assert.equal(notified, 2);

  tray.closeMenu();
  assert.equal(notified, 2, "closing an already-closed menu must not notify again");

  tray.dispose();
});

test("quick runs rank saved workflows by frecency, highest first, capped at the top three", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  // A fourth workflow beyond the three seeded ones, so the cap actually has
  // something to drop (library/mockRegistry.ts's own upsert: an unknown
  // name seeds a bare placeholder whose description is just the name).
  registry.upsert("fourth-workflow", {});
  const tray = createTray(bus, { registry, now: () => 1_000_000 });

  function ran(name: string, times: number): void {
    for (let i = 0; i < times; i++) {
      bus.publish("run.started", { run_id: `${name}-${i}`, goal: "run", mode: RUN_MODE_REPLAY, workflow_name: name });
    }
  }

  // Same instant for every pick (now is fixed above), so ranking here comes
  // down to count alone: backup-photos (3) > weekly-report-email (2) >
  // copy-invoice-total (1), and fourth-workflow (also 1) is bumped off the
  // end by the top-three cap.
  ran("copy-invoice-total", 1);
  ran("weekly-report-email", 2);
  ran("backup-photos", 3);
  ran("fourth-workflow", 1);

  assert.deepEqual(tray.getSnapshot().quickRuns, [
    { name: "backup-photos", title: "Back up this month's photos" },
    { name: "weekly-report-email", title: "Email the weekly report" },
    { name: "copy-invoice-total", title: "Copy the invoice total into the spreadsheet" },
  ]);

  tray.dispose();
});

test("quick runs stay empty until a saved workflow has actually run, and a teach run never counts", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);

  assert.deepEqual(tray.getSnapshot().quickRuns, []);

  // No workflow_name (a fresh teach run): nothing to rank.
  bus.publish("run.started", { run_id: "r1", goal: "teach it", mode: RUN_MODE_EXPLORE });
  assert.deepEqual(tray.getSnapshot().quickRuns, []);

  bus.publish("run.started", { run_id: "r2", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  assert.equal(tray.getSnapshot().quickRuns.length, 1);
  assert.equal(tray.getSnapshot().quickRuns[0].name, "copy-invoice-total");

  tray.dispose();
});

test("pauseAll pauses the one tracked run; a no-op with nothing running", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus);
  const paused: unknown[] = [];
  bus.subscribe("run.paused", (event) => paused.push(event.payload));

  tray.pauseAll();
  assert.equal(paused.length, 0, "nothing to pause while idle");

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  assert.equal(tray.getSnapshot().canPauseAll, true);

  tray.pauseAll();
  assert.deepEqual(paused, [{ run_id: "r1", by: "human" }]);

  tray.dispose();
});

test("panic engages the kill switch; the tray's own bus subscription then turns the glyph kill and raises a notification", () => {
  // The default two-path client (createBusPanicClient) end to end: with no run
  // under way the cooperative stop is a no-op and only the kill's echoed
  // killswitch.engaged fires, so this still sees exactly one engagement.
  const bus = createMockBusClient();
  const tray = createTray(bus, { now: () => 5000 });
  const engaged: unknown[] = [];
  bus.subscribe("killswitch.engaged", (event) => engaged.push(event.payload));

  tray.panic();

  assert.deepEqual(engaged, [{ at_ms: 5000 }]);
  const snap = tray.getSnapshot();
  assert.equal(snap.glyph, "kill");
  assert.equal(snap.glyphLabel, "Stopped, needs you");
  assert.equal(snap.notifications.at(-1)?.title, "Emergency stop engaged");

  tray.dispose();
});

test("panic drives the two-path stop (contracts/ipc.md section 5b): BOTH stop and kill fire, the cooperative stop then the backstop", () => {
  const bus = createMockBusClient();
  const calls: string[] = [];
  // Inject a spy for the command seam so this proves the wiring, not the mock
  // core's echo: both independent stop paths must be invoked, kill last.
  const tray = createTray(bus, {
    panicClient: {
      stop: (runId?: string) => calls.push(runId ? `stop:${runId}` : "stop"),
      kill: () => calls.push("kill"),
    },
  });

  tray.panic();

  assert.deepEqual(calls, ["stop", "kill"]);
  tray.dispose();
});

test("panic hands the tracked run to the cooperative stop, then the kill backstops it", () => {
  const bus = createMockBusClient();
  const calls: string[] = [];
  const tray = createTray(bus, {
    panicClient: {
      stop: (runId?: string) => calls.push(runId ? `stop:${runId}` : "stop"),
      kill: () => calls.push("kill"),
    },
  });

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  tray.panic();

  assert.deepEqual(calls, ["stop:r1", "kill"], "the active run is closed cooperatively, then killed as the backstop");
  tray.dispose();
});
