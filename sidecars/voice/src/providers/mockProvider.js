// Deterministic mock STT/TTS providers used for CI and the text-mode round
// trip: no model weights, no disk I/O, no network, same input always maps to
// the same output. Real engines are documented seams in ./whisperProvider.js
// (STT) and ./kokoroProvider.js (TTS).
//
// Both providers are lazy: constructing one does no work at all. The first
// stt()/tts() call is what flips `loaded` and bumps `loadCount`, which is
// exactly the behavior test/providers.test.js proves.

import { buildFakeWav } from "../wav.js";

/**
 * Fixture shared by tests and the mock STT provider. `KNOWN_UTTERANCE` stands
 * in for a real 16 kHz mono PCM capture buffer (docs/specs/voice.md); using a
 * readable ASCII marker instead of real PCM samples keeps the fixture
 * inspectable without changing the contract: STT maps this exact buffer to
 * this exact transcript, every time.
 */
export const MOCK_FIXTURE = Object.freeze({
  KNOWN_UTTERANCE: Buffer.from("OPERANT_VOICE_FIXTURE_V1:lights_on"),
  KNOWN_TRANSCRIPT: "turn on the lights",
});

// Pretend VRAM footprint reported once "loaded". Real providers report their
// real usage (see the vramFootprintMb() TODOs in the seam providers).
const MOCK_STT_VRAM_MB = 280;
const MOCK_TTS_VRAM_MB = 190;

export function createMockSttProvider() {
  let loaded = false;
  let loadCount = 0;

  return {
    name: "mock-stt",
    isLoaded: () => loaded,
    loadCount: () => loadCount,
    vramFootprintMb: () => (loaded ? MOCK_STT_VRAM_MB : 0),
    /** @param {Buffer} audioBuffer @returns {Promise<{text: string}>} */
    async stt(audioBuffer) {
      if (!loaded) {
        loaded = true;
        loadCount += 1;
      }
      if (Buffer.isBuffer(audioBuffer) && audioBuffer.equals(MOCK_FIXTURE.KNOWN_UTTERANCE)) {
        return { text: MOCK_FIXTURE.KNOWN_TRANSCRIPT };
      }
      // Deterministic fallback: any non-fixture buffer still behaves
      // predictably instead of throwing or returning random text.
      return { text: "" };
    },
    /** @returns {Promise<number>} mb freed, 0 if it was not loaded. */
    async unload() {
      if (!loaded) return 0;
      loaded = false;
      return MOCK_STT_VRAM_MB;
    },
  };
}

export function createMockTtsProvider() {
  let loaded = false;
  let loadCount = 0;

  return {
    name: "mock-tts",
    isLoaded: () => loaded,
    loadCount: () => loadCount,
    vramFootprintMb: () => (loaded ? MOCK_TTS_VRAM_MB : 0),
    /** @param {string} text @returns {Promise<{audio: Buffer, lengthMs: number}>} */
    async tts(text) {
      if (!loaded) {
        loaded = true;
        loadCount += 1;
      }
      return buildFakeWav(String(text ?? ""));
    },
    /** @returns {Promise<number>} mb freed, 0 if it was not loaded. */
    async unload() {
      if (!loaded) return 0;
      loaded = false;
      return MOCK_TTS_VRAM_MB;
    },
  };
}
