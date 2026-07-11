import test from "node:test";
import assert from "node:assert/strict";

import { createSidecar } from "../src/sidecar.js";
import { Bus } from "../src/bus.js";

test("start publishes sidecar.started with this process's real pid", () => {
  const bus = new Bus();
  const sub = bus.subscribe("sidecar.started");
  const sidecar = createSidecar({ providerKind: "mock", bus });

  const pid = sidecar.start();

  assert.equal(pid, process.pid);
  const [env] = sub.drain();
  assert.equal(env.topic, "sidecar.started");
  assert.deepEqual(env.payload, { name: "voice", pid: process.pid });
});

test("reportHealth publishes sidecar.health with ok and vram footprint", async () => {
  const bus = new Bus();
  const sub = bus.subscribe("sidecar.health");
  const sidecar = createSidecar({ providerKind: "mock", bus });

  sidecar.reportHealth(true);
  let [env] = sub.drain();
  assert.equal(env.topic, "sidecar.health");
  assert.equal(env.payload.name, "voice");
  assert.equal(env.payload.ok, true);
  assert.equal(env.payload.vram_mb, 0, "nothing loaded yet");

  await sidecar.sttProvider.stt(Buffer.from("x"));
  sidecar.reportHealth(true);
  [env] = sub.drain();
  assert.ok(env.payload.vram_mb > 0, "reports footprint once a provider is loaded");
});

test("createSidecar defaults to name voice", () => {
  const sidecar = createSidecar();
  assert.equal(sidecar.name, "voice");
});

test("an unknown providerKind fails clearly instead of silently falling back", () => {
  assert.throws(() => createSidecar({ providerKind: "nope" }), TypeError);
});
