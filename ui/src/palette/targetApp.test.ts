// Pure-logic tests for the target-app picker controller (./targetApp.ts): the
// state machine (closed / loading / list), the pre-selected front-app default,
// arrow-key-style moveSelection, and what confirm hands back. DOM wiring is
// covered end to end in ./targetAppAccessibility.test.ts (axe + real
// KeyboardEvents), the same logic/DOM split ./state.test.ts and
// ./accessibility.test.ts use for the palette itself.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createTargetAppPicker, FRONT_APP_ROW_ID, type TargetWindow } from "./targetApp.ts";
import { getTargetAppStrings } from "./strings.ts";

// Topmost-first, Operant already excluded (what list_windows returns): windows[0]
// is the app the person was last in.
const WINDOWS: TargetWindow[] = [
  { process: "chrome.exe", title: "Quarterly report - Chrome", id: "win-1" },
  { process: "notepad.exe", title: "notes.txt - Notepad", id: "win-2" },
  { process: "excel.exe", title: "Budget - Excel", id: "win-3" },
];

test("starts closed, not loading, with nothing selected", () => {
  const picker = createTargetAppPicker();
  const snap = picker.getSnapshot();
  assert.equal(snap.open, false);
  assert.equal(snap.loading, false);
  assert.equal(snap.selectedId, null);
  assert.deepEqual(snap.rows, []);
});

test("open(goal) enters the loading state, carrying the goal, until windows arrive", () => {
  const picker = createTargetAppPicker();
  picker.open("copy the invoice total");
  const snap = picker.getSnapshot();
  assert.equal(snap.open, true);
  assert.equal(snap.loading, true, "open with no windows yet must report loading");
  assert.equal(snap.goal, "copy the invoice total");
  assert.deepEqual(snap.rows, [], "no rows until list_windows resolves");
});

test("setWindows builds a front-app row plus one row per window, and clears loading", () => {
  const picker = createTargetAppPicker();
  picker.open("do a thing");
  picker.setWindows(WINDOWS);
  const snap = picker.getSnapshot();

  assert.equal(snap.loading, false);
  assert.equal(snap.rows.length, WINDOWS.length + 1, "a front-app row plus every window");

  const [front, ...rest] = snap.rows;
  assert.equal(front.id, FRONT_APP_ROW_ID);
  assert.equal(front.frontApp, true);
  assert.equal(front.title, getTargetAppStrings().frontApp);
  assert.equal(front.subtitle, WINDOWS[0].title, "the front-app row names which app it resolves to");
  assert.equal(front.process, WINDOWS[0].process, "the front-app row resolves to windows[0]");

  assert.deepEqual(
    rest.map((r) => ({ id: r.id, process: r.process, subtitle: r.subtitle, frontApp: r.frontApp })),
    WINDOWS.map((w) => ({ id: w.id, process: w.process, subtitle: w.process, frontApp: false })),
  );
});

test("the default selection is the front-app row, which resolves to the topmost window (windows[0])", () => {
  const picker = createTargetAppPicker();
  picker.open("do a thing");
  picker.setWindows(WINDOWS);
  const snap = picker.getSnapshot();

  assert.equal(snap.selectedId, FRONT_APP_ROW_ID, "the front-app row must be pre-selected");
  const selected = snap.rows.find((r) => r.id === snap.selectedId);
  assert.equal(selected?.process, WINDOWS[0].process, "confirming the default targets the app the person was last in");
});

test("moveSelection walks the flat row list and wraps at both ends", () => {
  const picker = createTargetAppPicker();
  picker.open("do a thing");
  picker.setWindows(WINDOWS);
  const ids = picker.getSnapshot().rows.map((r) => r.id);

  picker.moveSelection(1);
  assert.equal(picker.getSnapshot().selectedId, ids[1], "down from the front-app row selects the first window");
  picker.moveSelection(-1);
  assert.equal(picker.getSnapshot().selectedId, ids[0]);
  picker.moveSelection(-1);
  assert.equal(picker.getSnapshot().selectedId, ids[ids.length - 1], "up from the top wraps to the last row");
});

test("confirm() with the default selection hands back the goal and the topmost window's process, and closes", () => {
  const picker = createTargetAppPicker();
  picker.open("copy the invoice total");
  picker.setWindows(WINDOWS);

  const result = picker.confirm();
  assert.deepEqual(result, { goal: "copy the invoice total", windowProcess: WINDOWS[0].process });
  assert.equal(picker.getSnapshot().open, false, "confirming must close the picker");
});

test("confirm(rowId) targets an explicitly clicked window, not the current selection", () => {
  const picker = createTargetAppPicker();
  picker.open("copy the invoice total");
  picker.setWindows(WINDOWS);

  const result = picker.confirm("win-2");
  assert.equal(result?.windowProcess, "notepad.exe");
});

test("a keyboard move then confirm targets the moved-to window's process", () => {
  const picker = createTargetAppPicker();
  picker.open("copy the invoice total");
  picker.setWindows(WINDOWS);

  picker.moveSelection(1); // front-app -> win-1
  picker.moveSelection(1); // win-1 -> win-2
  const result = picker.confirm();
  assert.equal(result?.windowProcess, "notepad.exe");
});

test("an empty window list shows the empty state and confirms nothing", () => {
  const picker = createTargetAppPicker();
  picker.open("do a thing");
  picker.setWindows([]);
  const snap = picker.getSnapshot();

  assert.equal(snap.loading, false);
  assert.deepEqual(snap.rows, []);
  assert.equal(snap.selectedId, null);
  assert.equal(picker.confirm(), null, "there is nothing to confirm");
  assert.equal(picker.getSnapshot().open, true, "a no-op confirm must not close the picker");
});

test("close() closes the picker and drops any fetched windows", () => {
  const picker = createTargetAppPicker();
  picker.open("do a thing");
  picker.setWindows(WINDOWS);
  picker.close();
  const snap = picker.getSnapshot();
  assert.equal(snap.open, false);
  assert.deepEqual(snap.rows, [], "reopening later must start from a fresh loading state, not stale windows");
});

test("subscribe fires on open/setWindows/moveSelection/confirm and stops after unsubscribe", () => {
  const picker = createTargetAppPicker();
  let notifications = 0;
  const unsubscribe = picker.subscribe(() => notifications++);

  picker.open("do a thing");
  picker.setWindows(WINDOWS);
  picker.moveSelection(1);
  picker.confirm();
  assert.equal(notifications, 4);

  unsubscribe();
  picker.open("again");
  assert.equal(notifications, 4, "no further notifications after unsubscribe");
});
