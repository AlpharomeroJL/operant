// BAR: "fuzzy match (subsequence + word-boundary bonus) ranking."

import { test } from "node:test";
import assert from "node:assert/strict";
import { fuzzyMatch, highlightSegments } from "./fuzzy.ts";

test("fuzzyMatch: matches a plain subsequence, case-insensitively", () => {
  const m = fuzzyMatch("cit", "copy-invoice-total");
  assert.ok(m, "c, i, t must be found in order");
  assert.deepEqual(
    m!.indices.map((i) => "copy-invoice-total"[i]),
    ["c", "i", "t"],
  );

  const upper = fuzzyMatch("CIT", "copy-invoice-total");
  assert.ok(upper, "matching must ignore case on the query side");
  assert.equal(upper!.score, m!.score, "case must not change the score");
});

test("fuzzyMatch: returns null when the query is not a subsequence at all", () => {
  assert.equal(fuzzyMatch("xyz", "copy-invoice-total"), null);
  assert.equal(fuzzyMatch("total copy", "copy-invoice-total"), null, "out-of-order letters must not match");
});

test("fuzzyMatch: an empty query matches every target trivially, with no highlighted indices", () => {
  const m = fuzzyMatch("", "anything at all");
  assert.ok(m);
  assert.equal(m!.score, 0);
  assert.deepEqual(m!.indices, []);
});

test("fuzzyMatch: an empty target only matches an empty query", () => {
  assert.equal(fuzzyMatch("a", ""), null);
  assert.ok(fuzzyMatch("", ""));
});

test("fuzzyMatch ranking: a contiguous substring match outranks the same letters scattered", () => {
  const contiguous = fuzzyMatch("inv", "Copy the invoice total");
  const scattered = fuzzyMatch("iot", "Copy the invoice total"); // i(nvoice) o(f) ... still a subsequence but not a run
  assert.ok(contiguous && scattered);
  assert.ok(contiguous!.score > scattered!.score, "an unbroken run must score higher than scattered letters");
});

test("fuzzyMatch ranking: a word-boundary (acronym-style) match outranks an equal-length mid-word match", () => {
  const acronym = fuzzyMatch("ci", "Copy Invoice"); // C(opy) I(nvoice): both letters start a word
  const midWord = fuzzyMatch("ci", "specific"); // buried mid-word, no boundary
  assert.ok(acronym && midWord);
  assert.ok(acronym!.score > midWord!.score, "matching two word-initials must outrank the same two letters mid-word");
});

test("fuzzyMatch ranking: a match starting earlier in the string outranks the same match starting later", () => {
  const early = fuzzyMatch("cat", "cat food and other things");
  const late = fuzzyMatch("cat", "other things and cat food");
  assert.ok(early && late);
  assert.ok(early!.score > late!.score, "an earlier match must score higher than an identical later one");
});

test("fuzzyMatch ranking: an exact, full-string match scores higher than a partial one for a shared prefix query", () => {
  const exact = fuzzyMatch("settings", "settings");
  const partial = fuzzyMatch("settings", "settings and more settings besides");
  assert.ok(exact && partial);
  assert.ok(exact!.score > partial!.score, "the tighter (shorter) target must win when both otherwise match the same way");
});

test("fuzzyMatch: workflow-slug style targets (hyphenated) still find word-boundary matches", () => {
  const m = fuzzyMatch("it", "copy-invoice-total");
  assert.ok(m, "i (of invoice) and t (of total) both start a hyphen-separated word");
  assert.deepEqual(
    m!.indices.map((i) => "copy-invoice-total"[i]),
    ["i", "t"],
  );
});

test("highlightSegments: no matched indices returns the whole string unmatched", () => {
  assert.deepEqual(highlightSegments("hello", []), [{ text: "hello", matched: false }]);
});

test("highlightSegments: an empty string returns no segments", () => {
  assert.deepEqual(highlightSegments("", []), []);
});

test("highlightSegments: groups matched/unmatched runs in order, covering the whole string", () => {
  // "copy-invoice-total", indices for "i" (5) and "t" (13) from the fuzzyMatch("it", ...) test above.
  const segments = highlightSegments("copy-invoice-total", [5, 13]);
  const rebuilt = segments.map((s) => s.text).join("");
  assert.equal(rebuilt, "copy-invoice-total", "segments must reconstruct the original string exactly");
  assert.deepEqual(
    segments.map((s) => s.matched),
    [false, true, false, true, false],
  );
  assert.equal(segments[1].text, "i");
  assert.equal(segments[3].text, "t");
});

test("highlightSegments: consecutive matched indices merge into one segment", () => {
  const segments = highlightSegments("cat", [0, 1, 2]);
  assert.deepEqual(segments, [{ text: "cat", matched: true }]);
});
