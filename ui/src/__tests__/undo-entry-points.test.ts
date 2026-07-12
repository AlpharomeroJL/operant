// Proves ui/src/undo is reachable both ways docs/specs/design.md section 3
// and this packet's own bar require: from a completed run's own detail (the
// flight recorder, once done) and from the toast a completed run raises.
// Exercises ui/src/bus, ui/src/runViewer, and ui/src/undo together the same
// way ui/src/main.ts wires them, the same non-jsdom, non-main.ts-importing
// style ui/src/__tests__/palette-run-viewer.test.ts already uses for "the
// same way main.ts wires them" claims: main.ts's own DOM glue (which id goes
// with which button) is intentionally left to manual/visual verification,
// same as every other screen's wiring in that file, but the underlying
// contract both call sites depend on -- a run must actually reach "done"
// before the entry point is reachable, and run.completed must actually
// carry the data a toast needs -- is exercised here for real.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { submitGoal } from "../palette/palette.ts";
import { createRunViewer } from "../runViewer/state.ts";
import { createUndoScreen } from "../undo/state.ts";

test("run detail: the entry point is unreachable while a run is live, and reachable once it is done, opening the same run's undo preview", async () => {
  const bus = createMockBusClient();
  const runViewer = createRunViewer(bus);
  const undoScreen = createUndoScreen(bus);

  const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
  assert.ok(stop);

  // Live: ui/src/main.ts's renderUndoEntry only ever shows the button once
  // runState is "done", so there is nothing yet for a click to open.
  assert.notEqual(runViewer.getSnapshot().runState, "done");

  await new Promise((resolve) => setTimeout(resolve, 40));
  const snap = runViewer.getSnapshot();
  assert.equal(snap.runState, "done");
  assert.ok(snap.runId);

  // What renderUndoEntry's click handler does: open the undo screen for this
  // exact run id.
  undoScreen.open(snap.runId!);
  const undoSnap = undoScreen.getSnapshot();
  assert.equal(undoSnap.phase, "preview");
  assert.equal(undoSnap.runId, snap.runId);
  assert.ok(undoSnap.hasItems);

  stop?.();
  runViewer.dispose();
  undoScreen.dispose();
});

test("toast: run.completed carries everything the toast needs (a run id to open, and enough to word its message), independent of the run-detail path", async () => {
  const bus = createMockBusClient();
  const runViewer = createRunViewer(bus);
  const undoScreen = createUndoScreen(bus);

  // What ui/src/main.ts's own run.completed subscription captures for the
  // toast, kept local to this test rather than importing main.ts (see this
  // file's header).
  let toastRunId: string | null = null;
  let toastSteps: number | null = null;
  bus.subscribe("run.completed", (event) => {
    if (event.topic !== "run.completed") return;
    toastRunId = event.payload.run_id;
    toastSteps = event.payload.steps;
  });

  const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
  await new Promise((resolve) => setTimeout(resolve, 40));

  assert.ok(toastRunId, "run.completed must have fired with a run id for the toast to open");
  assert.ok(typeof toastSteps === "number" && toastSteps > 0);
  assert.equal(toastRunId, runViewer.getSnapshot().runId, "the toast must open the very run that just completed");

  // What the toast's own action button does: open the undo screen for that run.
  undoScreen.open(toastRunId!);
  const undoSnap = undoScreen.getSnapshot();
  assert.equal(undoSnap.phase, "preview");
  assert.equal(undoSnap.runId, toastRunId);

  stop?.();
  runViewer.dispose();
  undoScreen.dispose();
});

test("both entry points open the identical preview for the same completed run", async () => {
  const bus = createMockBusClient();
  const runViewer = createRunViewer(bus);

  let toastRunId: string | null = null;
  bus.subscribe("run.completed", (event) => {
    if (event.topic !== "run.completed") toastRunId = null;
    else toastRunId = event.payload.run_id;
  });

  const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
  await new Promise((resolve) => setTimeout(resolve, 40));
  const runDetailRunId = runViewer.getSnapshot().runId;

  assert.equal(toastRunId, runDetailRunId, "the run-detail entry and the toast must agree on which run they undo");

  const screenA = createUndoScreen(bus);
  const screenB = createUndoScreen(bus);
  screenA.open(runDetailRunId!);
  screenB.open(toastRunId!);
  assert.deepEqual(screenA.getSnapshot().items, screenB.getSnapshot().items);

  stop?.();
  runViewer.dispose();
  screenA.dispose();
  screenB.dispose();
});
