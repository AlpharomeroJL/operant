// docs/specs/voice.md: "yield (unload) within 2 s when the grounder requests
// headroom, reload lazily after." contracts/bus_events.md: `vram.yield` is
// {yielder, mb}, e.g. "voice yields to vision grounder".

import { TOPIC } from "./topics.js";

/** The 2s yield budget from docs/specs/voice.md. */
export const YIELD_BUDGET_MS = 2000;

/**
 * Handles inbound VRAM-yield requests from the C1 broker
 * (crates/core/src/supervisor.rs's VramBroker): unloads every loaded
 * provider it was given and reports `vram.yield` on the bus. Reload is
 * lazy: nothing here reloads a provider; the next stt()/tts() call does
 * that on its own via each provider's own load-on-first-use.
 */
export class VramClient {
  /**
   * @param {object} opts
   * @param {import("./bus.js").Bus} opts.bus
   * @param {{nowMs: () => number}} opts.clock
   * @param {string} opts.sourceName
   * @param {Array<{isLoaded: () => boolean, unload: () => Promise<number>}>} opts.providers
   */
  constructor({ bus, clock, sourceName, providers }) {
    this._bus = bus;
    this._clock = clock;
    this._sourceName = sourceName;
    this._providers = providers;
  }

  /**
   * @param {number} [budgetMs] defaults to the 2s spec budget.
   * @returns {Promise<{elapsedMs: number, freedMb: number, withinBudget: boolean}>}
   */
  async requestYield(budgetMs = YIELD_BUDGET_MS) {
    const startedMs = this._clock.nowMs();
    let freedMb = 0;
    for (const provider of this._providers) {
      if (provider.isLoaded()) {
        freedMb += await provider.unload();
      }
    }
    const elapsedMs = this._clock.nowMs() - startedMs;

    if (freedMb > 0) {
      this._bus.publish(TOPIC.VRAM_YIELD, { yielder: this._sourceName, mb: freedMb });
    }

    return { elapsedMs, freedMb, withinBudget: elapsedMs <= budgetMs };
  }
}
