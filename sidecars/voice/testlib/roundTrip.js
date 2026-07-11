// Shared by test/roundtrip.test.js (correctness) and test/network.test.js
// (zero-network), so both exercise the exact same flow rather than two
// slightly different ones.

import { createSidecar } from "../src/sidecar.js";
import { TestClock } from "../src/clock.js";
import { MOCK_FIXTURE } from "../src/providers/mockProvider.js";
import { waitForEvent } from "./waitForEvent.js";

/**
 * Drives one full text-mode round trip end to end with the mock provider:
 * push-to-talk hold -> feed the known fixture buffer -> release -> the 300ms
 * tail elapses on a fake clock -> STT resolves the known transcript -> that
 * becomes an intent -> a reply is spoken back through TTS.
 *
 * "Text-mode": mock audio in CI, no real microphone or speaker, no real
 * model weights, deterministic in and out.
 *
 * @param {{bus?: import("../src/bus.js").Bus}} [opts]
 * @returns {Promise<{sidecar: object, intent: {text: string, atMs: number}, spoken: {audio: Buffer, lengthMs: number}}>}
 */
export async function runTextModeRoundTrip({ bus } = {}) {
  const clock = new TestClock();
  const sidecar = createSidecar({ providerKind: "mock", clock, bus });

  const intentPromise = waitForEvent(sidecar.pushToTalk, "intent");
  sidecar.pushToTalk.holdStart();
  sidecar.pushToTalk.feed(MOCK_FIXTURE.KNOWN_UTTERANCE);
  sidecar.pushToTalk.holdEnd();
  clock.advance(300);
  const intent = await intentPromise;

  const spoken = await sidecar.speak(`Okay: ${intent.text}`);

  return { sidecar, intent, spoken };
}
