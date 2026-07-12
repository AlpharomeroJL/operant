// Proves the F1b wire this packet builds, end to end on the TypeScript side:
// a real undo.previewed payload (the wire shape crates/recorder now
// publishes; see crates/recorder/tests/undo_journal.rs's own end-to-end
// proof on the Rust side) decodes into exactly the restoration list the
// undo screen renders, and a run nothing real has arrived for still falls
// back to ./mockJournal.ts's fixture, unchanged.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import type { UndoJournalItemWire } from "../bus/types.ts";
import { createUndoScreen } from "./state.ts";
import { journalForRun as fixtureJournalForRun, previewLine, type UndoJournalEntry } from "./mockJournal.ts";
import { createRealJournalSource, decodeJournalItems } from "./realJournal.ts";

const SAMPLE_WIRE_ITEMS: UndoJournalItemWire[] = [
  { seq: 4, op: "restore_clipboard", had_prior: false },
  { seq: 3, op: "irreversible", description: "posted the update to #general" },
  { seq: 2, op: "reverse_move", moved_to: "Reports/2026/q2.xlsx", original: "q2.xlsx" },
  { seq: 1, op: "recreate_deleted", path: "budget.csv" },
];

test("decodeJournalItems: every wire op decodes to the matching UndoInverse, field for field", () => {
  const wire: UndoJournalItemWire[] = [
    { seq: 6, op: "delete_created", path: "new.txt" },
    { seq: 5, op: "recreate_deleted", path: "gone.txt" },
    { seq: 4, op: "reverse_move", moved_to: "b/there.txt", original: "here.txt" },
    { seq: 3, op: "restore_overwritten", path: "changed.txt" },
    { seq: 2, op: "restore_clipboard", had_prior: true },
    { seq: 1, op: "irreversible", description: "sent an email" },
  ];
  const decoded = decodeJournalItems(wire);
  const expected: UndoJournalEntry[] = [
    { seq: 6, inverse: { op: "delete_created", path: "new.txt" } },
    { seq: 5, inverse: { op: "recreate_deleted", path: "gone.txt" } },
    { seq: 4, inverse: { op: "reverse_move", movedTo: "b/there.txt", original: "here.txt" } },
    { seq: 3, inverse: { op: "restore_overwritten", path: "changed.txt" } },
    { seq: 2, inverse: { op: "restore_clipboard", hadPrior: true } },
    { seq: 1, inverse: { op: "irreversible", description: "sent an email" } },
  ];
  assert.deepEqual(decoded, expected);

  // Every kind mockJournal.ts's own fixture-coverage test holds itself to.
  const kinds = new Set(decoded.map((e) => e.inverse.op));
  assert.deepEqual(
    kinds,
    new Set(["delete_created", "recreate_deleted", "reverse_move", "restore_overwritten", "restore_clipboard", "irreversible"]),
  );
});

test("createRealJournalSource: remembers a run's items published on undo.previewed, keyed by run id", () => {
  const bus = createMockBusClient();
  const real = createRealJournalSource(bus);

  assert.equal(real.journalForRun("run-real"), undefined, "nothing remembered before anything is published");

  bus.publish("undo.previewed", { run_id: "run-real", entries: 4, irreversible: 1, items: SAMPLE_WIRE_ITEMS });

  const remembered = real.journalForRun("run-real");
  assert.ok(remembered);
  assert.deepEqual(remembered, decodeJournalItems(SAMPLE_WIRE_ITEMS));
  // A different run id is unaffected.
  assert.equal(real.journalForRun("some-other-run"), undefined);

  real.dispose();
});

test("createRealJournalSource: a payload with no items (or an empty items array) is never remembered", () => {
  const bus = createMockBusClient();
  const real = createRealJournalSource(bus);

  // Shape ./state.ts's own open() actually publishes today: no items key.
  bus.publish("undo.previewed", { run_id: "run-a", entries: 6, irreversible: 1 });
  assert.equal(real.journalForRun("run-a"), undefined);

  bus.publish("undo.previewed", { run_id: "run-b", entries: 0, irreversible: 0, items: [] });
  assert.equal(real.journalForRun("run-b"), undefined);

  real.dispose();
});

test("createRealJournalSource: dispose stops listening and forgets what was remembered", () => {
  const bus = createMockBusClient();
  const real = createRealJournalSource(bus);
  bus.publish("undo.previewed", { run_id: "run-real", entries: 4, irreversible: 1, items: SAMPLE_WIRE_ITEMS });
  assert.ok(real.journalForRun("run-real"));

  real.dispose();
  assert.equal(real.journalForRun("run-real"), undefined, "dispose must forget what was remembered");

  bus.publish("undo.previewed", { run_id: "run-later", entries: 1, irreversible: 0, items: SAMPLE_WIRE_ITEMS });
  assert.equal(real.journalForRun("run-later"), undefined, "dispose must stop listening for further events too");
});

test("end to end: the undo screen renders the REAL per-item text for a run with a published journal, and still falls back to the fixture for one without", () => {
  const bus = createMockBusClient();
  const real = createRealJournalSource(bus);
  // The exact composition ui/src/main.ts wires: real data first, fixture as
  // the test fallback (packet DELIVER), same as before this packet for any
  // run nothing real was ever published for.
  const screen = createUndoScreen(bus, {
    journalForRun: (runId) => real.journalForRun(runId) ?? fixtureJournalForRun(runId),
  });

  // A real journal arrives for run-real (what crates/recorder's
  // Recorder::publish_undo_preview publishes, once a transport carries it
  // here) before the screen opens it.
  bus.publish("undo.previewed", { run_id: "run-real", entries: 4, irreversible: 1, items: SAMPLE_WIRE_ITEMS });

  screen.open("run-real");
  const realSnap = screen.getSnapshot();
  assert.deepEqual(
    realSnap.items.map((i) => i.text),
    [
      "Would clear the clipboard (it was empty before the run)",
      "Cannot be undone: posted the update to #general",
      "Would move Reports/2026/q2.xlsx back to q2.xlsx",
      "Would recreate the deleted file from its saved copy: budget.csv",
    ],
    "real per-item content, not the demo fixture's wording",
  );
  assert.notEqual(realSnap.items[3].text, previewLine({ op: "recreate_deleted", path: "old_notes.txt" }), "must not be the fixture's own old_notes.txt line");

  // A run nothing real was ever published for still falls back to the
  // fixture, unchanged: "keep the fixture as a test fallback."
  screen.open("run-nothing-real-yet");
  const fallbackSnap = screen.getSnapshot();
  assert.deepEqual(
    fallbackSnap.items.map((i) => i.text),
    [
      "Would restore the previous clipboard contents",
      "Cannot be undone: sent the invoice email to boss@example.com",
      "Would delete the file the run created: receipt.txt",
      "Would restore the previous contents of invoice.txt",
      "Would move Archive/draft.txt back to draft.txt",
      "Would recreate the deleted file from its saved copy: old_notes.txt",
    ],
    "no real journal for this run: identical to the pre-F1b fixture-only behavior",
  );

  screen.dispose();
  real.dispose();
});
