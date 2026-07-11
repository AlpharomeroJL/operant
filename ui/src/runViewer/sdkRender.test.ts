// Proves the defensive fallback around @operant/sdk's renderStep, which is
// documented as total over every real Action IR kind
// (sdk/ts/test/render-totality.test.js). The only way to exercise the
// fallback branch is to hand it a kind the renderer refuses; the mocked bus
// in this shell never actually produces one itself (ui/src/bus/mockClient.ts
// only ever sends real kinds), so this is deliberately an edge-case test,
// not a re-test of the renderer's own behavior.

import { test } from "node:test";
import assert from "node:assert/strict";
import { renderStepSentence } from "./sdkRender.ts";

test("renders a real step through the plain-English renderer", () => {
  assert.equal(renderStepSentence({ kind: "key", params: { combo: "ctrl+s" } }, "fallback"), "Save the file");
});

test("falls back to the given text for a step the renderer refuses", () => {
  assert.equal(renderStepSentence({ kind: "not-a-real-kind" }, "Step 1"), "Step 1");
});
