import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient, simulateDemoRun } from "./mockClient.ts";
import { GROUNDING_UIA, RUN_MODE_REPLAY, type BusEvent, type RunStepThumb } from "./types.ts";

test("publish delivers a versioned, sequenced envelope to matching subscribers", () => {
  const bus = createMockBusClient();
  const received: BusEvent[] = [];
  bus.subscribe("run", (e) => received.push(e));

  bus.publish("run.started", { run_id: "r1", goal: "test goal", mode: "dry" });

  assert.equal(received.length, 1);
  const [first] = received;
  assert.equal(first.v, 1);
  assert.equal(first.seq, 1);
  assert.equal(first.topic, "run.started");
  assert.ok(!Number.isNaN(Date.parse(first.ts)));
  // Narrow the payload union on the topic discriminant before reading a
  // field that is not common to every topic's payload.
  if (first.topic === "run.started") {
    assert.equal(first.payload.run_id, "r1");
  } else {
    assert.fail("expected a run.started event");
  }
});

test("seq increments across publishes regardless of topic", () => {
  const bus = createMockBusClient();
  const received: BusEvent[] = [];
  bus.subscribe("*", (e) => received.push(e));

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: "dry" });
  bus.publish("doctor.finding", {
    finding_id: "d1",
    severity: "info",
    what: "x",
    why: "y",
    action: "z",
  });

  assert.deepEqual(
    received.map((e) => e.seq),
    [1, 2],
  );
});

test("prefix subscription only matches its own namespace", () => {
  const bus = createMockBusClient();
  const runEvents: BusEvent[] = [];
  const doctorEvents: BusEvent[] = [];
  bus.subscribe("run", (e) => runEvents.push(e));
  bus.subscribe("doctor", (e) => doctorEvents.push(e));

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: "dry" });
  bus.publish("doctor.finding", {
    finding_id: "d1",
    severity: "info",
    what: "x",
    why: "y",
    action: "z",
  });

  assert.equal(runEvents.length, 1);
  assert.equal(doctorEvents.length, 1);
});

test("wildcard subscription receives every topic", () => {
  const bus = createMockBusClient();
  const all: BusEvent[] = [];
  bus.subscribe("*", (e) => all.push(e));

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: "dry" });
  bus.publish("doctor.finding", {
    finding_id: "d1",
    severity: "info",
    what: "x",
    why: "y",
    action: "z",
  });

  assert.equal(all.length, 2);
});

test("unsubscribe stops further delivery", () => {
  const bus = createMockBusClient();
  const received: BusEvent[] = [];
  const unsubscribe = bus.subscribe("run", (e) => received.push(e));

  bus.publish("run.started", { run_id: "r1", goal: "g", mode: "dry" });
  unsubscribe();
  bus.publish("run.started", { run_id: "r2", goal: "g2", mode: "dry" });

  assert.equal(received.length, 1);
});

test("simulateDemoRun starts immediately, streams steps, completes, and can be cancelled", async () => {
  const bus = createMockBusClient();
  const topics: string[] = [];
  bus.subscribe("run", (e) => topics.push(e.topic));

  const stop = simulateDemoRun(bus, { stepDelayMs: 2 });
  assert.equal(topics[0], "run.started");

  await new Promise((resolve) => setTimeout(resolve, 60));

  assert.ok(topics.includes("run.step.proposed"));
  assert.ok(topics.includes("run.step.executed"));
  assert.equal(topics[topics.length - 1], "run.completed");

  stop();
});

test("simulateDemoRun carries a custom goal and run id", () => {
  const bus = createMockBusClient();
  const received: BusEvent[] = [];
  bus.subscribe("run.started", (e) => received.push(e));

  const stop = simulateDemoRun(bus, { goal: "Custom goal text", runId: "custom-1", stepDelayMs: 2 });

  assert.equal(received.length, 1);
  const [first] = received;
  if (first.topic === "run.started") {
    assert.equal(first.payload.goal, "Custom goal text");
    assert.equal(first.payload.run_id, "custom-1");
  } else {
    assert.fail("expected a run.started event");
  }
  stop();
});

