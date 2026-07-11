// Clock abstraction mirroring crates/core/src/supervisor.rs's `Clock` trait:
// production code runs on real wall time and real timers; tests inject a
// TestClock that only moves when told to, so timing-sensitive assertions
// (the 300ms push-to-talk tail, the 2s VRAM yield budget) are deterministic
// and never sleep for real.

export class SystemClock {
  nowMs() {
    return Date.now();
  }

  /**
   * @param {() => void} fn
   * @param {number} ms
   * @returns {{clear: () => void}}
   */
  setTimer(fn, ms) {
    const handle = setTimeout(fn, ms);
    return { clear: () => clearTimeout(handle) };
  }
}

export class TestClock {
  #now = 0;
  #timers = [];

  nowMs() {
    return this.#now;
  }

  /**
   * @param {() => void} fn
   * @param {number} ms
   * @returns {{clear: () => void}}
   */
  setTimer(fn, ms) {
    const timer = { dueMs: this.#now + ms, fn, fired: false, cleared: false };
    this.#timers.push(timer);
    return {
      clear: () => {
        timer.cleared = true;
      },
    };
  }

  /**
   * Advance the clock by `ms`, synchronously firing any timer now due, in
   * due-time order, including timers newly scheduled by a firing timer.
   */
  advance(ms) {
    this.#now += ms;
    let progressed = true;
    while (progressed) {
      progressed = false;
      const due = this.#timers
        .filter((t) => !t.fired && !t.cleared && t.dueMs <= this.#now)
        .sort((a, b) => a.dueMs - b.dueMs);
      for (const t of due) {
        if (t.fired || t.cleared) continue;
        t.fired = true;
        progressed = true;
        t.fn();
      }
    }
  }
}
