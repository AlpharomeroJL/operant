// Tests for the redacted filmstrip thumbnails (docs/specs/design.md section
// 3). The point being verified is design.md's: a thumbnail is a generated
// placeholder derived only from the step id, so it is stable across
// re-renders and can never carry captured pixels. No DOM here; the view test
// (./view.test.ts) covers that these bars actually render as a redacted
// thumbnail with no image element.

import { test } from "node:test";
import assert from "node:assert/strict";
import { redactionBars, BAR_MIN_PCT, BAR_MAX_PCT } from "./thumbnails.ts";

test("redactionBars is deterministic for a given step id", () => {
  assert.deepEqual(redactionBars("s1"), redactionBars("s1"));
  assert.deepEqual(redactionBars("demo-42"), redactionBars("demo-42"));
});

test("redactionBars returns the requested count, every bar within the redaction range", () => {
  const bars = redactionBars("step-xyz", 3);
  assert.equal(bars.length, 3);
  for (const width of bars) {
    assert.ok(width >= BAR_MIN_PCT && width <= BAR_MAX_PCT, `bar width ${width} must land in [${BAR_MIN_PCT}, ${BAR_MAX_PCT}]`);
  }
  assert.equal(redactionBars("step-xyz", 5).length, 5);
});

test("different step ids generally draw different patterns", () => {
  // Not a guarantee for every possible pair, but the hash must not collapse
  // every step to one identical placeholder.
  const patterns = new Set(["a", "b", "c", "s1", "s2"].map((id) => redactionBars(id).join(",")));
  assert.ok(patterns.size > 1, "the placeholder pattern must vary by step id");
});
