// Push-to-talk state machine: idle -> recording -> tail -> transcribing -> idle.
//
// docs/specs/voice.md: "push-to-talk key held-to-record with 300 ms tail".
// Holding the key starts `recording`; releasing starts a `tail` window (so a
// trailing word is not clipped right at release) before the accumulated
// audio is handed to STT. The tail is driven by an injected Clock so tests
// advance it deterministically instead of sleeping for real.

import { EventEmitter } from "node:events";
import { TOPIC } from "./topics.js";

/**
 * Emits (via EventEmitter):
 *   "state"  (newState: "idle"|"recording"|"tail"|"transcribing")
 *   "intent" ({text, atMs}) - also published on `bus` as voice.intent, the
 *             channel the palette is wired to, so recognized speech shows up
 *             as a message/event rather than a direct function call.
 *   "error"  (Error) - the STT call itself failed; the machine still returns
 *             to idle and no intent is emitted or published.
 */
export class PushToTalk extends EventEmitter {
  /**
   * @param {object} opts
   * @param {{stt: (audio: Buffer) => Promise<{text: string}>}} opts.sttProvider
   * @param {{nowMs: () => number, setTimer: (fn: () => void, ms: number) => {clear: () => void}}} opts.clock
   * @param {number} [opts.tailMs]
   * @param {import("./bus.js").Bus | null} [opts.bus]
   * @param {string} [opts.sourceName]
   */
  constructor({ sttProvider, clock, tailMs = 300, bus = null, sourceName = "voice" }) {
    super();
    if (!sttProvider) throw new TypeError("sttProvider is required");
    if (!clock) throw new TypeError("clock is required");
    this._stt = sttProvider;
    this._clock = clock;
    this._tailMs = tailMs;
    this._bus = bus;
    this._sourceName = sourceName;
    this._chunks = [];
    this._tailTimer = null;
    this.state = "idle";
  }

  _setState(next) {
    this.state = next;
    this.emit("state", next);
  }

  /** Key pressed: begin accumulating audio. */
  holdStart() {
    if (this.state !== "idle") {
      throw new Error(`push-to-talk: cannot start recording from state "${this.state}"`);
    }
    this._chunks = [];
    this._setState("recording");
  }

  /** Append a chunk of captured audio while recording. */
  feed(chunk) {
    if (this.state !== "recording") {
      throw new Error(`push-to-talk: cannot feed audio from state "${this.state}"`);
    }
    this._chunks.push(chunk);
  }

  /** Key released: start the tail window, then transcribe once it elapses. */
  holdEnd() {
    if (this.state !== "recording") {
      throw new Error(`push-to-talk: cannot release from state "${this.state}"`);
    }
    this._setState("tail");
    this._tailTimer = this._clock.setTimer(() => {
      this._finishTail();
    }, this._tailMs);
  }

  /** Abort an in-progress hold (e.g. the user cancels) without running STT. */
  cancel() {
    if (this.state === "idle") return;
    if (this._tailTimer) {
      this._tailTimer.clear();
      this._tailTimer = null;
    }
    this._chunks = [];
    this._setState("idle");
  }

  async _finishTail() {
    this._setState("transcribing");
    const audio = Buffer.concat(this._chunks);
    this._chunks = [];

    let result;
    try {
      result = await this._stt.stt(audio);
    } catch (err) {
      this._setState("idle");
      this.emit("error", err);
      return null;
    }

    this._setState("idle");
    const intent = { text: result.text, atMs: this._clock.nowMs() };
    if (this._bus) {
      this._bus.publish(TOPIC.VOICE_INTENT, { source: this._sourceName, text: intent.text });
    }
    this.emit("intent", intent);
    return intent;
  }
}
