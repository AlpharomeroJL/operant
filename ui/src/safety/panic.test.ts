// The kill switch's two-path panic (SAFETY, never-cut). Proves the inversion
// A5 asked for: the panic path invokes BOTH stop commands the contract defines
// (contracts/ipc.md section 5b) and the halted UI state follows from the core's
// echoed killswitch.engaged, not the cosmetic self-publish that "rendered but
// did not stop."

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY } from "../bus/types.ts";
import { createTray } from "../tray/state.ts";
import { createDomEnv } from "../styles/testDomEnv.ts";
import {
  enginePanic,
  createBusPanicClient,
  releaseHeldModifiers,
  type PanicClient,
} from "./panic.ts";

/** A PanicClient that records the order of its calls instead of touching a bus. */
function recordingClient(log: string[]): PanicClient {
  return {
    stop: (runId?: string) => log.push(runId ? `stop:${runId}` : "stop"),
    kill: () => log.push("kill"),
  };
}

test("the panic path fires BOTH independent stops, the cooperative stop then the backstop kill (contracts/ipc.md section 5b)", () => {
  const log: string[] = [];
  enginePanic(recordingClient(log));
  assert.deepEqual(log, ["stop", "kill"], "both commands fire, kill last as the guaranteed final word");
});

test("the panic path releases held modifiers before either stop, so a chord modifier cannot ride along", () => {
  const log: string[] = [];
  enginePanic(recordingClient(log), { releaseModifiers: () => log.push("release") });
  assert.deepEqual(log, ["release", "stop", "kill"]);
});

test("the backstop kill still fires when the cooperative stop throws (never-cut: a wedged stop cannot skip the kill)", () => {
  let killed = false;
  const client: PanicClient = {
    stop: () => {
      throw new Error("cooperative stop is wedged");
    },
    kill: () => {
      killed = true;
    },
  };
  assert.doesNotThrow(() => enginePanic(client));
  assert.equal(killed, true, "the hard-terminate backstop is reached even when the cooperative path throws");
});

test("a modifier-reset hiccup cannot delay the stop paths either", () => {
  const log: string[] = [];
  enginePanic(recordingClient(log), {
    releaseModifiers: () => {
      throw new Error("DOM hiccup");
    },
  });
  assert.deepEqual(log, ["stop", "kill"], "both stops still fire when the modifier reset throws");
});

test("the tracked run id is handed to the cooperative stop; omitted when a trigger has none in hand", () => {
  const calls: (string | undefined)[] = [];
  const client: PanicClient = { stop: (runId) => calls.push(runId), kill: () => {} };
  enginePanic(client, { runId: "run-7" });
  enginePanic(client, {});
  assert.deepEqual(calls, ["run-7", undefined]);
});

test("releaseHeldModifiers is a safe no-op with no DOM (plain node --test host)", () => {
  assert.equal(typeof document, "undefined", "guards the assumption this test relies on");
  assert.doesNotThrow(() => releaseHeldModifiers());
});

test("releaseHeldModifiers dispatches a keyup for every modifier so none stays logically held in the webview", () => {
  const env = createDomEnv();
  try {
    const seen: string[] = [];
    env.document.addEventListener("keyup", (event) => seen.push((event as KeyboardEvent).key));
    releaseHeldModifiers();
    assert.deepEqual([...seen].sort(), ["Alt", "Control", "Meta", "Shift"]);
  } finally {
    env.cleanup();
  }
});

test("createBusPanicClient.kill echoes killswitch.engaged (the contract's kill echo), stamped with the current time", () => {
  const bus = createMockBusClient();
  const engaged: unknown[] = [];
  bus.subscribe("killswitch.engaged", (event) => engaged.push(event.payload));

  createBusPanicClient(bus, () => 4242).kill();

  assert.deepEqual(engaged, [{ at_ms: 4242 }]);
});

test("createBusPanicClient.stop closes the given run cooperatively (run.halted, reason human); a no-op with no run in hand", () => {
  const bus = createMockBusClient();
  const halted: unknown[] = [];
  bus.subscribe("run.halted", (event) => halted.push(event.payload));

  const client = createBusPanicClient(bus);
  client.stop("run-9");
  client.stop();

  assert.deepEqual(halted, [{ run_id: "run-9", reason: "human" }], "only the run-bearing stop publishes; the empty one is a no-op");
});

test("end to end: the halted UI state follows from the echoed killswitch.engaged, not a self-publish", () => {
  const bus = createMockBusClient();
  const tray = createTray(bus, { now: () => 777 });
  bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  assert.equal(tray.getSnapshot().glyph, "replaying");

  // The panic path over the mock core: stop closes r1, kill echoes the
  // killswitch. The tray's own bus subscription paints the halted state off
  // those echoes, the same way it would off a real core.
  enginePanic(createBusPanicClient(bus, () => 777), { runId: "r1" });

  const snap = tray.getSnapshot();
  assert.equal(snap.glyph, "kill", "the tray reflects halted off the echoed events");
  assert.equal(snap.canPauseAll, false, "the run is gone: nothing left to pause");
  assert.equal(snap.notifications.at(-1)?.title, "Emergency stop engaged", "the killswitch echo is the authoritative last notification");
});
