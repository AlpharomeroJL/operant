// Minimal, synchronous, in-process pub/sub mirroring the envelope shape and
// exact/prefix subscription semantics documented in contracts/bus_events.md
// and implemented for the real bus in crates/core/src/bus.rs. This is the
// Node-side half of the same contract, not a port of the Rust code: no
// cross-thread channel to model since Node is single-threaded, so delivery
// here is a direct synchronous call in subscriber registration order.
//
// contracts/bus_events.md: "Every event on the bus is wrapped" in
// {v, seq, ts, topic, payload}; "Subscribers match on exact topic or prefix
// (`run.*`)."

/**
 * @typedef {object} Envelope
 * @property {number} v - envelope version, currently 1.
 * @property {number} seq - monotonically increasing per-bus sequence number.
 * @property {string} ts - ISO 8601 UTC timestamp, assigned at publish.
 * @property {string} topic - dot-separated topic string.
 * @property {object} payload - topic-specific payload.
 */

function matchesTopic(topic, pattern) {
  if (pattern === "*") return true; // local convenience: the process entry point's stdout forwarder
  if (pattern === topic) return true;
  if (pattern.endsWith(".*")) {
    const prefix = pattern.slice(0, -1); // keep the trailing dot
    return topic.startsWith(prefix);
  }
  return false;
}

export class Bus {
  #seq = 0;
  #subs = [];

  /**
   * Publish a payload under a topic. `seq` and `ts` are stamped here.
   * @param {string} topic
   * @param {object} payload
   * @returns {Envelope}
   */
  publish(topic, payload) {
    const env = {
      v: 1,
      seq: this.#seq++,
      ts: new Date().toISOString(),
      topic,
      payload,
    };
    for (const sub of this.#subs) {
      if (matchesTopic(env.topic, sub.pattern)) {
        sub.events.push(env);
        if (sub.onEvent) sub.onEvent(env);
      }
    }
    return env;
  }

  /**
   * @param {string} pattern - exact topic, a `prefix.*` wildcard, or `*` for everything.
   * @param {(env: Envelope) => void} [onEvent] - optional push-style callback, called synchronously on each match.
   * @returns {{pattern: string, events: Envelope[], drain: () => Envelope[], unsubscribe: () => void}}
   */
  subscribe(pattern, onEvent) {
    const sub = { pattern, onEvent, events: [] };
    this.#subs.push(sub);
    return {
      pattern,
      events: sub.events,
      drain: () => sub.events.splice(0, sub.events.length),
      unsubscribe: () => {
        this.#subs = this.#subs.filter((s) => s !== sub);
      },
    };
  }
}
