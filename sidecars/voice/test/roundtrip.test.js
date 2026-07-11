// The lane's primary BAR proof: text-mode STT -> intent -> TTS round trip
// with the mock provider. Runnable standalone: `node test/roundtrip.test.js`.

import test from "node:test";
import assert from "node:assert/strict";

import { runTextModeRoundTrip } from "../testlib/roundTrip.js";
import { MOCK_FIXTURE } from "../src/providers/mockProvider.js";

test("text-mode round trip: STT -> intent -> TTS with the mock provider", async () => {
  const { intent, spoken } = await runTextModeRoundTrip();

  assert.equal(intent.text, MOCK_FIXTURE.KNOWN_TRANSCRIPT);

  assert.ok(Buffer.isBuffer(spoken.audio), "tts() returns a Buffer");
  assert.equal(spoken.audio.subarray(0, 4).toString("ascii"), "RIFF");
  assert.equal(spoken.audio.subarray(8, 12).toString("ascii"), "WAVE");
  assert.ok(spoken.lengthMs > 0, "fake wav reports a positive length");
  assert.equal(
    spoken.audio.length,
    44 + Math.round((spoken.lengthMs / 1000) * 16000) * 2,
    "declared data size matches the actual buffer length"
  );
});

test("round trip is deterministic: same fixture in, same transcript and audio out", async () => {
  const a = await runTextModeRoundTrip();
  const b = await runTextModeRoundTrip();
  assert.equal(a.intent.text, b.intent.text);
  assert.equal(a.spoken.lengthMs, b.spoken.lengthMs);
  assert.ok(a.spoken.audio.equals(b.spoken.audio), "identical text always produces identical audio bytes");
});