test("simulateDemoRun skips the proposed step outside teach mode", async () => {
  // Per contracts/bus_events.md, run.step.proposed only fires while teaching, before the checkpoint.
  const bus = createMockBusClient();
  const topics: string[] = [];
  bus.subscribe("run", (e) => topics.push(e.topic));

  const stop = simulateDemoRun(bus, { mode: RUN_MODE_REPLAY, stepDelayMs: 2 });
  await new Promise((resolve) => setTimeout(resolve, 60));

  assert.ok(!topics.includes("run.step.proposed"));
  assert.ok(topics.includes("run.step.executed"));
  assert.equal(topics[topics.length - 1], "run.completed");
  stop();
});

test("simulateDemoRun cancellation prevents further events", async () => {
  const bus = createMockBusClient();
  const topics: string[] = [];
  bus.subscribe("run", (e) => topics.push(e.topic));

  const stop = simulateDemoRun(bus, { stepDelayMs: 5 });
  stop();
  const countAfterStop = topics.length;

  await new Promise((resolve) => setTimeout(resolve, 40));

  assert.equal(topics.length, countAfterStop);
});

// --- evt-frame sidecar (contracts/ipc.md section 2d/7) ---

test("publish delivers the evt sidecar (the flight-recorder thumb) to subscribers, beside the envelope", () => {
  const bus = createMockBusClient();
  const seen: Array<{ topic: string; thumb: RunStepThumb | null | undefined }> = [];
  bus.subscribe("run", (e, sidecar) => seen.push({ topic: e.topic, thumb: sidecar?.thumb }));

  const thumb: RunStepThumb = { run_id: "r1", step_id: "s1", format: "png", w: 320, h: 200, redacted: true, data_b64: "aGk=" };
  bus.publish("run.step.executed", { run_id: "r1", step_id: "s1", outcome: "ok", ms: 5, grounding: GROUNDING_UIA }, { thumb });
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 1, wall_ms: 5 });

  assert.equal(seen.length, 2);
  assert.deepEqual(seen[0].thumb, thumb, "the step frame's thumbnail rides the subscription's sidecar argument");
  assert.equal(seen[1].thumb, undefined, "an event published with no sidecar carries none");
});

// --- run-control command echo (contracts/ipc.md section 5b) --- The mock
// stands in for the core: a command's observable effect is the run.* event the
// core would publish back, so the run viewer's round trip works on the mock.

test("command(stop) echoes run.halted with reason human", () => {
  const bus = createMockBusClient();
  const halted: unknown[] = [];
  bus.subscribe("run.halted", (e) => halted.push(e.payload));

  bus.command({ cmd: "stop", run_id: "r1" });

  assert.deepEqual(halted, [{ run_id: "r1", reason: "human" }]);
});

test("command(pause) and command(resume) echo the paired run events", () => {
  const bus = createMockBusClient();
  const topics: string[] = [];
  bus.subscribe("run", (e) => topics.push(e.topic));

  bus.command({ cmd: "pause", run_id: "r1" });
  bus.command({ cmd: "resume", run_id: "r1" });

  assert.deepEqual(topics, ["run.paused", "run.resumed"]);
});

test("command(redirect) echoes run.redirected then run.resumed (the correction is captured, then the run resumes on its own)", () => {
  const bus = createMockBusClient();
  const events: Array<{ topic: string; payload: unknown }> = [];
  bus.subscribe("run", (e) => events.push({ topic: e.topic, payload: e.payload }));

  bus.command({ cmd: "redirect", run_id: "r1", instruction: "use ctrl+s" });

  assert.deepEqual(events, [
    { topic: "run.redirected", payload: { run_id: "r1", instruction: "use ctrl+s" } },
    { topic: "run.resumed", payload: { run_id: "r1" } },
  ]);
});
