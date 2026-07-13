// @advanced
// Marked @advanced (exempt from scripts/microcopy_lint.mjs) for the same reason
// ui/src/bus/types.ts and ui/src/bus/realClient.ts are: this test asserts
// against wire vocabulary (topic names, command names, the recorded fixture's
// "explore"/"replay"/"uia" values), never user-facing UI copy.
//
// Proves createRealClient (ui/src/bus/realClient.ts) speaks the frozen IPC
// contract (contracts/ipc.md) end to end on the TypeScript side, with no live
// webview: the recorded fixture session's real evt frames are fed through a
// fake `listen` and must dispatch to subscribers by the exact same prefix rule
// the mock uses (exact / namespace-prefix / "*"), and UI publishes must route
// to invoke("core_call", { cmd, args }) for command topics while no-opping the
// core-owned event topics the core echoes back.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  createRealClient,
  BUS_EVENT_CHANNEL,
  THUMB_EVENT_CHANNEL,
  CORE_CALL_COMMAND,
  type TauriBridge,
} from "./realClient.ts";
import type { BusEnvelope, BusEvent, EvtSidecar } from "./types.ts";

// The recorded explore -> compile -> replay -> undo session, framed exactly per
// contracts/ipc.md. This file lives at ui/src/bus/; the repo root is three up.
const here = dirname(fileURLToPath(import.meta.url));
const fixturePath = join(here, "..", "..", "..", "contracts", "fixtures", "ipc", "session-explore-compile-replay-undo.jsonl");

interface Frame {
  t: string;
  env?: BusEnvelope;
  thumb?: unknown;
}

/** Every bus Envelope carried by an `evt` frame in the recorded session. */
function fixtureEnvelopes(): BusEnvelope[] {
  const lines = readFileSync(fixturePath, "utf8").split(/\r?\n/).filter(Boolean);
  const envs: BusEnvelope[] = [];
  for (const line of lines) {
    const frame = JSON.parse(line) as Frame;
    if (frame.t === "evt" && frame.env) envs.push(frame.env);
  }
  return envs;
}

interface FakeBridge {
  bridge: TauriBridge;
  invocations: { cmd: string; args: Record<string, unknown> }[];
  /** operant://bus: B2 emits the RAW envelope as the event payload (no { env } wrapper). */
  emit: (env: BusEnvelope) => void;
  /** operant://thumb: the flight-recorder thumbnail, a separate channel from the bus event. */
  emitThumb: (thumb: unknown) => void;
  channels: string[];
  busAttached: () => boolean;
  unlistenCount: () => number;
}

function makeFakeBridge(): FakeBridge {
  const invocations: { cmd: string; args: Record<string, unknown> }[] = [];
  const handlers = new Map<string, (event: { payload: unknown }) => void>();
  const channels: string[] = [];
  let unlistened = 0;

  const bridge: TauriBridge = {
    invoke: async (cmd, args) => {
      invocations.push({ cmd, args: args ?? {} });
      return undefined;
    },
    listen: (ch, h) => {
      channels.push(ch);
      handlers.set(ch, h);
      return Promise.resolve(() => {
        unlistened++;
        handlers.delete(ch);
      });
    },
  };

  return {
    bridge,
    invocations,
    // B2 emits the raw envelope as the payload; feed it exactly that way.
    emit: (env) => handlers.get(BUS_EVENT_CHANNEL)?.({ payload: env }),
    emitThumb: (thumb) => handlers.get(THUMB_EVENT_CHANNEL)?.({ payload: thumb }),
    channels,
    busAttached: () => handlers.has(BUS_EVENT_CHANNEL),
    unlistenCount: () => unlistened,
  };
}

test("subscribes to both the operant://bus and operant://thumb channels on creation", () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);
  assert.ok(fake.channels.includes(BUS_EVENT_CHANNEL), "a listener must be attached to the forwarded bus channel");
  assert.ok(fake.channels.includes(THUMB_EVENT_CHANNEL), "a listener must be attached to the thumbnail channel");
  assert.ok(fake.busAttached(), "the bus listener must be attached");
  client.close();
});

