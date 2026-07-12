// Tests for the run viewer's state machine: derives run state, step rows,
// and the model indicator from run.* bus events (contracts/bus_events.md),
// and turns Stop/Pause/intervene into the right bus publishes. No DOM here;
// main.ts binds this to the page (see ui/src/__tests__ for the scripted
// palette-to-run-viewer drive).

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient, type BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY, GROUNDING_UIA, type RunControlCommand, type RunMode, type RunStepThumb } from "../bus/types.ts";
import { createRunViewer } from "./state.ts";

function startRun(bus: BusClient, runId = "r1", mode: RunMode = RUN_MODE_EXPLORE): void {
  bus.publish("run.started", { run_id: runId, goal: "test goal", mode });
}

/**
 * A bus that records every run-control command the viewer sends. With
 * `echo:false` it does NOT play the core back (no run.* echo), so a test can
 * prove the viewer sends a command and never authors a run.* event itself; with
 * `echo:true` it delegates to the real mock so the round trip advances state.
 */
function spyBus(echo = true): { bus: BusClient; commands: RunControlCommand[] } {
  const inner = createMockBusClient();
  const commands: RunControlCommand[] = [];
  const bus: BusClient = {
    ...inner,
    command: (c) => {
      commands.push(c);
      if (echo) inner.command(c);
    },
  };
  return { bus, commands };
}

const SAMPLE_THUMB: RunStepThumb = {
  run_id: "r1",
  step_id: "s1",
  format: "png",
  w: 320,
  h: 200,
  redacted: true,
  data_b64: "aGVsbG8=",
};

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

// --- Flight recorder (docs/specs/design.md section 3) ---

test("the mode chip follows the run mode: recording while teaching, quiet for a saved-workflow run", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  assert.equal(viewer.getSnapshot().runChip, null, "no chip before any run has started");

  startRun(bus, "r1", RUN_MODE_EXPLORE);
  assert.equal(viewer.getSnapshot().runChip, "rec");

  startRun(bus, "r2", RUN_MODE_REPLAY);
  assert.equal(viewer.getSnapshot().runChip, "exact");
});

test("the filmstrip auto-follows: with nothing scrubbed to, the active step is always the latest", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  assert.equal(viewer.getSnapshot().activeStepId, null, "no active step before any arrive");

  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });
  assert.equal(viewer.getSnapshot().activeStepId, "s1");

  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s2", kind: "wait" } });
  assert.equal(viewer.getSnapshot().activeStepId, "s2", "the strip follows the newest frame on its own");

  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s3", kind: "wait" } });
  assert.equal(viewer.getSnapshot().activeStepId, "s3");
});

test("select scrubs to a step and pins the highlight; selecting null hands control back to auto-follow", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s2", kind: "wait" } });

  viewer.select("s1");
  assert.equal(viewer.getSnapshot().selectedStepId, "s1");
  assert.equal(viewer.getSnapshot().activeStepId, "s1", "a scrubbed-to step pins the highlight");

  // A newer step arriving must not steal the pinned highlight.
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s3", kind: "wait" } });
  assert.equal(viewer.getSnapshot().activeStepId, "s1");

  viewer.select(null);
  assert.equal(viewer.getSnapshot().activeStepId, "s3", "clearing the scrub returns to the latest");
});

test("select ignores a step id that is not part of this run", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });

  viewer.select("not-a-real-step");
  assert.equal(viewer.getSnapshot().selectedStepId, null);
});

test("a new run resets any scrub selection back to auto-follow", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });
  viewer.select("s1");
  assert.equal(viewer.getSnapshot().selectedStepId, "s1");

  startRun(bus, "r2");
  assert.equal(viewer.getSnapshot().selectedStepId, null);
});

test("a failed safety check is recorded on its step; a passing one leaves no trace", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });

  // A passing check changes nothing on the row.
  bus.publish("run.step.gated", { run_id: "r1", step_id: "s1", gate_kind: "pre", result: "pass" });
  assert.equal(viewer.getSnapshot().steps[0].gate, undefined);

  // A failing one marks the step so the viewer can draw its inline card.
  bus.publish("run.step.gated", { run_id: "r1", step_id: "s1", gate_kind: "safety", result: "fail", expr: "balance < 1000" });
  const failed = viewer.getSnapshot().steps[0].gate;
  assert.ok(failed, "a failed check must be recorded on the step");
  assert.equal(failed?.kind, "safety");
});

