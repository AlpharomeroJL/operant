// Tests for the run viewer's state machine: derives run state, step rows,
// and the model indicator from run.* bus events (contracts/bus_events.md),
// and turns Stop/Pause/intervene into the right bus publishes. No DOM here;
// main.ts binds this to the page (see ui/src/__tests__ for the scripted
// palette-to-run-viewer drive).

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient, type BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY, GROUNDING_UIA, type RunMode } from "../bus/types.ts";
import { createRunViewer } from "./state.ts";

function startRun(bus: BusClient, runId = "r1", mode: RunMode = RUN_MODE_EXPLORE): void {
  bus.publish("run.started", { run_id: runId, goal: "test goal", mode });
}

test("starts idle, with no intervene field and no model reading yet", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const snap = viewer.getSnapshot();

  assert.equal(snap.runState, "idle");
  assert.equal(snap.runStateLabel, "Idle");
  assert.equal(snap.showIntervene, false);
  assert.equal(snap.modelOn, null);
  assert.equal(snap.canStop, false);
  assert.equal(snap.canPause, false);
});

test("run.started moves to running and sets the model indicator from mode", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);

  startRun(bus, "r1", RUN_MODE_EXPLORE);
  let snap = viewer.getSnapshot();
  assert.equal(snap.runState, "running");
  assert.equal(snap.runId, "r1");
  assert.equal(snap.modelOn, true);
  assert.equal(snap.modelIndicatorLabel, "Thinking live");
  assert.equal(snap.canStop, true);
  assert.equal(snap.canPause, true);

  startRun(bus, "r2", RUN_MODE_REPLAY);
  snap = viewer.getSnapshot();
  assert.equal(snap.modelOn, false);
  assert.equal(snap.modelIndicatorLabel, "Running from memory, no thinking needed");
});

test("a proposed step renders through the plain-English renderer", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus);

  bus.publish("run.step.proposed", {
    run_id: "r1",
    step: { v: 1, id: "s1", kind: "key", params: { combo: "ctrl+s" } },
  });

  const snap = viewer.getSnapshot();
  assert.equal(snap.steps.length, 1);
  assert.equal(snap.steps[0].sentence, "Save the file");
  assert.equal(snap.steps[0].status, "pending");
});

test("executed and failed update the matching row by step id, never duplicating it", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus);

  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });
  bus.publish("run.step.executed", { run_id: "r1", step_id: "s1", outcome: "ok", ms: 10, grounding: GROUNDING_UIA });

  let snap = viewer.getSnapshot();
  assert.equal(snap.steps.length, 1);
  assert.equal(snap.steps[0].status, "ok");
  assert.equal(snap.steps[0].sentence, "Wait for the screen to update");

  bus.publish("run.step.proposed", {
    run_id: "r1",
    step: { v: 1, id: "s2", kind: "scroll", params: { direction: "down" } },
  });
  bus.publish("run.step.failed", { run_id: "r1", step_id: "s2", error_id: "E1", message: "boom" });

  snap = viewer.getSnapshot();
  assert.equal(snap.steps.length, 2);
  assert.equal(snap.steps[1].status, "failed");
  assert.equal(snap.steps[1].sentence, "Scroll down");
});

test("a step that only ever arrives as executed (no proposed, as when running a saved workflow) still gets a plain sentence", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1", RUN_MODE_REPLAY);

  bus.publish("run.step.executed", {
    run_id: "r1",
    step_id: "only-id-1",
    outcome: "ok",
    ms: 5,
    grounding: GROUNDING_UIA,
  });

  const snap = viewer.getSnapshot();
  assert.equal(snap.steps.length, 1);
  assert.equal(snap.steps[0].sentence, "Step 1");
  assert.ok(!snap.steps[0].sentence.includes("only-id-1"), "the raw step id must never leak into the row");
});

test("stop publishes run.halted for the current run and moves state to halted", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const halted: unknown[] = [];
  bus.subscribe("run.halted", (e) => halted.push(e.payload));

  startRun(bus, "r1");
  viewer.stop();

  assert.equal(halted.length, 1);
  assert.deepEqual(halted[0], { run_id: "r1", reason: "human" });
  const snap = viewer.getSnapshot();
  assert.equal(snap.runState, "halted");
  assert.equal(snap.runStateLabel, "Stopped, needs you");
  assert.equal(snap.canStop, false);
});

test("stop before any run has started is a no-op", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const events: string[] = [];
  bus.subscribe("*", (e) => events.push(e.topic));

  viewer.stop();

  assert.equal(events.length, 0);
});

test("togglePause pauses a running run and shows the intervene field", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const paused: unknown[] = [];
  bus.subscribe("run.paused", (e) => paused.push(e.payload));

  startRun(bus, "r1");
  viewer.togglePause();

  assert.equal(paused.length, 1);
  assert.deepEqual(paused[0], { run_id: "r1", by: "human" });
  const snap = viewer.getSnapshot();
  assert.equal(snap.runState, "paused");
  assert.equal(snap.runStateLabel, "Paused, waiting for you");
  assert.equal(snap.showIntervene, true);
  assert.equal(snap.pauseButtonLabel, "Resume");
});

test("togglePause resumes a paused run and hides the intervene field", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);

  startRun(bus, "r1");
  viewer.togglePause();
  viewer.togglePause();

  const snap = viewer.getSnapshot();
  assert.equal(snap.runState, "running");
  assert.equal(snap.showIntervene, false);
  assert.equal(snap.pauseButtonLabel, "Pause");
});

test("intervene redirects and resumes a paused run, but is refused while running", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const redirected: unknown[] = [];
  bus.subscribe("run.redirected", (e) => redirected.push(e.payload));

  startRun(bus, "r1");
  assert.equal(viewer.intervene("try the other button"), false, "cannot intervene on a running, non-paused run");
  assert.equal(redirected.length, 0);

  viewer.togglePause();
  assert.equal(viewer.intervene("  try the other button  "), true);
  assert.deepEqual(redirected[0], { run_id: "r1", instruction: "try the other button" });

  const snap = viewer.getSnapshot();
  assert.equal(snap.runState, "running");
  assert.equal(snap.showIntervene, false);
});

test("intervene rejects a blank instruction and leaves the run paused", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  viewer.togglePause();

  assert.equal(viewer.intervene("   "), false);
  assert.equal(viewer.getSnapshot().runState, "paused");
});

test("run.completed moves to done, distinct from idle", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");

  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 0, wall_ms: 10 });

  const snap = viewer.getSnapshot();
  assert.equal(snap.runState, "done");
  assert.equal(snap.runStateLabel, "Done");
  assert.notEqual(snap.runStateLabel, "Idle");
});

test("events for a different run id are ignored once a run is under way", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");

  bus.publish("run.step.executed", {
    run_id: "some-other-run",
    step_id: "x",
    outcome: "ok",
    ms: 1,
    grounding: GROUNDING_UIA,
  });

  assert.equal(viewer.getSnapshot().steps.length, 0);
});

test("dispose stops listening to the bus", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  let notifications = 0;
  viewer.subscribe(() => notifications++);

  viewer.dispose();
  startRun(bus, "r1");

  assert.equal(notifications, 0);
  assert.equal(viewer.getSnapshot().runState, "idle");
});
