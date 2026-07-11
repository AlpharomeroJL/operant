// docs/specs/voice.md: "a CI test asserts the voice sidecar opens zero
// network sockets." Runs the exact same round trip roundtrip.test.js proves
// correct, wrapped in a guard that throws on any attempt to open a net,
// dgram, or tls socket.

import test from "node:test";
import assert from "node:assert/strict";

import { installNetworkGuard } from "../testlib/networkGuard.js";
import { runTextModeRoundTrip } from "../testlib/roundTrip.js";

test("full text-mode round trip opens zero network sockets", async () => {
  const guard = installNetworkGuard();
  let result;
  try {
    result = await runTextModeRoundTrip();
  } finally {
    guard.restore();
  }
  assert.ok(result.intent.text.length > 0, "sanity: the round trip actually ran");
  assert.equal(guard.count(), 0, `expected zero network attempts, saw: ${guard.calls.join(", ")}`);
});
