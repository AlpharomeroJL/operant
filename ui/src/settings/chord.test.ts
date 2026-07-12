import { test } from "node:test";
import assert from "node:assert/strict";
import { chordPartsFromEvent, formatChord, isUsableChord, DEFAULT_KILL_SWITCH_CHORD, type ChordKeyEvent } from "./chord.ts";

test("the default kill switch chord matches docs/specs/guardian.md", () => {
  assert.equal(DEFAULT_KILL_SWITCH_CHORD, "Ctrl+Alt+Shift+Space");
});

test("chordPartsFromEvent captures modifiers in a fixed order plus the main key", () => {
  const parts = chordPartsFromEvent({ key: " ", ctrlKey: true, altKey: true, shiftKey: true });
  assert.deepEqual(parts, ["Ctrl", "Alt", "Shift", "Space"]);
});

test("chordPartsFromEvent uppercases a single-character key", () => {
  assert.deepEqual(chordPartsFromEvent({ key: "k", ctrlKey: true }), ["Ctrl", "K"]);
});

test("chordPartsFromEvent drops a bare modifier press (no main key yet)", () => {
  assert.deepEqual(chordPartsFromEvent({ key: "Control", ctrlKey: true }), ["Ctrl"]);
});

test("formatChord joins parts with +", () => {
  assert.equal(formatChord(["Ctrl", "Alt", "Shift", "Space"]), "Ctrl+Alt+Shift+Space");
});

test("the exact predicate ui/src/main.ts's global kill chord uses: the default chord fires, a near miss never does", () => {
  // SAFETY, never-cut: a regression that made this always-false would silently
  // disable the global panic trigger, so lock in the composed match directly.
  const matches = (event: ChordKeyEvent): boolean => formatChord(chordPartsFromEvent(event)) === DEFAULT_KILL_SWITCH_CHORD;
  assert.equal(matches({ key: " ", ctrlKey: true, altKey: true, shiftKey: true }), true, "Ctrl+Alt+Shift+Space fires the panic");
  assert.equal(matches({ key: " ", ctrlKey: true, altKey: true }), false, "a missing modifier must not fire the panic");
  assert.equal(matches({ key: "k", ctrlKey: true }), false, "the palette hotkey is not the kill chord");
});

test("isUsableChord requires at least one modifier and exactly one main key", () => {
  assert.equal(isUsableChord(["Ctrl", "Alt", "Shift", "Space"]), true);
  assert.equal(isUsableChord(["Ctrl", "K"]), true);
  assert.equal(isUsableChord(["K"]), false, "a bare key with no modifier is refused");
  assert.equal(isUsableChord(["Ctrl"]), false, "a modifier alone is not a complete chord");
  assert.equal(isUsableChord([]), false);
});
