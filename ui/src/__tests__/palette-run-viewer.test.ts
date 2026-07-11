// Scripted drive proving the command palette and run viewer work end to end
// against the mocked bus (contracts/bus_events.md), per docs/specs/ui.md:
// submit a goal in the palette, a run starts, steps stream in as
// plain-English rows, and Stop ends it. Exercises ui/src/palette and
// ui/src/runViewer together the same way main.ts wires them. No DOM (this
// project has no jsdom); DOM glue itself is intentionally untested, the same
// split used by every other module in ui/src.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY, GROUNDING_UIA } from "../bus/types.ts";
import { submitGoal } from "../palette/palette.ts";
import { createRunViewer } from "../runViewer/state.ts";

test("palette submit starts a run, steps stream as plain-English rows, then Stop halts it", async () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const seenStates: string[] = [];
  viewer.subscribe((snap) => seenStates.push(snap.runState));

  // 1. Submit: a plain goal typed into the palette.
  const stop = submitGoal(bus, "  Copy the invoice total into the spreadsheet  ", { stepDelayMs: 3 });
  assert.ok(stop, "a non-blank goal must start a run");

  // 2. Run starts: the run viewer reflects it immediately, model on (explore).
  let snap = viewer.getSnapshot();
  assert.equal(snap.runState, "running");
  assert.equal(snap.runStateLabel, "Running");
  assert.ok(snap.runId && snap.runId.length > 0);
  assert.equal(snap.modelOn, true);
  assert.equal(snap.modelIndicatorLabel, "Thinking live");

  // 3. Steps stream: every row is a plain-English sentence, never raw data.
  await new Promise((resolve) => setTimeout(resolve, 80));

  snap = viewer.getSnapshot();
  assert.deepEqual(
    snap.steps.map((s) => s.sentence),
    ['Click "Downloads"', 'Click "Invoice.pdf"', "Copy the selection", "Paste"],
  );
  for (const step of snap.steps) {
    assert.equal(typeof step.sentence, "string");
    assert.ok(step.sentence.length > 0, "every row must have a sentence");
    assert.ok(!/[{}]/.test(step.sentence), "a row must never show raw JSON");
    assert.ok(!/^s\d+$/.test(step.sentence), "a row must be a sentence, not a raw step id");
    assert.equal(step.status, "ok");
  }

  // 4. Stop: the run halts in human language, and stays halted.
  stop?.();
  viewer.stop();

  snap = viewer.getSnapshot();
  assert.equal(snap.runState, "halted");
  assert.equal(snap.runStateLabel, "Stopped, needs you");
  assert.equal(snap.canStop, false);

  assert.ok(seenStates.includes("running"));
  assert.ok(seenStates.includes("halted"));

  viewer.dispose();
});

test("a blank palette submission never starts a run", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);

  const stop = submitGoal(bus, "   ");

  assert.equal(stop, null);
  assert.equal(viewer.getSnapshot().runState, "idle");
});

test("the model indicator reflects mode: on while teaching, off for a saved-workflow run", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);

  const stopTeaching = submitGoal(bus, "Teach it something new", { stepDelayMs: 2 });
  assert.equal(viewer.getSnapshot().modelIndicatorLabel, "Thinking live");
  stopTeaching?.();
  viewer.stop();

  // A saved-workflow run never publishes run.step.proposed
  // (contracts/bus_events.md marks it as only published while teaching,
  // before the checkpoint), so a row from one still has to be a clean
  // plain-English sentence, never the raw step id, even without it.
  const runId = "saved-workflow-run-1";
  bus.publish("run.started", { run_id: runId, goal: "Run the saved workflow", mode: RUN_MODE_REPLAY });
  assert.equal(viewer.getSnapshot().modelIndicatorLabel, "Running from memory, no thinking needed");

  bus.publish("run.step.gated", { run_id: runId, step_id: "r1", gate_kind: "pre", result: "pass" });
  bus.publish("run.step.executed", { run_id: runId, step_id: "r1", outcome: "ok", ms: 50, grounding: GROUNDING_UIA });

  const snap = viewer.getSnapshot();
  assert.equal(snap.steps.length, 1);
  assert.equal(snap.steps[0].sentence, "Step 1");
  assert.equal(snap.steps[0].status, "ok");

  viewer.dispose();
});
