// Scripted drive proving the tray's Quick Runs menu and the workflow
// library work end to end against the mocked bus (contracts/bus_events.md),
// per docs/specs/design.md section 3, Tray: "the top three frecent
// workflows as one-click Quick Runs." Exercises ui/src/tray and
// ui/src/library together the same way ui/src/main.ts wires them: a Quick
// Run only ever supplies a workflow name, ui/src/main.ts's requestRun (the
// same path Library's own Run button uses) is what actually starts it, so
// this scripts that same call rather than reimplementing tray-side run
// logic that does not exist (ui/src/tray/state.ts owns no bus.publish for
// starting a run at all, on purpose). No DOM (this project has no jsdom);
// DOM glue itself is intentionally untested, the same split used by every
// other module in ui/src.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY } from "../bus/types.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { createLibrary } from "../library/state.ts";
import { createTray } from "../tray/state.ts";

test("a quick run runs the saved workflow: Library.run publishes it, and the tray reflects it live", () => {
  const bus = createMockBusClient();
  // The same shared registry instance ui/src/main.ts hands both modules, so
  // the tray's Quick Runs and Library's own cards agree on titles.
  const registry = createMockRegistry();
  const library = createLibrary(bus, { registry });
  const tray = createTray(bus, { registry });

  const seenTopics: string[] = [];
  bus.subscribe("*", (event) => seenTopics.push(event.topic));

  // Nothing has ever run: no quick runs to offer yet.
  assert.deepEqual(tray.getSnapshot().quickRuns, []);

  // Run the saved workflow once, the same way Library's own Run button does
  // (ui/src/main.ts's requestRun -> library.run), so it becomes frecent.
  library.run("copy-invoice-total");
  assert.deepEqual(seenTopics, ["run.started", "run.completed"]);

  // It is now the top (only) Quick Run, titled the same as its Library card.
  const quickRuns = tray.getSnapshot().quickRuns;
  assert.equal(quickRuns.length, 1);
  assert.equal(quickRuns[0].name, "copy-invoice-total");
  assert.equal(quickRuns[0].title, "Copy the invoice total into the spreadsheet");

  // Clicking that Quick Run in ui/src/main.ts calls requestRun(quickRuns[0].name),
  // which (no capability grant needed here) is library.run again: a second
  // saved-workflow run, mode replay, never explore/AI.
  library.run(quickRuns[0].name);
  assert.deepEqual(seenTopics, ["run.started", "run.completed", "run.started", "run.completed"]);

  // Library's own bookkeeping moved too: this really is Library's Run path,
  // not a tray-private reimplementation of it.
  const card = library.getSnapshot().cards.find((c) => c.name === "copy-invoice-total");
  assert.ok(card, "the workflow must still be a library card after running it");
  assert.equal(card?.lastRunStatus, "ok");

  tray.dispose();
  library.dispose();
});

test("the tray glyph turns replaying (never recording) while a quick run is in flight", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  const tray = createTray(bus, { registry });
  const seenGlyphs: string[] = [];
  tray.subscribe((snap) => seenGlyphs.push(snap.glyph));

  // The same run.started/run.completed pair ui/src/library/state.ts's run()
  // publishes for a saved workflow (standing in for a real backend's
  // asynchronous replay): the glyph must visit "replaying" in between,
  // never "recording" (design.md section 3: replay is never AI).
  bus.publish("run.started", {
    run_id: "quickrun-1",
    goal: "Copy the invoice total into the spreadsheet",
    mode: RUN_MODE_REPLAY,
    workflow_name: "copy-invoice-total",
  });
  assert.equal(tray.getSnapshot().glyph, "replaying");

  bus.publish("run.completed", { run_id: "quickrun-1", outcome: "ok", steps: 4, wall_ms: 400 });
  assert.equal(tray.getSnapshot().glyph, "idle");

  assert.deepEqual(seenGlyphs, ["replaying", "idle"]);
  assert.ok(!seenGlyphs.includes("recording"), "a quick run must never show as recording/AI");

  tray.dispose();
});
