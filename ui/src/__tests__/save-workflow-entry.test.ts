// @advanced
// Exempt from scripts/microcopy_lint.mjs (same reason ui/src/bus/realClient.test.ts
// is): a test file, not shipped UI copy, whose assertions name wire-protocol
// vocabulary from contracts/ipc.md ("compile", "replay", "trajectory", ...).
// Proves the shell-level "Save as workflow" entry (ui/src/main.ts's
// renderSaveWorkflowEntry) is reachable exactly when it should be and produces
// a real library workflow, exercising ui/src/teach, ui/src/runViewer, and
// ui/src/library together the same way main.ts wires them. Same non-jsdom,
// non-main.ts-importing style ui/src/__tests__/undo-entry-points.test.ts uses:
// main.ts's own DOM glue (which id goes with which button) is left to
// manual/visual verification, but the underlying contract the entry depends on
// -- a teach run must reach "done" as the recording (model-on) kind before the
// button is offered, compiling it must yield a library card, and it must never
// be offered twice or for a saved-workflow replay -- is exercised here for real.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY } from "../bus/types.ts";
import { createRunViewer, type RunViewerSnapshot } from "../runViewer/state.ts";
import { createLibrary } from "../library/state.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { createMockTeachClient, workflowNameFromGoal } from "../teach/client.ts";

// The exact predicate ui/src/main.ts's renderSaveWorkflowEntry gates the button
// on, minus wizardDismissed (first-run-only shell state): a completed teach run
// (runChip "rec" is the teach/model-on discriminant) not already compiled.
function entryShows(snap: RunViewerSnapshot, compiledRunIds: Set<string>): boolean {
  return snap.runState === "done" && snap.runChip === "rec" && snap.runId !== null && !compiledRunIds.has(snap.runId);
}

test("the shell Save as workflow entry: hidden until a teach run is done, then compiles it into a library card and hides again", async () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const registry = createMockRegistry([]);
  const library = createLibrary(bus, { registry });
  const teachClient = createMockTeachClient(bus);

  // What main.ts's workflow.compiled subscription tracks, kept local here.
  const compiledRunIds = new Set<string>();
  bus.subscribe("workflow.compiled", (e) => {
    if (e.topic === "workflow.compiled") compiledRunIds.add(e.payload.source_run_id);
  });

  // A teach run started from the running app, exactly as the palette submit
  // does: a goal plus the foreground window.
  const goal = "Copy the invoice total into the spreadsheet";
  const run = teachClient.startExplore({ goal, windowProcess: "chrome.exe", stepDelayMs: 3 });

  // Live: the entry is not offered while the run is still under way.
  assert.equal(entryShows(viewer.getSnapshot(), compiledRunIds), false);

  await new Promise((r) => setTimeout(r, 60));
  let snap = viewer.getSnapshot();
  assert.equal(snap.runState, "done");
  assert.equal(snap.runChip, "rec", "a teach run is the recording/model-on kind");
  assert.equal(entryShows(snap, compiledRunIds), true, "a completed teach run offers Save as workflow");

  // What the button's click does: compile_run for this run, named from its goal.
  const name = workflowNameFromGoal(goal);
  teachClient.compileRun(run.runId, { name });

  // The compiled workflow is now a card in the library (the flow's last leg).
  assert.ok(
    library.getSnapshot().cards.some((c) => c.name === name),
    "compiling the teach run must make it appear in the library",
  );
  // And the entry hides again: this run is compiled, never offered twice.
  snap = viewer.getSnapshot();
  assert.equal(entryShows(snap, compiledRunIds), false, "a compiled run is never offered for saving again");

  viewer.dispose();
  library.dispose();
});

test("the shell Save as workflow entry never shows for a saved-workflow replay (nothing to compile)", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const compiledRunIds = new Set<string>();

  // A saved-workflow replay: model off, so the flight recorder shows the quiet
  // "exact" chip, and there is no explored trajectory to compile.
  bus.publish("run.started", { run_id: "replay-1", goal: "Run the saved workflow", mode: RUN_MODE_REPLAY });
  bus.publish("run.completed", { run_id: "replay-1", outcome: "ok", steps: 2, wall_ms: 100 });

  const snap = viewer.getSnapshot();
  assert.equal(snap.runState, "done");
  assert.equal(snap.runChip, "exact");
  assert.equal(entryShows(snap, compiledRunIds), false, "a replay has no trajectory to compile");

  viewer.dispose();
});