test("executed records the step duration for the mono time on its row", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });
  bus.publish("run.step.executed", { run_id: "r1", step_id: "s1", outcome: "ok", ms: 128, grounding: GROUNDING_UIA });
  assert.equal(viewer.getSnapshot().steps[0].durationMs, 128);
});

// --- Run-control command inversion (contracts/ipc.md section 5b; ipc-bridge
// section 8b) --- Stop/Pause/intervene must SEND commands to the core, not
// publish core-owned run.* events themselves. The core (or the mock standing in
// for it) echoes the resulting run.* back, which the handlers above render.

test("stop sends the stop command and never authors run.halted itself", () => {
  const { bus, commands } = spyBus(false);
  const viewer = createRunViewer(bus);
  const runTopics: string[] = [];
  bus.subscribe("run", (e) => runTopics.push(e.topic));

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  runTopics.length = 0; // ignore the setup event; watch only what stop() causes

  viewer.stop();

  assert.deepEqual(commands, [{ cmd: "stop", run_id: "r1" }]);
  assert.deepEqual(runTopics, [], "the viewer must not publish run.* itself; the core echoes it back");
});

test("togglePause sends the pause command, then the resume command across a cycle", () => {
  const { bus, commands } = spyBus(); // echo so the state advances through the cycle
  const viewer = createRunViewer(bus);
  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });

  viewer.togglePause(); // running -> pause
  assert.equal(viewer.getSnapshot().runState, "paused");
  viewer.togglePause(); // paused -> resume
  assert.equal(viewer.getSnapshot().runState, "running");

  assert.deepEqual(commands, [
    { cmd: "pause", run_id: "r1" },
    { cmd: "resume", run_id: "r1" },
  ]);
});

test("intervene sends a single redirect command and never authors run.* itself", () => {
  const { bus, commands } = spyBus(false);
  const viewer = createRunViewer(bus);
  const runTopics: string[] = [];
  bus.subscribe("run", (e) => runTopics.push(e.topic));

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
  bus.publish("run.paused", { run_id: "r1", by: "human" }); // core echo puts the run in paused
  runTopics.length = 0;

  assert.equal(viewer.intervene("  use ctrl+s  "), true);
  assert.deepEqual(commands, [{ cmd: "redirect", run_id: "r1", instruction: "use ctrl+s" }]);
  assert.deepEqual(runTopics, [], "no run.redirected/run.resumed is authored by the viewer");
});

test("a redirect the core echoes back arrives as run.redirected then run.resumed, resuming the run", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const topics: string[] = [];
  bus.subscribe("run", (e) => topics.push(e.topic));

  startRun(bus, "r1");
  viewer.togglePause(); // -> run.paused echo
  topics.length = 0;

  assert.equal(viewer.intervene("use ctrl+s"), true);
  assert.deepEqual(topics, ["run.redirected", "run.resumed"], "redirect captures the correction and resumes on its own");
  assert.equal(viewer.getSnapshot().runState, "running");
});

// --- Flight-recorder thumbnails (contracts/ipc.md section 7) --- The redacted
// screenshot rides the evt frame as a sidecar beside the envelope, so it
// reaches the viewer through the subscription's second argument, never inside
// the bus payload.

test("a redacted thumbnail on the executed frame is stored on its step row", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  bus.publish("run.step.proposed", { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } });
  bus.publish(
    "run.step.executed",
    { run_id: "r1", step_id: "s1", outcome: "ok", ms: 12, grounding: GROUNDING_UIA },
    { thumb: SAMPLE_THUMB },
  );
  assert.deepEqual(viewer.getSnapshot().steps[0].thumb, SAMPLE_THUMB);
});

test("a thumbnail may already ride the proposed frame", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1");
  bus.publish(
    "run.step.proposed",
    { run_id: "r1", step: { v: 1, id: "s1", kind: "wait" } },
    { thumb: SAMPLE_THUMB },
  );
  assert.deepEqual(viewer.getSnapshot().steps[0].thumb, SAMPLE_THUMB);
});

test("a null thumbnail (headless/mock core) leaves the row without one, so the filmstrip draws a placeholder", () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  startRun(bus, "r1", RUN_MODE_REPLAY);
  bus.publish(
    "run.step.executed",
    { run_id: "r1", step_id: "s1", outcome: "ok", ms: 12, grounding: GROUNDING_UIA },
    { thumb: null },
  );
  assert.equal(viewer.getSnapshot().steps[0].thumb, undefined);
});
