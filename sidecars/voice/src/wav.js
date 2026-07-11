// Deterministic fake WAV audio: a real, valid RIFF/WAVE header (any real WAV
// parser accepts it) wrapped around a silence payload sized from `text`. No
// synthesis happens here on purpose: this is what the MOCK tts provider
// returns. A real Kokoro-backed provider (see providers/kokoroProvider.js)
// would replace the payload with real PCM samples but should keep using this
// same header shape so callers never need to branch on which provider ran.

const SAMPLE_RATE = 16000; // matches the STT capture rate in docs/specs/voice.md
const CHANNELS = 1;
const BITS_PER_SAMPLE = 16;
const BYTES_PER_SAMPLE = BITS_PER_SAMPLE / 8;
const MS_PER_CHAR = 60; // deterministic stand-in "speaking rate" for the mock

/**
 * @param {string} text
 * @returns {{audio: Buffer, lengthMs: number}}
 */
export function buildFakeWav(text) {
  const lengthMs = Math.max(MS_PER_CHAR, text.length * MS_PER_CHAR);
  const numSamples = Math.round((lengthMs / 1000) * SAMPLE_RATE);
  const dataSize = numSamples * CHANNELS * BYTES_PER_SAMPLE;
  const byteRate = SAMPLE_RATE * CHANNELS * BYTES_PER_SAMPLE;
  const blockAlign = CHANNELS * BYTES_PER_SAMPLE;

  const header = Buffer.alloc(44);
  header.write("RIFF", 0, "ascii");
  header.writeUInt32LE(36 + dataSize, 4);
  header.write("WAVE", 8, "ascii");
  header.write("fmt ", 12, "ascii");
  header.writeUInt32LE(16, 16); // PCM fmt chunk size
  header.writeUInt16LE(1, 20); // audio format: 1 = PCM
  header.writeUInt16LE(CHANNELS, 22);
  header.writeUInt32LE(SAMPLE_RATE, 24);
  header.writeUInt32LE(byteRate, 28);
  header.writeUInt16LE(blockAlign, 32);
  header.writeUInt16LE(BITS_PER_SAMPLE, 34);
  header.write("data", 36, "ascii");
  header.writeUInt32LE(dataSize, 40);

  const data = Buffer.alloc(dataSize, 0); // silence: deterministic and CI-safe
  return { audio: Buffer.concat([header, data]), lengthMs };
}