test("dispatches forwarded events by the exact mock prefix rule, over the real recorded fixture", () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);

  const all: string[] = [];
  const run: string[] = [];
  const runStep: string[] = [];
  const runStepExecuted: string[] = [];
  const undo: string[] = [];
  const workflowCompiled: string[] = [];

  client.subscribe("*", (e) => all.push(e.topic));
  client.subscribe("run", (e) => run.push(e.topic));
  client.subscribe("run.step", (e) => runStep.push(e.topic));
  client.subscribe("run.step.executed", (e) => runStepExecuted.push(e.topic));
  client.subscribe("undo", (e) => undo.push(e.topic));
  client.subscribe("workflow.compiled", (e) => workflowCompiled.push(e.topic));

  const envs = fixtureEnvelopes();
  for (const env of envs) fake.emit(env);

  // The fixture carries 22 evt frames; every count below is derived from the
  // real recorded session, not hand-fed data.
  assert.equal(all.length, 22, "* must receive every forwarded event");
  assert.equal(run.length, 19, `"run" must receive every run.* event`);
  assert.equal(runStep.length, 15, `"run.step" must receive only run.step.* events`);
  assert.equal(runStepExecuted.length, 6, "exact topic must receive only run.step.executed");
  assert.equal(undo.length, 2, `"undo" must receive undo.previewed and undo.applied`);
  assert.equal(workflowCompiled.length, 1, "exact topic must receive only its own topic");

  // The prefix subscriber never leaks a sibling namespace.
  assert.ok(!run.includes("workflow.compiled"));
  assert.ok(runStepExecuted.every((t) => t === "run.step.executed"));

  client.close();
});

test("delivers the untouched envelope (v, seq, topic, payload) to subscribers", () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);

  const received: BusEvent[] = [];
  client.subscribe("run.started", (e) => received.push(e));

  const first = fixtureEnvelopes().find((e) => e.topic === "run.started");
  assert.ok(first, "the fixture must contain a run.started event");
  fake.emit(first);

  assert.equal(received.length, 1);
  const got = received[0];
  assert.equal(got.v, 1);
  assert.equal(got.topic, "run.started");
  if (got.topic === "run.started") {
    assert.equal(got.payload.run_id, "run_0");
    assert.equal(got.payload.mode, "explore");
  } else {
    assert.fail("expected a run.started event");
  }

  client.close();
});

test("unsubscribe stops further delivery", () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);

  const received: string[] = [];
  const unsubscribe = client.subscribe("run", (e) => received.push(e.topic));

  const [firstRun] = fixtureEnvelopes().filter((e) => e.topic.startsWith("run"));
  fake.emit(firstRun);
  unsubscribe();
  fake.emit(firstRun);

  assert.equal(received.length, 1, "no delivery after unsubscribe");
  client.close();
});

test("publish routes command topics to invoke(core_call, {cmd, args})", () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);

  client.publish("run.paused", { run_id: "run_9", by: "human" });
  client.publish("run.resumed", { run_id: "run_9" });
  client.publish("run.redirected", { run_id: "run_9", instruction: "open the file menu" });
  client.publish("run.halted", { run_id: "run_9", reason: "human" });
  client.publish("killswitch.engaged", { at_ms: 123 });
  client.publish("undo.previewed", { run_id: "run_9", entries: 1, irreversible: 0 });
  client.publish("undo.applied", { run_id: "run_9", restored: 1, narration: [] });
  client.publish("config.changed", { key: "voice.enabled", value: true });

  // Every routed publish is one core_call carrying { cmd, args }.
  assert.ok(fake.invocations.every((i) => i.cmd === CORE_CALL_COMMAND));
  const calls = fake.invocations.map((i) => i.args as { cmd: string; args: Record<string, unknown> });
  assert.deepEqual(calls, [
    { cmd: "pause", args: { run_id: "run_9" } },
    { cmd: "resume", args: { run_id: "run_9" } },
    { cmd: "redirect", args: { instruction: "open the file menu" } },
    { cmd: "stop", args: { run_id: "run_9" } },
    { cmd: "kill", args: {} },
    { cmd: "preview_undo", args: { run_id: "run_9" } },
    { cmd: "undo_run", args: { run_id: "run_9" } },
    { cmd: "set_settings", args: { key: "voice.enabled", value: true } },
  ]);

  client.close();
});

