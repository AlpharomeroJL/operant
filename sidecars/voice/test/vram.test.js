import test from "node:test";
import assert from "node:assert/strict";

import { VramClient, YIELD_BUDGET_MS } from "../src/vram.js";
import { TestClock } from "../src/clock.js";
import { Bus } from "../src/bus.js";
import { createMockSttProvider, createMockTtsProvider, MOCK_FIXTURE } from "../src/providers/mockProvider.js";

test("yield on an unloaded sidecar is a no-op: nothing to free, nothing published", async () => {
  const bus = new Bus();
  const sub = bus.subscribe("vram.yield");
  const clock = new TestClock();
  const stt = createMockSttProvider();
  const tts = createMockTtsProvider();
  const vram = new VramClient({ bus, clock, sourceName: "voice", providers: [stt, tts] });

  const result = await vram.requestYield();
  assert.equal(result.freedMb, 0);
  assert.equal(result.withinBudget, true);
  assert.equal(sub.drain().length, 0);
});

test("yield unloads every loaded provider and reports vram.yield with the total freed mb", async () => {
  const bus = new Bus();
  const sub = bus.subscribe("vram.yield");
  const clock = new TestClock();
  const stt = createMockSttProvider();
  const tts = createMockTtsProvider();
  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  await tts.tts("hello");
  assert.equal(stt.isLoaded(), true);
  assert.equal(tts.isLoaded(), true);

  const vram = new VramClient({ bus, clock, sourceName: "voice", providers: [stt, tts] });
  const result = await vram.requestYield();

  assert.equal(stt.isLoaded(), false);
  assert.equal(tts.isLoaded(), false);
  assert.ok(result.freedMb > 0);
  assert.equal(result.withinBudget, true);

  const [env] = sub.drain();
  assert.equal(env.topic, "vram.yield");
  assert.equal(env.payload.yielder, "voice");
  assert.equal(env.payload.mb, result.freedMb);
});

test("yield within budget uses the 2s default from docs/specs/voice.md", async () => {
  const clock = new TestClock();
  const bus = new Bus();
  const stt = createMockSttProvider();
  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  const vram = new VramClient({ bus, clock, sourceName: "voice", providers: [stt] });

  const result = await vram.requestYield();
  assert.equal(YIELD_BUDGET_MS, 2000);
  assert.equal(result.elapsedMs, 0, "the mock unload is synchronous on a fake clock that never advances on its own");
  assert.equal(result.withinBudget, true);
});

test("yield reports budget exceeded when unload is slow", async () => {
  const clock = new TestClock();
  const bus = new Bus();
  const slowProvider = {
    isLoaded: () => true,
    async unload() {
      clock.advance(2500); // simulate a slow real unload consuming 2.5s
      return 512;
    },
  };
  const vram = new VramClient({ bus, clock, sourceName: "voice", providers: [slowProvider] });

  const result = await vram.requestYield();
  assert.equal(result.elapsedMs, 2500);
  assert.equal(result.withinBudget, false);
});

test("reload after yield is lazy: the next stt() call reloads on its own", async () => {
  const clock = new TestClock();
  const bus = new Bus();
  const stt = createMockSttProvider();
  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  const vram = new VramClient({ bus, clock, sourceName: "voice", providers: [stt] });
  await vram.requestYield();
  assert.equal(stt.isLoaded(), false);

  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  assert.equal(stt.isLoaded(), true, "using it again reloads it without any explicit reload call");
  assert.equal(stt.loadCount(), 2);
});
