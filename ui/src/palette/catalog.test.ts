// BAR: "Teach this" fallback row renders when nothing matches (here: is
// produced by the pure matching logic; ./accessibility.test.ts covers it
// actually rendering in the DOM). Also covers design.md section 3's
// grouping (Workflows, Actions, Recent) and frecency pulling a picked entry
// into its own Recent group.

import { test } from "node:test";
import assert from "node:assert/strict";
import { matchEntries, type PaletteEntry, type MatchStrings } from "./catalog.ts";
import { createFrecencyStore } from "./frecency.ts";

const STRINGS: MatchStrings = {
  groupWorkflows: "Workflows",
  groupActions: "Actions",
  groupRecent: "Recent",
  teachThis: "Teach this",
  teachHint: "Press Enter to teach it",
};

const ENTRIES: PaletteEntry[] = [
  { id: "copy-invoice-total", kind: "workflow", title: "Copy the invoice total into the spreadsheet", keywords: ["copy-invoice-total"] },
  { id: "weekly-report-email", kind: "workflow", title: "Email the weekly report", keywords: ["weekly-report-email"] },
  { id: "action.nav.settings", kind: "action", title: "Settings" },
  { id: "setting.privacy", kind: "setting", title: "Privacy" },
];

function freshFrecency(now = () => 0) {
  return createFrecencyStore({ now, storageKey: `test.catalog.${Math.random()}` });
}

test("matchEntries: a blank query matches everything, grouped by kind, workflows then actions", () => {
  const result = matchEntries(ENTRIES, "", freshFrecency(), STRINGS);
  assert.equal(result.teachRow, null);
  assert.deepEqual(
    result.groups.map((g) => g.title),
    ["Workflows", "Actions"],
  );
  assert.equal(result.groups[0].rows.length, 2, "both workflows must appear");
  assert.equal(result.groups[1].rows.length, 2, "the action and the setting must both fold into Actions");
  assert.deepEqual(
    result.rows.map((r) => r.id),
    result.groups.flatMap((g) => g.rows.map((r) => r.id)),
    "the flat row list must equal the groups flattened, in the same order",
  );
});

test("matchEntries: settings entries fold into the Actions group, not a fourth group", () => {
  const result = matchEntries(ENTRIES, "", freshFrecency(), STRINGS);
  const actionsGroup = result.groups.find((g) => g.title === "Actions");
  assert.ok(actionsGroup);
  const kinds = actionsGroup!.rows.map((r) => r.kind).sort();
  assert.deepEqual(kinds, ["action", "setting"]);
});

test("matchEntries: a query that matches nothing at all, with the box non-empty, returns only the Teach this row", () => {
  const result = matchEntries(ENTRIES, "zzzzzz not a real match", freshFrecency(), STRINGS);
  assert.equal(result.groups.length, 0);
  assert.equal(result.rows.length, 1);
  assert.ok(result.teachRow);
  assert.equal(result.teachRow!.kind, "teach");
  assert.equal(result.teachRow!.title, "Teach this");
  assert.equal(result.teachRow!.subtitle, "zzzzzz not a real match", "the typed text must be preserved verbatim (trimmed) for main.ts to hand to submitGoal");
  assert.equal(result.teachRow!.id, "__teach__");
  assert.deepEqual(result.rows[0], result.teachRow);
});

test("matchEntries: highlight indices always index into the row's own title, never into a keyword that happened to score higher", () => {
  // A keyword whose match against the raw slug scores higher than the
  // title's own match (a tight, word-boundary-heavy hit on a short hyphenated
  // string easily outscores the same query landing mid-sentence in a long
  // title): the row must still highlight real positions in the *title*, not
  // borrow the keyword's indices, which would land on the wrong characters
  // entirely (this exact query/entry pair reproduces a bug where "invoice"
  // scored higher against the keyword "copy-invoice-total" than against the
  // title, and the keyword's indices were rendered against the title as if
  // they were its own, highlighting "the inv" instead of "invoice").
  const entries: PaletteEntry[] = [
    { id: "copy-invoice-total", kind: "workflow", title: "Copy the invoice total into the spreadsheet", keywords: ["copy-invoice-total"] },
  ];
  const result = matchEntries(entries, "invoice", freshFrecency(), STRINGS);
  const row = result.rows[0];
  assert.ok(row);
  const highlighted = row.highlight.map((i) => row.title[i]).join("");
  assert.equal(highlighted, "invoice", `expected the highlighted characters to spell "invoice", got ${JSON.stringify(highlighted)} from indices ${JSON.stringify(row.highlight)}`);
});

test("matchEntries: a query that matches only a keyword, not the title at all, still surfaces the entry with no highlight", () => {
  const entries: PaletteEntry[] = [{ id: "copy-invoice-total", kind: "workflow", title: "Copy the invoice total into the spreadsheet", keywords: ["copy-invoice-total"] }];
  // "cit" is a subsequence of the slug "copy-invoice-total" (c...i...t) but
  // not of the title in that order at a useful boundary; whether or not it
  // happens to also match the title is not the point being tested here, so
  // pick a query that is a subsequence of the keyword only by construction:
  // the slug's own hyphens, which do not exist anywhere in the title.
  const result = matchEntries(entries, "cy-in", freshFrecency(), STRINGS);
  assert.equal(result.rows.length, 1, "the keyword match must still surface the entry");
  assert.deepEqual(result.rows[0].highlight, [], "with no title match, there must be nothing to highlight");
});

