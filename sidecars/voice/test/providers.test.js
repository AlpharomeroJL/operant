import test from "node:test";
import assert from "node:assert/strict";

import { createMockSttProvider, createMockTtsProvider, MOCK_FIXTURE } from "../src/providers/mockProvider.js";
import { createWhisperSttProvider, NotImplementedError as WhisperNotImplemented } from "../src/providers/whisperProvider.js";
import { createKokoroTtsProvider, NotImplementedError as KokoroNotImplemented } from "../src/providers/kokoroProvider.js";
import { createSidecar } from "../src/sidecar.js";

test("mock stt/tts providers are not loaded at construction", () => {
  const stt = createMockSttProvider();
  const tts = createMockTtsProvider();
  assert.equal(stt.isLoaded(), false);
  assert.equal(stt.loadCount(), 0);
  assert.equal(tts.isLoaded(), false);
  assert.equal(tts.loadCount(), 0);
});

test("stt provider loads only on its first stt() call, once", async () => {
  const stt = createMockSttProvider();
  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  assert.equal(stt.isLoaded(), true);
  assert.equal(stt.loadCount(), 1);
  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  assert.equal(stt.loadCount(), 1, "a second call does not reload");
});

test("tts provider loads only on its first tts() call, once", async () => {
  const tts = createMockTtsProvider();
  await tts.tts("hello");
  assert.equal(tts.isLoaded(), true);
  assert.equal(tts.loadCount(), 1);
  await tts.tts("hello again");
  assert.equal(tts.loadCount(), 1, "a second call does not reload");
});

test("loading stt does not load tts and vice versa", async () => {
  const stt = createMockSttProvider();
  const tts = createMockTtsProvider();
  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  assert.equal(tts.isLoaded(), false);
  await tts.tts("hi");
  assert.equal(stt.isLoaded(), true, "unaffected, still loaded from before");
});

test("mock stt maps the known fixture buffer to the known transcript, deterministically", async () => {
  const stt = createMockSttProvider();
  const { text } = await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  assert.equal(text, MOCK_FIXTURE.KNOWN_TRANSCRIPT);
});

test("mock stt returns empty text for a buffer that is not the known fixture", async () => {
  const stt = createMockSttProvider();
  const { text } = await stt.stt(Buffer.from("some other audio"));
  assert.equal(text, "");
});

test("mock tts returns a fake wav header plus a length", async () => {
  const tts = createMockTtsProvider();
  const { audio, lengthMs } = await tts.tts("hello world");
  assert.ok(Buffer.isBuffer(audio));
  assert.equal(audio.subarray(0, 4).toString("ascii"), "RIFF");
  assert.equal(audio.subarray(8, 12).toString("ascii"), "WAVE");
  assert.equal(audio.subarray(12, 16).toString("ascii"), "fmt ");
  assert.equal(audio.subarray(36, 40).toString("ascii"), "data");
  assert.ok(lengthMs > 0);
});

test("unload is safe when never loaded, and reports freed mb only when it was loaded", async () => {
  const stt = createMockSttProvider();
  assert.equal(await stt.unload(), 0, "never loaded, nothing to free");
  await stt.stt(MOCK_FIXTURE.KNOWN_UTTERANCE);
  const freed = await stt.unload();
  assert.ok(freed > 0);
  assert.equal(stt.isLoaded(), false);
});

test("constructing a full sidecar does not load any provider (lazy-load, sidecar level)", () => {
  const sidecar = createSidecar({ providerKind: "mock" });
  assert.equal(sidecar.sttProvider.isLoaded(), false);
  assert.equal(sidecar.ttsProvider.isLoaded(), false);
  assert.equal(sidecar.sttProvider.loadCount(), 0);
  assert.equal(sidecar.ttsProvider.loadCount(), 0);
});

test("whisper.cpp and kokoro seams never load and fail loudly instead of faking data", async () => {
  const whisper = createWhisperSttProvider();
  const kokoro = createKokoroTtsProvider();
  assert.equal(whisper.isLoaded(), false);
  assert.equal(kokoro.isLoaded(), false);
  await assert.rejects(() => whisper.stt(Buffer.from("x")), WhisperNotImplemented);
  await assert.rejects(() => kokoro.tts("x"), KokoroNotImplemented);
});
