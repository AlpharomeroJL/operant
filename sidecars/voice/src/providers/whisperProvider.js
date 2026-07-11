// Documented seam: real STT via whisper.cpp. Per the lane brief, wiring a
// real engine is not required to run; calling stt() throws NotImplementedError
// instead of returning fake data, so this provider can never be silently
// mistaken for the mock.
//
// To wire up a real engine:
//   1. Add whisper.cpp bindings (a native addon, or spawn the whisper.cpp CLI
//      as a child process and parse its output) as an optional dependency,
//      loaded with a dynamic `import()` INSIDE the first stt() call, never at
//      module scope. That is what keeps "load on first use" true for the
//      real provider, not just the mock (docs/specs/voice.md).
//   2. Resolve the configured model (default: whisper small.en quantized,
//      per docs/specs/voice.md) from the local model cache.
//   3. Run VAD trim on the incoming 16 kHz mono PCM buffer before inference.
//   4. Stream partial hypotheses back through a caller-supplied onPartial
//      hook as they arrive, matching "streaming partials to the palette" in
//      the spec.
//   5. Report real usage from vramFootprintMb() instead of a constant, and
//      flip isLoaded()/loadCount() for real once loading actually happens.

export class NotImplementedError extends Error {
  constructor(what) {
    super(`${what}: whisper.cpp wiring is a documented seam, not implemented yet`);
    this.name = "NotImplementedError";
  }
}

const WHISPER_MODEL_DEFAULT = "whisper-small-en-q5_1";

/** @param {{model?: string}} [options] */
export function createWhisperSttProvider(options = {}) {
  const model = options.model || WHISPER_MODEL_DEFAULT;

  return {
    name: "whisper.cpp",
    model,
    isLoaded: () => false,
    loadCount: () => 0,
    vramFootprintMb: () => 0,
    async stt() {
      throw new NotImplementedError("whisper.cpp stt()");
    },
    async unload() {
      return 0;
    },
  };
}
