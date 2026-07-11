// Pure-logic tests for the command palette: the hotkey matcher and the
// submit path that starts a run against the bus (contracts/bus_events.md).
// DOM wiring (focusing the input, reading paletteInput.value) lives in
// main.ts and is intentionally left untested here, the same split used by
// ui/src/bus/mockClient.test.ts and ui/src/state/mode.test.ts.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE } from "../bus/types.ts";
import { isGlobalPaletteHotkey, normalizeGoal, submitGoal } from "./palette.ts";

test("isGlobalPaletteHotkey matches Ctrl+K and Cmd+K, case-insensitively", () => {
  assert.equal(isGlobalPaletteHotkey({ key: "k", ctrlKey: true }), true);
  assert.equal(isGlobalPaletteHotkey({ key: "K", ctrlKey: true }), true);
  assert.equal(isGlobalPaletteHotkey({ key: "k", metaKey: true }), true);
});

test("isGlobalPaletteHotkey ignores plain letters and unrelated combos", () => {
  assert.equal(isGlobalPaletteHotkey({ key: "k" }), false);
  assert.equal(isGlobalPaletteHotkey({ key: "p", ctrlKey: true }), false);
  assert.equal(isGlobalPaletteHotkey({ key: "Enter", ctrlKey: true }), false);
});

test("normalizeGoal trims and rejects a blank goal", () => {
  assert.equal(normalizeGoal("  Copy the invoice total  "), "Copy the invoice total");
  assert.equal(normalizeGoal(""), null);
  assert.equal(normalizeGoal("   "), null);
});

test("submitGoal does nothing for a blank goal", () => {
  const bus = createMockBusClient();
  const events: string[] = [];
  bus.subscribe("*", (e) => events.push(e.topic));

  const stop = submitGoal(bus, "\t  \n");

  assert.equal(stop, null);
  assert.equal(events.length, 0);
});

test("submitGoal starts a run carrying the typed goal, in teach mode", () => {
  const bus = createMockBusClient();
  const started: Array<{ run_id: string; goal: string; mode: string }> = [];
  bus.subscribe("run.started", (e) => {
    if (e.topic === "run.started") started.push(e.payload);
  });

  const stop = submitGoal(bus, "  Find last month's invoices  ", { stepDelayMs: 5 });

  assert.ok(stop, "a non-blank goal should start a run");
  assert.equal(started.length, 1);
  assert.equal(started[0].goal, "Find last month's invoices");
  assert.equal(started[0].mode, RUN_MODE_EXPLORE);

  stop?.();
});
