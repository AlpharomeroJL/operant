import { test } from "node:test";
import assert from "node:assert/strict";
import { assignGlyph, GLYPH_HUE_COUNT } from "./glyph.ts";

test("assignGlyph is deterministic: the same name always yields the same hue and letter", () => {
  const a = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet");
  const b = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet");
  assert.deepEqual(a, b);
});

test("assignGlyph is a pure function of the name: it does not depend on call order or other names hashed before it", () => {
  assignGlyph("weekly-report-email", "Email the weekly report");
  const first = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet");
  assignGlyph("backup-photos", "Back up this month's photos");
  assignGlyph("another-workflow", "Another one");
  const second = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet");
  assert.deepEqual(first, second);
});

test("assignGlyph spreads distinct names across the hue ramp (not all collapsing to one hue)", () => {
  const names = ["copy-invoice-total", "weekly-report-email", "backup-photos", "sort-downloads", "rename-screenshots", "clean-inbox"];
  const hues = new Set(names.map((n) => assignGlyph(n, n).hueIndex));
  assert.ok(hues.size > 1, "at least two distinct names must land on different hues");
});

test("assignGlyph's hueIndex always stays within the fixed 12-hue ramp", () => {
  for (const name of ["a", "zzzzzzzzzzzzzzzzzzzz", "Copy the invoice total into the spreadsheet", "", "123456"]) {
    const glyph = assignGlyph(name, name);
    assert.ok(glyph.hueIndex >= 0 && glyph.hueIndex < GLYPH_HUE_COUNT, `hueIndex out of range for ${JSON.stringify(name)}`);
    assert.equal(glyph.hueRotationDeg, glyph.hueIndex * 30);
  }
});

test("assignGlyph's hue is overridable: an explicit hueIndex always wins over the computed default, design.md's 'overridable'", () => {
  const withoutOverride = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet");
  const overridden = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet", 5);
  assert.equal(overridden.hueIndex, 5);
  assert.equal(overridden.hueRotationDeg, 150);
  // Sanity: the override actually changed something relative to the un-overridden call, for at least
  // one name whose computed default does not itself happen to land on 5 (copy-invoice-total's own
  // result is not asserted here, to avoid coupling this test to the hash function's exact output;
  // guard against a coincidental match instead).
  if (withoutOverride.hueIndex === 5) {
    const differentOverride = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet", 6);
    assert.equal(differentOverride.hueIndex, 6);
  }
});

test("assignGlyph wraps an out-of-range override into the fixed ramp instead of throwing or producing a negative index", () => {
  assert.equal(assignGlyph("x", "x", 12).hueIndex, 0);
  assert.equal(assignGlyph("x", "x", 13).hueIndex, 1);
  assert.equal(assignGlyph("x", "x", -1).hueIndex, 11);
});

test("assignGlyph's letter is the first letter of the title, uppercased", () => {
  assert.equal(assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet").letter, "C");
  assert.equal(assignGlyph("weekly-report-email", "email the weekly report").letter, "E");
});

test("assignGlyph falls back to the workflow name when the title is blank, and to '?' when neither has a letter or digit", () => {
  assert.equal(assignGlyph("zebra-task", "").letter, "Z");
  assert.equal(assignGlyph("", "").letter, "?");
  assert.equal(assignGlyph("---", "!!!").letter, "?");
});
