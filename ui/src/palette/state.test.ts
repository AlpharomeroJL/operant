// Tests the palette controller's state machine: open/close, typed-query
// selection reset, arrow-key-style moveSelection (BAR: "keyboard-only
// operation (arrow keys + Enter...)" at the logic layer; ./accessibility.test.ts
// covers the same thing end to end through real DOM KeyboardEvents), commit
// intents, and frecency recording on commit.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createPaletteController } from "./state.ts";
import { createFrecencyStore } from "./frecency.ts";
import type { PaletteEntry } from "./catalog.ts";

const ENTRIES: PaletteEntry[] = [
  { id: "wf-copy-invoice", kind: "workflow", title: "Copy the invoice total into the spreadsheet" },
  { id: "wf-weekly-report", kind: "workflow", title: "Email the weekly report" },
  { id: "action.nav.library", kind: "action", title: "Library" },
  { id: "setting.privacy", kind: "setting", title: "Privacy" },
];

function makeController(overrides: { now?: () => number } = {}) {
  const frecency = createFrecencyStore({ now: overrides.now, storageKey: `test.state.${Math.random()}` });
  const controller = createPaletteController({ frecency, now: overrides.now });
  controller.setEntries(ENTRIES);
  return { controller, frecency };
}

test("starts closed, with a blank query and nothing selected", () => {
  const { controller } = makeController();
  const snap = controller.getSnapshot();
  assert.equal(snap.open, false);
  assert.equal(snap.query, "");
  assert.equal(snap.selectedId, null);
});

test("open() opens the palette and resets query and selection to the top of the root view", () => {
  const { controller } = makeController();
  controller.setQuery("invoice");
  controller.open();
  const snap = controller.getSnapshot();
  assert.equal(snap.open, true);
  assert.equal(snap.query, "", "opening must clear any leftover query from a previous session");
  assert.equal(snap.selectedId, snap.rows[0]?.id, "selection must default to the top row");
});

test("close() closes the palette and clears the query", () => {
  const { controller } = makeController();
  controller.open();
  controller.setQuery("invoice");
  controller.close();
  const snap = controller.getSnapshot();
  assert.equal(snap.open, false);
  assert.equal(snap.query, "");
});

test("setQuery narrows the rows and resets selection to the new top result", () => {
  const { controller } = makeController();
  controller.open();
  controller.setQuery("invoice");
  const snap = controller.getSnapshot();
  assert.deepEqual(
    snap.rows.map((r) => r.id),
    ["wf-copy-invoice"],
  );
  assert.equal(snap.selectedId, "wf-copy-invoice");
});

test("moveSelection: ArrowDown steps forward through the flat row list and wraps past the end", () => {
  const { controller } = makeController();
  controller.open();
  const rows = controller.getSnapshot().rows.map((r) => r.id);
  assert.ok(rows.length >= 3, "fixture must have enough rows for this test to mean anything");

  assert.equal(controller.getSnapshot().selectedId, rows[0]);
  controller.moveSelection(1);
  assert.equal(controller.getSnapshot().selectedId, rows[1]);
  controller.moveSelection(1);
  assert.equal(controller.getSnapshot().selectedId, rows[2]);

  // Wrap: stepping forward past the last row lands back on the first.
  for (let i = 3; i < rows.length; i++) controller.moveSelection(1);
  controller.moveSelection(1);
  assert.equal(controller.getSnapshot().selectedId, rows[0], "ArrowDown past the last row must wrap to the first");
});

test("moveSelection: ArrowUp from the top row wraps to the last row", () => {
  const { controller } = makeController();
  controller.open();
  const rows = controller.getSnapshot().rows.map((r) => r.id);
  controller.moveSelection(-1);
  assert.equal(controller.getSnapshot().selectedId, rows[rows.length - 1], "ArrowUp before the first row must wrap to the last");
});

test("commit('run') on a workflow row returns it, records frecency, and closes the palette", () => {
  const { controller, frecency } = makeController();
  controller.open();
  controller.setQuery("invoice");

  const commit = controller.commit("run");
  assert.ok(commit);
  assert.equal(commit!.intent, "run");
  assert.equal(commit!.row.id, "wf-copy-invoice");
  assert.equal(frecency.countOf("wf-copy-invoice"), 1, "committing must record a pick for frecency");
  assert.equal(controller.getSnapshot().open, false, "committing must close the palette");
});

