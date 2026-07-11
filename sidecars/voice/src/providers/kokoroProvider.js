// Documented seam: real TTS via a Kokoro-class local model. See
// whisperProvider.js for the parallel STT seam; the same lazy-load
// discipline applies here (load only inside the first tts() call, never at
// module scope).
//
// To wire up a real engine:
//   1. Add a Kokoro ONNX/PyTorch runtime binding as an optional dependency,
//      loaded with a dynamic `import()` INSIDE the first tts() call.
//   2. Resolve the configured voice and speaking rate (docs/specs/voice.md:
//      "one default voice, speaking rate configurable").
//   3. Synthesize `text` to 16-bit PCM and wrap it in the same WAV framing
//      ../wav.js already produces for the mock, so callers never branch on
//      which provider produced the audio.
//   4. Report real usage from vramFootprintMb() instead of a constant, and
//      flip isLoaded()/loadCount() for real once loading actually happens.

export class NotImplementedError extends Error {
  constructor(what) {
    super(`${what}: Kokoro wiring is a documented seam, not implemented yet`);
    this.name = "NotImplementedError";
  }
}

const KOKORO_VOICE_DEFAULT = "default";
const KOKORO_SPEAKING_RATE_DEFAULT = 1.0;

/** @param {{voice?: string, speakingRate?: number}} [options] */
export function createKokoroTtsProvider(options = {}) {
  const voice = options.voice || KOKORO_VOICE_DEFAULT;
  const speakingRate = options.speakingRate ?? KOKORO_SPEAKING_RATE_DEFAULT;

  return {
    name: "kokoro",
    voice,
    speakingRate,
    isLoaded: () => false,
    loadCount: () => 0,
    vramFootprintMb: () => 0,
    async tts() {
      throw new NotImplementedError("Kokoro tts()");
    },
    async unload() {
      return 0;
    },
  };
}
