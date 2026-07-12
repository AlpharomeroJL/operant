// Proves the B10 inversion end to end on the TypeScript side: the undo screen
// now sends the preview_undo / undo_run commands (contracts/ipc.md 5c) and
// renders the core's echoed undo.previewed / undo.applied, never a self-
// fabricated preview or a self-published applied event. ui/src/undo/
// realJournal.ts's createMockUndoCommands plays the core's part on the mock
// bus (what crates/recorder's Recorder::publish_undo_preview / undo_run emit;
// see crates/recorder/tests/undo_journal.rs for the Rust-side proof), and
// decodeJournalItems / encodeJournalItems are the two halves of the wire.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import type { BusEvent, UndoJournalItemWire } from "../bus/types.ts";
import { createUndoScreen, type UndoCommands } from "./state.ts";
import { previewLine, type UndoJournalEntry } from "./mockJournal.ts";
import { createMockUndoCommands, decodeJournalItems, encodeJournalItems } from "./realJournal.ts";

// A mixed real journal (what crates/recorder would preview): three reversible
// restorations and one irreversible step, newest-first.
const SAMPLE_WIRE_ITEMS: UndoJournalItemWire[] = [
  { seq: 4, op: "restore_clipboard", had_prior: false },
  { seq: 3, op: "irreversible", description: "posted the update to #general" },
  { seq: 2, op: "reverse_move", moved_to: "Reports/2026/q2.xlsx", original: "q2.xlsx" },
  { seq: 1, op: "recreate_deleted", path: "budget.csv" },
];

const EXPECTED_PREVIEW_TEXT = [
  "Would clear the clipboard (it was empty before the run)",
  "Cannot be undone: posted the update to #general",
  "Would move Reports/2026/q2.xlsx back to q2.xlsx",
  "Would recreate the deleted file from its saved copy: budget.csv",
];

const EXPECTED_APPLIED_TEXT = [
  "Cleared the clipboard (it was empty before the run)",
  "Cannot be undone: posted the update to #general",
  "Moved Reports/2026/q2.xlsx back to q2.xlsx",
  "Recreated the deleted file from its saved copy: budget.csv",
];

function collect(bus: ReturnType<typeof createMockBusClient>): BusEvent[] {
  const events: BusEvent[] = [];
  bus.subscribe("*", (e) => events.push(e));
  return events;
}

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

test("encodeJournalItems round-trips with decodeJournalItems in both directions", () => {
  // wire -> entries -> wire is the identity (what a real core sends, decoded
  // for this screen, then re-encoded by the mock core stand-in unchanged).
  assert.deepEqual(encodeJournalItems(decodeJournalItems(SAMPLE_WIRE_ITEMS)), SAMPLE_WIRE_ITEMS);

  // entries -> wire -> entries is the identity too (covers every op kind).
  const entries = decodeJournalItems([
    { seq: 6, op: "delete_created", path: "new.txt" },
    { seq: 5, op: "recreate_deleted", path: "gone.txt" },
    { seq: 4, op: "reverse_move", moved_to: "b/there.txt", original: "here.txt" },
    { seq: 3, op: "restore_overwritten", path: "changed.txt" },
    { seq: 2, op: "restore_clipboard", had_prior: true },
    { seq: 1, op: "irreversible", description: "sent an email" },
  ]);
  assert.deepEqual(decodeJournalItems(encodeJournalItems(entries)), entries);
});

test("createMockUndoCommands.previewUndo publishes undo.previewed with the real items and matching counts", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const commands = createMockUndoCommands(bus, () => decodeJournalItems(SAMPLE_WIRE_ITEMS));

  commands.previewUndo("run-x");

  const previewed = events.filter((e) => e.topic === "undo.previewed");
  assert.equal(previewed.length, 1);
  assert.deepEqual(previewed[0].payload, {
    run_id: "run-x",
    entries: 4,
    irreversible: 1,
    items: SAMPLE_WIRE_ITEMS,
  });
});

test("createMockUndoCommands.undoRun publishes undo.applied with the restored count and full newest-first narration", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const commands = createMockUndoCommands(bus, () => decodeJournalItems(SAMPLE_WIRE_ITEMS));

  commands.undoRun("run-x");

  const applied = events.filter((e) => e.topic === "undo.applied");
  assert.equal(applied.length, 1);
  assert.deepEqual(applied[0].payload, {
    run_id: "run-x",
    restored: 3,
    narration: EXPECTED_APPLIED_TEXT,
  });
});

test("real preview path: the inverted screen renders the core's real per-item restorations, irreversible labeled 'Cannot be undone'", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus, {
    commands: createMockUndoCommands(bus, () => decodeJournalItems(SAMPLE_WIRE_ITEMS)),
  });

  screen.open("run-real");
  const snap = screen.getSnapshot();

  assert.equal(snap.phase, "preview");
  assert.equal(snap.runId, "run-real");
  assert.deepEqual(snap.items.map((i) => i.text), EXPECTED_PREVIEW_TEXT, "real per-item content, not a summary");
  // Irreversible labeling: exactly the one step with no safe inverse, grayed
  // and never marked applied even in preview.
  const irreversible = snap.items.filter((i) => i.irreversible);
  assert.equal(irreversible.length, 1);
  assert.equal(irreversible[0].seq, 3);
  assert.ok(irreversible[0].text.startsWith("Cannot be undone:"));
  assert.equal(irreversible[0].applied, false);
  assert.equal(snap.restorableCount, 3);
  assert.equal(snap.irreversibleCount, 1);

  screen.dispose();
});