test("publish no-ops core-owned event topics: the core is their only source", () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);

  // These are exactly the topics the mock run simulators publish. Over a real
  // core they must never be sent as commands, so no fake run is ever faked.
  client.publish("run.started", { run_id: "x", goal: "g", mode: "explore" });
  client.publish("run.step.proposed", { run_id: "x", step: { v: 1, id: "s1", kind: "click" } });
  client.publish("run.step.gated", { run_id: "x", step_id: "s1", gate_kind: "pre", result: "pass" });
  client.publish("run.step.executed", { run_id: "x", step_id: "s1", outcome: "ok", ms: 1, grounding: "uia" });
  client.publish("run.completed", { run_id: "x", outcome: "ok", steps: 1, wall_ms: 1 });

  assert.equal(fake.invocations.length, 0, "no command may be sent for a core-owned event topic");
  client.close();
});

test("close detaches the forwarded-event listener and stops dispatch", async () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);

  const received: string[] = [];
  client.subscribe("*", (e) => received.push(e.topic));

  // Let the listen promise resolve so the unlisten handle is stored.
  await Promise.resolve();

  const [env] = fixtureEnvelopes();
  fake.emit(env);
  assert.equal(received.length, 1);

  client.close();
  assert.equal(fake.unlistenCount(), 2, "close must detach both the bus and thumbnail listeners");

  fake.emit(env);
  assert.equal(received.length, 1, "no dispatch after close");
});

test("correlates an operant://thumb by (run_id, step_id) onto its run-step event's sidecar", () => {
  const fake = makeFakeBridge();
  const client = createRealClient(fake.bridge);

  // A run-step subscriber captures the optional evt sidecar B4's filmstrip reads.
  const sidecars: (EvtSidecar | undefined)[] = [];
  client.subscribe("run.step.executed", (_e, sidecar) => sidecars.push(sidecar));

  // The thumbnail arrives first, on its own channel (contracts/ipc.md section 7).
  const thumb = { run_id: "run_0", step_id: "s2", format: "png", w: 320, h: 200, redacted: true, data_b64: "AAAA" };
  fake.emitThumb(thumb);

  // Then the run-step event on the bus, raw envelope, matching run_id/step_id.
  const executed: BusEnvelope = {
    v: 1,
    seq: 7,
    ts: "2026-07-12T12:00:00.000Z",
    topic: "run.step.executed",
    payload: { run_id: "run_0", step_id: "s2", outcome: "ok", ms: 12, grounding: "uia" },
  };
  fake.emit(executed);

  assert.equal(sidecars.length, 1);
  assert.deepEqual(sidecars[0], { thumb }, "the run-step subscriber receives the correlated thumbnail as its sidecar");

  // A run-step with no matching thumbnail delivers no sidecar (filmstrip draws its placeholder).
  const noThumb: BusEnvelope = {
    v: 1,
    seq: 8,
    ts: "2026-07-12T12:00:01.000Z",
    topic: "run.step.executed",
    payload: { run_id: "run_0", step_id: "s3", outcome: "ok", ms: 12, grounding: "uia" },
  };
  fake.emit(noThumb);
  assert.equal(sidecars.length, 2);
  assert.equal(sidecars[1], undefined, "a step with no thumbnail carries no sidecar");

  client.close();
});
