// The teach client (ui/src/teach/client.ts): the shell-side seam for
// start_explore and compile_run (contracts/ipc.md 5b, 5c). These prove the
// mock build's two commands do what the present-tense teach flow needs, driven
// against the same modules main.ts wires them to (ui/src/runViewer,
// ui/src/library), the same no-DOM, module-level style the rest of ui/src
// uses. The wizard's own wiring of this seam is covered in
// ui/src/wizard/state.test.ts.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import type { ActionIR, BusEvent } from "../bus/types.ts";
import { createRunViewer } from "../runViewer/state.ts";
import { createLibrary } from "../library/state.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { createMockTeachClient, workflowNameFromGoal, PLACEHOLDER_FOREGROUND_WINDOW } from "./client.ts";

const SCRIPT: ReadonlyArray<Pick<ActionIR, "kind" | "target" | "params">> = [
  { kind: "click", target: { selectors: [{ kind: "name_role_path", path: [{ role: "button", name: "New email" }] }] } },
  { kind: "type", params: { text: "Hello" } },
];

test("startExplore streams a watchable teach run: running -> plain-English steps -> done, model on", async () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const client = createMockTeachClient(bus);

  const run = client.startExplore({ goal: "Send a quick note", windowProcess: "outlook.exe", script: SCRIPT, stepDelayMs: 2 });
  assert.ok(run.runId.length > 0);

  // run.started is synchronous, so the flight recorder shows the run at once,
  // model on because teaching is an explore run.
  let snap = viewer.getSnapshot();
  assert.equal(snap.runState, "running");
  assert.equal(snap.modelOn, true);

  await new Promise((r) => setTimeout(r, 40));
  snap = viewer.getSnapshot();
  assert.equal(snap.runState, "done");
  assert.equal(snap.steps.length, SCRIPT.length);
  for (const step of snap.steps) {
    assert.ok(step.sentence.length > 0, "every row must have a sentence");
    assert.ok(!/[{}]/.test(step.sentence), "a row must never show raw JSON");
    assert.equal(step.status, "ok");
  }

  viewer.dispose();
});

test("startExplore with only a goal streams the default trajectory as plain sentences, never raw ids", async () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const client = createMockTeachClient(bus);

  client.startExplore({ goal: "Do the thing", windowProcess: PLACEHOLDER_FOREGROUND_WINDOW, stepDelayMs: 2 });
  await new Promise((r) => setTimeout(r, 40));

  const snap = viewer.getSnapshot();
  assert.equal(snap.runState, "done");
  assert.ok(snap.steps.length > 0, "a goal with no script still streams something to watch");
  for (const step of snap.steps) {
    assert.ok(!/[{}]/.test(step.sentence), "a row must never show raw JSON");
    assert.ok(!/^s\d+$/.test(step.sentence), "a row must be a sentence, not a raw step id");
  }

  viewer.dispose();
});

test("stop cancels steps not yet streamed, so the run never completes", async () => {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const client = createMockTeachClient(bus);

  const run = client.startExplore({ goal: "g", windowProcess: "w", script: SCRIPT, stepDelayMs: 50 });
  run.stop();
  await new Promise((r) => setTimeout(r, 130));

  const snap = viewer.getSnapshot();
  assert.notEqual(snap.runState, "done", "a stopped run must not keep streaming to completion");
  assert.equal(snap.steps.length, 0, "no step scheduled after stop may still arrive");

  viewer.dispose();
});

test("two explore runs get distinct ids even started in the same tick", () => {
  const bus = createMockBusClient();
  const client = createMockTeachClient(bus);
  const a = client.startExplore({ goal: "g", windowProcess: "w", stepDelayMs: 1000 });
  const b = client.startExplore({ goal: "g", windowProcess: "w", stepDelayMs: 1000 });
  assert.notEqual(a.runId, b.runId);
  a.stop();
  b.stop();
});

test("compileRun echoes workflow.compiled for the run, and the library picks it up as a card", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry([]);
  const library = createLibrary(bus, { registry });
  const compiled: BusEvent[] = [];
  bus.subscribe("workflow.compiled", (e) => compiled.push(e));

  const client = createMockTeachClient(bus);
  const result = client.compileRun("run-123", { name: "send-a-quick-note" });

  assert.equal(result.name, "send-a-quick-note");
  assert.equal(result.sourceRunId, "run-123");

  assert.equal(compiled.length, 1);
  const payload = compiled[0].payload as { name: string; source_run_id: string };
  assert.equal(payload.name, "send-a-quick-note");
  assert.equal(payload.source_run_id, "run-123");

  // The library upserts on workflow.compiled (ui/src/library/state.ts), so a
  // card exists for the just-compiled workflow with no further wiring: this is
  // the "compiled workflow appears in the library" end of the teach flow.
  assert.ok(
    library.getSnapshot().cards.some((c) => c.name === "send-a-quick-note"),
    "the compiled workflow must appear as a library card",
  );

  library.dispose();
});

test("workflowNameFromGoal slugifies, caps length, and falls back for an unusable goal", () => {
  assert.equal(
    workflowNameFromGoal("Copy the invoice total into the spreadsheet"),
    "copy-the-invoice-total-into-the-spreadsheet",
  );
  assert.equal(workflowNameFromGoal("  Send!!!  Report  "), "send-report");
  assert.equal(workflowNameFromGoal("!!!"), "taught-workflow");

  const long = workflowNameFromGoal("word ".repeat(40));
  assert.ok(long.length <= 48, "a long goal must not become an unwieldy id");
  assert.ok(!long.endsWith("-"), "a capped name must not end on a dangling hyphen");
});