test("commit('preview') and commit('details') work for a workflow row", () => {
  const { controller } = makeController();
  controller.open();
  controller.setQuery("invoice");

  const preview = controller.commit("preview");
  assert.equal(preview?.intent, "preview");
  assert.equal(preview?.row.id, "wf-copy-invoice");

  controller.open();
  controller.setQuery("invoice");
  const details = controller.commit("details");
  assert.equal(details?.intent, "details");
  assert.equal(details?.row.id, "wf-copy-invoice");
});

test("commit('preview') and commit('details') return null for a non-workflow row: nothing to preview or explain", () => {
  const { controller } = makeController();
  controller.open();
  controller.setQuery("library");
  assert.equal(controller.getSnapshot().rows[0]?.id, "action.nav.library");

  assert.equal(controller.commit("preview"), null);
  assert.equal(controller.getSnapshot().open, true, "a null commit must not close the palette");

  assert.equal(controller.commit("details"), null);
});

test("commit('run') on an unmatched query commits the Teach this row, without recording frecency (it has no stable id)", () => {
  const { controller, frecency } = makeController();
  controller.open();
  controller.setQuery("a whole sentence nothing will ever match");

  const commit = controller.commit("run");
  assert.ok(commit);
  assert.equal(commit!.row.kind, "teach");
  assert.equal(commit!.row.subtitle, "a whole sentence nothing will ever match");
  assert.deepEqual(frecency.all(), [], "a teach row must never be recorded into frecency");
});

test("commit('preview') and commit('details') return null on the Teach this row", () => {
  const { controller } = makeController();
  controller.open();
  controller.setQuery("a whole sentence nothing will ever match");
  assert.equal(controller.commit("preview"), null);
  assert.equal(controller.commit("details"), null);
});

test("commit() returns null when the palette has no rows at all (blank query, empty catalog)", () => {
  const controller = createPaletteController({ frecency: createFrecencyStore({ storageKey: `test.state.${Math.random()}` }) });
  controller.open();
  assert.equal(controller.getSnapshot().rows.length, 0);
  assert.equal(controller.commit("run"), null);
});

test("setEntries live-refreshes the catalog (a newly taught workflow appears without recreating the controller)", () => {
  const { controller } = makeController();
  controller.open();
  controller.setQuery("brand new");
  // Nothing in the original fixture matches yet, so this is the Teach this fallback, not an empty list.
  assert.equal(controller.getSnapshot().teachRow?.kind, "teach");

  controller.setEntries([...ENTRIES, { id: "wf-brand-new", kind: "workflow", title: "Brand new workflow" }]);
  const snap = controller.getSnapshot();
  assert.deepEqual(
    snap.rows.map((r) => r.id),
    ["wf-brand-new"],
  );
  assert.equal(snap.teachRow, null, "a real match must replace the Teach this fallback");
});

test("subscribe fires on open/close/setQuery/moveSelection/commit, and stops after unsubscribe", () => {
  const { controller } = makeController();
  let notifications = 0;
  const unsubscribe = controller.subscribe(() => notifications++);

  controller.open();
  controller.setQuery("invoice");
  controller.moveSelection(1);
  controller.commit("run");
  assert.equal(notifications, 4);

  unsubscribe();
  controller.open();
  assert.equal(notifications, 4, "no further notifications after unsubscribe");
});

test("dispose() clears listeners: no further notifications reach them even if the controller keeps being driven", () => {
  const { controller } = makeController();
  let notifications = 0;
  controller.subscribe(() => notifications++);
  controller.dispose();

  controller.open();
  controller.setQuery("invoice");
  assert.equal(notifications, 0, "a disposed controller's old listeners must never fire again");
});

test("commit() notifies subscribers exactly once, even though it both records frecency and closes the palette", () => {
  const { controller } = makeController();
  controller.open();
  controller.setQuery("invoice");
  let notifications = 0;
  controller.subscribe(() => notifications++);

  controller.commit("run");
  assert.equal(notifications, 1, "one commit must produce exactly one notification, not one per internal side effect");
});