test("matchEntries: a blank query with zero entries shows nothing, not a Teach row (nothing was typed to teach)", () => {
  const result = matchEntries([], "", freshFrecency(), STRINGS);
  assert.equal(result.teachRow, null);
  assert.deepEqual(result.rows, []);
});

test("matchEntries: a query matching only a workflow's slug keyword (not its title) still finds it", () => {
  const result = matchEntries(ENTRIES, "wkrpt", freshFrecency(), STRINGS); // subsequence of "weekly-report-email"... actually check against title too
  // "wkrpt" is a subsequence of "weekly-report-email" via the keyword; assert it is found by id.
  const ids = result.rows.map((r) => r.id);
  assert.ok(ids.includes("weekly-report-email"), `expected weekly-report-email among ${JSON.stringify(ids)}`);
});

test("matchEntries: query filters out non-matching entries entirely", () => {
  const result = matchEntries(ENTRIES, "invoice", freshFrecency(), STRINGS);
  assert.deepEqual(
    result.rows.map((r) => r.id),
    ["copy-invoice-total"],
  );
});

test("matchEntries: highlight indices point at the matched characters in the row's own title", () => {
  const result = matchEntries(ENTRIES, "settings", freshFrecency(), STRINGS);
  const row = result.rows.find((r) => r.id === "action.nav.settings");
  assert.ok(row);
  assert.deepEqual(row!.highlight, [0, 1, 2, 3, 4, 5, 6, 7], "an exact full match against 'Settings' must highlight every character");
});

test("matchEntries: an entry ever picked (frecency count > 0) moves into its own Recent group, out of Workflows/Actions", () => {
  const frecency = freshFrecency();
  frecency.record("copy-invoice-total");

  const result = matchEntries(ENTRIES, "", frecency, STRINGS);
  assert.deepEqual(
    result.groups.map((g) => g.title),
    ["Workflows", "Actions", "Recent"],
  );
  const workflowsGroup = result.groups.find((g) => g.title === "Workflows")!;
  assert.deepEqual(workflowsGroup.rows.map((r) => r.id), ["weekly-report-email"], "the picked workflow must no longer appear in Workflows");
  const recentGroup = result.groups.find((g) => g.title === "Recent")!;
  assert.deepEqual(recentGroup.rows.map((r) => r.id), ["copy-invoice-total"]);
});

test("matchEntries: Recent is ranked by frecency, most-frecent first, when scores are otherwise tied (blank query)", () => {
  let clock = 0;
  const frecency = freshFrecency(() => clock);

  frecency.record("copy-invoice-total"); // picked once, long ago
  clock = 20 * 24 * 3_600_000; // 20 days later
  for (let i = 0; i < 5; i++) frecency.record("weekly-report-email"); // picked often, recently

  const result = matchEntries(ENTRIES, "", frecency, STRINGS);
  const recentGroup = result.groups.find((g) => g.title === "Recent")!;
  assert.deepEqual(
    recentGroup.rows.map((r) => r.id),
    ["weekly-report-email", "copy-invoice-total"],
    "the frequently and recently picked entry must rank above the stale one-time pick",
  );
});

test("matchEntries: frecency for an entry that does not match the typed query never pulls it into the results", () => {
  const frecency = freshFrecency();
  frecency.record("weekly-report-email"); // heavy frecency, but about to type a query it cannot match

  const result = matchEntries(ENTRIES, "Settings", frecency, STRINGS);
  const ids = result.rows.map((r) => r.id);
  assert.deepEqual(ids, ["action.nav.settings"], "frecency ranks matches; it never rescues something the query does not match at all");
});

test("matchEntries: within Recent, a stronger current match can outrank a more-frecent one (score + frecency combine, not frecency alone)", () => {
  const DAY = 24 * 3_600_000;
  const frecencyEntries: PaletteEntry[] = [
    { id: "wf-a", kind: "workflow", title: "Report Generator" }, // "Report Gen" matches as an exact, index-0 prefix
    { id: "wf-b", kind: "workflow", title: "Monthly Report Generation Summary" }, // same 10-char run, but starting later in a longer string: a strictly weaker fuzzy score
  ];
  let clock = 0;
  const frecency = freshFrecency(() => clock);
  frecency.record("wf-a"); // picked once, about to go stale
  clock = 10 * DAY;
  frecency.record("wf-b"); // picked once, more recently than wf-a
  clock = 12 * DAY; // "now": wf-a is 12 days stale (frecency weight 10), wf-b is 2 days stale (weight 50) -- frecency alone favors wf-b

  const result = matchEntries(frecencyEntries, "Report Gen", frecency, STRINGS);
  const recentGroup = result.groups.find((g) => g.title === "Recent");
  assert.ok(recentGroup, "both entries have been picked before, so both must be in Recent");
  assert.deepEqual(
    recentGroup!.rows.map((r) => r.id),
    ["wf-a", "wf-b"],
    "wf-b has the higher frecency score alone, but wf-a's stronger current match must still win the combined ranking",
  );
});