test("apply path: confirm() flips to the reverse filmstrip off the echoed undo.applied, restoring the reversible items and never the irreversible one", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const screen = createUndoScreen(bus, {
    commands: createMockUndoCommands(bus, () => decodeJournalItems(SAMPLE_WIRE_ITEMS)),
  });

  screen.open("run-real");
  screen.confirm();
  const snap = screen.getSnapshot();

  assert.equal(snap.phase, "done");
  assert.deepEqual(snap.items.map((i) => i.text), EXPECTED_APPLIED_TEXT, "reverse filmstrip narrates every entry in past tense");
  for (const item of snap.items) {
    assert.equal(item.applied, !item.irreversible, `applied must track irreversible exactly for ${item.text}`);
  }
  // The done summary reports the core's own restored count from undo.applied.
  const applied = events.find((e) => e.topic === "undo.applied");
  assert.equal((applied!.payload as { restored: number }).restored, 3);

  screen.dispose();
});

test("inversion: open() sends preview_undo without self-publishing, and the list stays empty until the core echoes undo.previewed", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const sent: string[] = [];
  const stub: UndoCommands = { previewUndo: (runId) => sent.push(runId), undoRun: () => {} };
  const screen = createUndoScreen(bus, { commands: stub });

  screen.open("run-real");

  assert.deepEqual(sent, ["run-real"], "open must send the preview_undo command");
  assert.equal(events.filter((e) => e.topic === "undo.previewed").length, 0, "open must not self-publish undo.previewed");
  let snap = screen.getSnapshot();
  assert.equal(snap.phase, "preview");
  assert.equal(snap.hasItems, false, "nothing renders until the core answers");

  // The core answers: now, and only now, the real restorations appear.
  bus.publish("undo.previewed", { run_id: "run-real", entries: 4, irreversible: 1, items: SAMPLE_WIRE_ITEMS });
  snap = screen.getSnapshot();
  assert.equal(snap.hasItems, true);
  assert.deepEqual(snap.items.map((i) => i.text), EXPECTED_PREVIEW_TEXT);
  // A preview echoed for some other run is ignored.
  bus.publish("undo.previewed", { run_id: "some-other-run", entries: 1, irreversible: 0, items: [{ seq: 1, op: "delete_created", path: "x" }] });
  assert.deepEqual(screen.getSnapshot().items.map((i) => i.text), EXPECTED_PREVIEW_TEXT, "an unrelated run's preview must not overwrite this one");

  screen.dispose();
});

test("inversion: confirm() sends undo_run once and reaches done only on the echoed undo.applied (no self-publish, no double-submit)", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const undone: string[] = [];
  const stub: UndoCommands = { previewUndo: () => {}, undoRun: (runId) => undone.push(runId) };
  const screen = createUndoScreen(bus, { commands: stub });

  screen.open("run-real");
  // Give the screen a real preview to act on (the core's echo).
  bus.publish("undo.previewed", { run_id: "run-real", entries: 4, irreversible: 1, items: SAMPLE_WIRE_ITEMS });

  screen.confirm();
  screen.confirm(); // a second click before the core answers must not resend.

  assert.deepEqual(undone, ["run-real"], "confirm must send undo_run exactly once");
  assert.equal(events.filter((e) => e.topic === "undo.applied").length, 0, "confirm must not self-publish undo.applied");
  assert.equal(screen.getSnapshot().phase, "preview", "the screen stays in preview until the core echoes undo.applied");

  // The core executes and echoes; now the screen flips to the reverse filmstrip.
  bus.publish("undo.applied", { run_id: "run-real", restored: 3, narration: EXPECTED_APPLIED_TEXT });
  const snap = screen.getSnapshot();
  assert.equal(snap.phase, "done");
  assert.deepEqual(snap.items.map((i) => i.text), EXPECTED_APPLIED_TEXT);
  assert.equal(snap.doneSummary, "Restored 3 items.");

  screen.dispose();
});

test("end to end: the undo screen renders the REAL per-item text over the mock core, distinct from ./mockJournal.ts's own fixture wording", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus, {
    commands: createMockUndoCommands(bus, () => decodeJournalItems(SAMPLE_WIRE_ITEMS)),
  });

  screen.open("run-real");
  const snap = screen.getSnapshot();
  assert.deepEqual(snap.items.map((i) => i.text), EXPECTED_PREVIEW_TEXT);
  // Not the demo fixture's own recreate line, so this is genuinely the run's data.
  assert.notEqual(
    snap.items[3].text,
    previewLine({ op: "recreate_deleted", path: "old_notes.txt" }),
    "must not be the fixture's own old_notes.txt line",
  );

  screen.dispose();
});
