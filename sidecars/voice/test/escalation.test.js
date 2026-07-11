import test from "node:test";
import assert from "node:assert/strict";

import { renderEscalation } from "../src/escalation.js";
import { createMockTtsProvider } from "../src/providers/mockProvider.js";

test("escalation renders text and spoken audio when tts succeeds", async () => {
  const ttsProvider = createMockTtsProvider();
  const result = await renderEscalation(
    { run_id: "r1", sentence: "This will delete 40 files. Approve?", requires_approval: true },
    { ttsProvider }
  );
  assert.equal(result.text, "This will delete 40 files. Approve?");
  assert.ok(result.spoken);
  assert.equal(result.spoken.audio.subarray(0, 4).toString("ascii"), "RIFF");
  assert.ok(result.spoken.lengthMs > 0);
});

test("text is always present even when voice is disabled", async () => {
  const result = await renderEscalation({ sentence: "Confirm this write." }, { voiceEnabled: false });
  assert.equal(result.text, "Confirm this write.");
  assert.equal("spoken" in result, false);
});

test("text is always present even with no tts provider configured", async () => {
  const result = await renderEscalation({ sentence: "Confirm this write." });
  assert.equal(result.text, "Confirm this write.");
  assert.equal("spoken" in result, false);
});

test("voice is additive, never the only channel: a tts failure still leaves text", async () => {
  const brokenTts = {
    async tts() {
      throw new Error("model crashed");
    },
  };
  const result = await renderEscalation({ sentence: "Confirm this write." }, { ttsProvider: brokenTts });
  assert.equal(result.text, "Confirm this write.");
  assert.equal("spoken" in result, false);
});

test("escalation requires a non-empty sentence", async () => {
  await assert.rejects(() => renderEscalation({}), TypeError);
  await assert.rejects(() => renderEscalation({ sentence: "" }), TypeError);
  await assert.rejects(() => renderEscalation(null), TypeError);
});

test("step_id and requires_approval are optional and do not affect text rendering", async () => {
  const result = await renderEscalation({ sentence: "Heads up." });
  assert.equal(result.text, "Heads up.");
});
