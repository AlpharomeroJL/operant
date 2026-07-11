import test from "node:test";
import assert from "node:assert/strict";

import { Bus } from "../src/bus.js";

test("publish stamps v, seq, ts, topic, payload", () => {
  const bus = new Bus();
  const env = bus.publish("vram.yield", { yielder: "voice", mb: 512 });
  assert.equal(env.v, 1);
  assert.equal(env.seq, 0);
  assert.equal(env.topic, "vram.yield");
  assert.deepEqual(env.payload, { yielder: "voice", mb: 512 });
  assert.equal(typeof env.ts, "string");
  assert.ok(!Number.isNaN(Date.parse(env.ts)), "ts is a parseable timestamp");
});

test("seq increases monotonically across publishes", () => {
  const bus = new Bus();
  const a = bus.publish("sidecar.started", { name: "voice", pid: 1 });
  const b = bus.publish("sidecar.health", { name: "voice", ok: true });
  assert.equal(a.seq, 0);
  assert.equal(b.seq, 1);
});

test("subscribers match exact topic or a prefix.* wildcard, not siblings", () => {
  const bus = new Bus();
  const exact = bus.subscribe("vram.yield");
  const prefix = bus.subscribe("vram.*");
  const sidecarFamily = bus.subscribe("sidecar.*");

  bus.publish("vram.yield", { yielder: "voice", mb: 100 });
  bus.publish("vram.request", { requester: "vision", mb: 4000 });

  assert.equal(exact.events.length, 1, "exact match only sees vram.yield");
  assert.equal(prefix.events.length, 2, "prefix match sees both vram.* events");
  assert.equal(sidecarFamily.events.length, 0, "unrelated family sees nothing");
});

test("a * subscriber sees every topic (used by the process entry point's stdout forwarder)", () => {
  const bus = new Bus();
  const all = bus.subscribe("*");
  bus.publish("sidecar.started", { name: "voice", pid: 1 });
  bus.publish("vram.yield", { yielder: "voice", mb: 1 });
  assert.equal(all.events.length, 2);
});

test("subscribe accepts a push-style callback delivered synchronously", () => {
  const bus = new Bus();
  const seen = [];
  bus.subscribe("sidecar.*", (env) => seen.push(env.topic));
  bus.publish("sidecar.started", { name: "voice", pid: 1 });
  assert.deepEqual(seen, ["sidecar.started"]);
});

test("unsubscribe stops further delivery", () => {
  const bus = new Bus();
  const sub = bus.subscribe("sidecar.*");
  bus.publish("sidecar.started", { name: "voice", pid: 1 });
  sub.unsubscribe();
  bus.publish("sidecar.started", { name: "voice", pid: 2 });
  assert.equal(sub.events.length, 1);
});

test("drain empties the recorded events", () => {
  const bus = new Bus();
  const sub = bus.subscribe("sidecar.started");
  bus.publish("sidecar.started", { name: "voice", pid: 1 });
  const drained = sub.drain();
  assert.equal(drained.length, 1);
  assert.equal(sub.events.length, 0);
});
