import test from "node:test";
import assert from "node:assert/strict";

import { TestClock, SystemClock } from "../src/clock.js";

test("TestClock starts at zero and only moves on advance()", () => {
  const clock = new TestClock();
  assert.equal(clock.nowMs(), 0);
  clock.advance(150);
  assert.equal(clock.nowMs(), 150);
});

test("TestClock fires a timer exactly when its due time is reached, not before", () => {
  const clock = new TestClock();
  let fired = false;
  clock.setTimer(() => {
    fired = true;
  }, 300);
  clock.advance(299);
  assert.equal(fired, false);
  clock.advance(1);
  assert.equal(fired, true);
});

test("TestClock.clear() prevents a timer from firing", () => {
  const clock = new TestClock();
  let fired = false;
  const timer = clock.setTimer(() => {
    fired = true;
  }, 100);
  timer.clear();
  clock.advance(1000);
  assert.equal(fired, false);
});

test("TestClock fires multiple due timers in due-time order", () => {
  const clock = new TestClock();
  const order = [];
  clock.setTimer(() => order.push("b"), 200);
  clock.setTimer(() => order.push("a"), 100);
  clock.advance(200);
  assert.deepEqual(order, ["a", "b"]);
});

test("SystemClock reports real, non-decreasing wall-clock time", async () => {
  const clock = new SystemClock();
  const t0 = clock.nowMs();
  await new Promise((resolve) => setTimeout(resolve, 5));
  const t1 = clock.nowMs();
  assert.ok(t1 >= t0);
});
