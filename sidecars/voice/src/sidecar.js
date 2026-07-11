// Composition root: wires providers, the push-to-talk state machine, the
// VRAM client, and the bus together into one voice sidecar. Constructing a
// sidecar does no loading at all (see test/providers.test.js); the process
// entry point (./index.js) and the test suite both build on this same
// surface, the entry point over a stdio protocol, tests in-process directly.

import { Bus } from "./bus.js";
import { SystemClock } from "./clock.js";
import { PushToTalk } from "./pushToTalk.js";
import { VramClient } from "./vram.js";
import { TOPIC } from "./topics.js";
import { createMockSttProvider, createMockTtsProvider } from "./providers/mockProvider.js";
import { createWhisperSttProvider } from "./providers/whisperProvider.js";
import { createKokoroTtsProvider } from "./providers/kokoroProvider.js";

export const SIDECAR_NAME = "voice";
export const PUSH_TO_TALK_TAIL_MS = 300; // docs/specs/voice.md: "held-to-record with 300 ms tail"

function buildProviders(providerKind) {
  switch (providerKind) {
    case "mock":
      return { stt: createMockSttProvider(), tts: createMockTtsProvider() };
    case "real":
      // Documented seam: see providers/whisperProvider.js and
      // providers/kokoroProvider.js. Neither loads anything at construction
      // time; both throw NotImplementedError on first actual use until the
      // real model wiring lands.
      return { stt: createWhisperSttProvider(), tts: createKokoroTtsProvider() };
    default:
      throw new TypeError(`unknown providerKind: ${providerKind}`);
  }
}

/**
 * @param {object} [opts]
 * @param {"mock"|"real"} [opts.providerKind]
 * @param {string} [opts.name]
 * @param {import("./bus.js").Bus} [opts.bus]
 * @param {{nowMs: () => number, setTimer: Function}} [opts.clock]
 * @param {number} [opts.tailMs]
 */
export function createSidecar({
  providerKind = "mock",
  name = SIDECAR_NAME,
  bus = new Bus(),
  clock = new SystemClock(),
  tailMs = PUSH_TO_TALK_TAIL_MS,
} = {}) {
  const { stt: sttProvider, tts: ttsProvider } = buildProviders(providerKind);

  const pushToTalk = new PushToTalk({
    sttProvider,
    clock,
    tailMs,
    bus,
    sourceName: name,
  });

  const vram = new VramClient({
    bus,
    clock,
    sourceName: name,
    providers: [sttProvider, ttsProvider],
  });

  return {
    name,
    bus,
    clock,
    sttProvider,
    ttsProvider,
    pushToTalk,
    vram,

    /** Emits sidecar.started with this process's real pid. */
    start() {
      bus.publish(TOPIC.SIDECAR_STARTED, { name, pid: process.pid });
      return process.pid;
    },

    /** Emits sidecar.health, including the current combined VRAM footprint. */
    reportHealth(ok = true) {
      const rssMb = Math.round(process.memoryUsage().rss / (1024 * 1024));
      const vramMb = sttProvider.vramFootprintMb() + ttsProvider.vramFootprintMb();
      bus.publish(TOPIC.SIDECAR_HEALTH, { name, ok, rss_mb: rssMb, vram_mb: vramMb });
    },

    /** @param {string} text @returns {Promise<{audio: Buffer, lengthMs: number}>} */
    async speak(text) {
      return ttsProvider.tts(text);
    },
  };
}
