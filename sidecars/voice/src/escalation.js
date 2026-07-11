// docs/specs/voice.md: "every spoken escalation also renders as text (voice
// is additive, never the only channel)". contracts/bus_events.md's
// gate.escalation payload is {run_id, step_id?, sentence, requires_approval};
// `sentence` is documented there as "plain language" - that is the text this
// function renders and, optionally, speaks.

/**
 * Renders an escalation for both channels. `text` is always present: a TTS
 * failure, a disabled voice mode, or no configured provider at all must never
 * take the text channel down with it. `spoken` is present only when audio was
 * actually produced.
 *
 * @param {{sentence: string, run_id?: string, step_id?: string, requires_approval?: boolean}} escalation
 * @param {{ttsProvider?: {tts: (text: string) => Promise<{audio: Buffer, lengthMs: number}>}, voiceEnabled?: boolean}} [options]
 * @returns {Promise<{text: string, spoken?: {audio: Buffer, lengthMs: number}}>}
 */
export async function renderEscalation(escalation, options = {}) {
  if (!escalation || typeof escalation.sentence !== "string" || !escalation.sentence) {
    throw new TypeError("escalation.sentence is required");
  }
  const { ttsProvider = null, voiceEnabled = true } = options;
  const text = escalation.sentence;

  if (!voiceEnabled || !ttsProvider) {
    return { text };
  }

  try {
    const { audio, lengthMs } = await ttsProvider.tts(text);
    return { text, spoken: { audio, lengthMs } };
  } catch {
    // Voice is additive: a TTS failure must never take the text channel down with it.
    return { text };
  }
}
