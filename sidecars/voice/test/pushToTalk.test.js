import test from "node:test";
import assert from "node:assert/strict";

import { PushToTalk } from "../src/pushToTalk.js";
import { TestClock } from "../src/clock.js";
import { Bus } from "../src/bus.js";
import { createMockSttProvider, MOCK_FIXTURE } from "../src/providers/mockProvider.js";
import { waitForEvent } from "../testlib/waitForEvent.js";

function makePushToTalk(overrides = {}) {
  const clock = overrides.clock || new TestClock();
  const bus = overrides.bus || new Bus();
  const sttProvider = overrides.sttProvider || createMockSttProvider();
  const ptt = new PushToTalk({ sttProvider, clock, bus, sourceName: "voice", tailMs: 300 });
  return { ptt, clock, bus, sttProvider };
}

test("starts idle", () => {
  const { ptt } = makePushToTalk();
  assert.equal(ptt.state, "idle");
});

test("holdStart moves to recording, holdEnd moves to tail, waits the full 300ms before transcribing", async () => {
  const { ptt, clock } = makePushToTalk();
  const intentPromise = waitForEvent(ptt, "intent");

  ptt.holdStart();
  assert.equal(ptt.state, "recording");

  ptt.feed(MOCK_FIXTURE.KNOWN_UTTERANCE);
  ptt.holdEnd();
  assert.equal(ptt.state, "tail");

  clock.advance(299);
  assert.equal(ptt.state, "tail", "must not finalize a millisecond early");

  clock.advance(1);
  const intent = await intentPromise;
  assert.equal(intent.text, MOCK_FIXTURE.KNOWN_TRANSCRIPT);
  assert.equal(ptt.state, "idle");
});

test("multiple feed() chunks are concatenated before STT sees them", async () => {
  const { ptt, clock } = makePushToTalk();
  const half = Math.floor(MOCK_FIXTURE.KNOWN_UTTERANCE.length / 2);
  const intentPromise = waitForEvent(ptt, "intent");
  ptt.holdStart();
  ptt.feed(MOCK_FIXTURE.KNOWN_UTTERANCE.subarray(0, half));
  ptt.feed(MOCK_FIXTURE.KNOWN_UTTERANCE.subarray(half));
  ptt.holdEnd();
  clock.advance(300);
  const intent = await intentPromise;
  assert.equal(intent.text, MOCK_FIXTURE.KNOWN_TRANSCRIPT);
});

test("intent is published to the bus as voice.intent (the palette's channel)", async () => {
  const { ptt, clock, bus } = makePushToTalk();
  const sub = bus.subscribe("voice.intent");
  ptt.holdStart();
  ptt.feed(MOCK_FIXTURE.KNOWN_UTTERANCE);
  ptt.holdEnd();
  clock.advance(300);
  await waitForEvent(ptt, "intent");
  const [env] = sub.drain();
  assert.equal(env.topic, "voice.intent");
  assert.equal(env.payload.text, MOCK_FIXTURE.KNOWN_TRANSCRIPT);
  assert.equal(env.payload.source, "voice");
  assert.equal(env.v, 1);
});

test("feed() before holdStart throws", () => {
  const { ptt } = makePushToTalk();
  assert.throws(() => ptt.feed(Buffer.from("x")));
});

test("holdStart while already recording throws", () => {
  const { ptt } = makePushToTalk();
  ptt.holdStart();
  assert.throws(() => ptt.holdStart());
});

test("holdEnd while idle throws", () => {
  const { ptt } = makePushToTalk();
  assert.throws(() => ptt.holdEnd());
});

test("cancel during recording returns to idle without calling stt", () => {
  let called = false;
  const sttProvider = {
    isLoaded: () => false,
    loadCount: () => 0,
    vramFootprintMb: () => 0,
    async stt() {
      called = true;
      return { text: "should not happen" };
    },
    async unload() {
      return 0;
    },
  };
  const { ptt } = makePushToTalk({ sttProvider });
  ptt.holdStart();
  ptt.feed(Buffer.from("x"));
  ptt.cancel();
  assert.equal(ptt.state, "idle");
  assert.equal(called, false);
});

test("cancel during the tail window also aborts before stt runs", () => {
  let called = false;
  const sttProvider = {
    isLoaded: () => false,
    loadCount: () => 0,
    vramFootprintMb: () => 0,
    async stt() {
      called = true;
      return { text: "should not happen" };
    },
    async unload() {
      return 0;
    },
  };
  const { ptt, clock } = makePushToTalk({ sttProvider });
  ptt.holdStart();
  ptt.feed(Buffer.from("x"));
  ptt.holdEnd();
  ptt.cancel();
  assert.equal(ptt.state, "idle");
  clock.advance(1000);
  assert.equal(called, false, "the cleared tail timer must never fire");
});

test("an stt failure emits error, returns to idle, and does not publish an intent", async () => {
  const bus = new Bus();
  const sub = bus.subscribe("voice.intent");
  const sttProvider = {
    isLoaded: () => false,
    loadCount: () => 0,
    vramFootprintMb: () => 0,
    async stt() {
      throw new Error("model crashed");
    },
    async unload() {
      return 0;
    },
  };
  const { ptt, clock } = makePushToTalk({ sttProvider, bus });
  const errorPromise = waitForEvent(ptt, "error");
  ptt.holdStart();
  ptt.feed(Buffer.from("x"));
  ptt.holdEnd();
  clock.advance(300);
  const err = await errorPromise;
  assert.equal(err.message, "model crashed");
  assert.equal(ptt.state, "idle");
  assert.equal(sub.drain().length, 0);
});
