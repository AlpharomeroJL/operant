import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { createUndoScreen } from "./state.ts";
import { DEMO_JOURNAL_FIXTURE, type UndoJournalEntry } from "./mockJournal.ts";
import { undoScreenStrings } from "./strings.ts";

function collect(bus: ReturnType<typeof createMockBusClient>): BusEvent[] {
  const events: BusEvent[] = [];
  bus.subscribe("*", (e) => events.push(e));
  return events;
}

test("closed by default: no run id, no items, nothing to confirm or dismiss", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus);
  const snap = screen.getSnapshot();

  assert.equal(snap.phase, "closed");
  assert.equal(snap.runId, null);
  assert.deepEqual(snap.items, []);
  assert.equal(snap.hasItems, false);
  screen.dispose();
});

test("open: the fixture's completed-run journal previews as the exact restoration list, newest-first", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus);

  screen.open("run-1");
  const snap = screen.getSnapshot();

  assert.equal(snap.phase, "preview");
  assert.equal(snap.runId, "run-1");
  assert.equal(snap.title, undoScreenStrings.title);
  assert.equal(snap.hasItems, true);

  // Exact text, exact order: this is the plain-English preview list design.md
  // section 3 calls for, not a summary or a count.
  assert.deepEqual(
    snap.items.map((i) => i.text),
    [
      "Would restore the previous clipboard contents",
      "Cannot be undone: sent the invoice email to boss@example.com",
      "Would delete the file the run created: receipt.txt",
      "Would restore the previous contents of invoice.txt",
      "Would move Archive/draft.txt back to draft.txt",
      "Would recreate the deleted file from its saved copy: old_notes.txt",
    ],
  );

  // seq descending: newest-journaled entry previews first.
  assert.deepEqual(
    snap.items.map((i) => i.seq),
    [6, 5, 4, 3, 2, 1],
  );

  screen.dispose();
});

test("open: irreversible items are flagged, grayed-eligible, and never marked applied even before confirm", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus);
  screen.open("run-1");
  const snap = screen.getSnapshot();

  const irreversible = snap.items.filter((i) => i.irreversible);
  assert.equal(irreversible.length, 1);
  assert.ok(irreversible[0].text.startsWith("Cannot be undone:"));
  assert.equal(irreversible[0].applied, false);

  assert.equal(snap.restorableCount, 5);
  assert.equal(snap.irreversibleCount, 1);
  screen.dispose();
});

test("open sends preview_undo, whose echoed undo.previewed carries the real per-item items alongside the counts (contracts/ipc.md, bus_events.md)", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const screen = createUndoScreen(bus);

  screen.open("run-42");

  // With the default (mock) core stand-in, the preview_undo command is
  // answered by exactly one undo.previewed, now carrying the fixture's real
  // per-item restorations, not just a count.
  const previewedEvents = events.filter((e) => e.topic === "undo.previewed");
  assert.equal(previewedEvents.length, 1, "exactly one undo.previewed answers the command");
  const payload = previewedEvents[0].payload as {
    run_id: string;
    entries: number;
    irreversible: number;
    items?: { seq: number }[];
  };
  assert.equal(payload.run_id, "run-42");
  assert.equal(payload.entries, 6);
  assert.equal(payload.irreversible, 1);
  assert.equal(payload.items?.length, 6, "the echoed preview carries every journal item, not just the counts");
  assert.deepEqual(
    payload.items?.map((i) => i.seq),
    [6, 5, 4, 3, 2, 1],
    "items mirror the fixture's own journal entries",
  );
  screen.dispose();
});

test("confirm: reversible items flip to past-tense narration and applied=true; the irreversible item stays worded the same and is never applied", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus);
  screen.open("run-1");

  screen.confirm();
  const snap = screen.getSnapshot();

  assert.equal(snap.phase, "done");
  assert.deepEqual(
    snap.items.map((i) => i.text),
    [
      "Restored the previous clipboard contents",
      "Cannot be undone: sent the invoice email to boss@example.com",
      "Deleted the file the run created: receipt.txt",
      "Restored the previous contents of invoice.txt",
      "Moved Archive/draft.txt back to draft.txt",
      "Recreated the deleted file from its saved copy: old_notes.txt",
    ],
  );

  for (const item of snap.items) {
    assert.equal(item.applied, !item.irreversible, `applied must track irreversible exactly for ${item.text}`);
  }
  assert.equal(snap.doneSummary, undoScreenStrings.doneSummary(5));
  screen.dispose();
});

test("confirm publishes undo.applied with restored count and full narration (irreversible entries narrated but not counted as restored)", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const screen = createUndoScreen(bus);
  screen.open("run-9");

  screen.confirm();

  const applied = events.find((e) => e.topic === "undo.applied");
  assert.ok(applied, "undo.applied must be published");
  const payload = applied!.payload as { run_id: string; restored: number; narration: string[] };
  assert.equal(payload.run_id, "run-9");
  assert.equal(payload.restored, 5);
  assert.equal(payload.narration.length, 6, "one narration line per journal entry, including the irreversible one");
  assert.ok(payload.narration.some((line) => line.includes("Cannot be undone")));
  screen.dispose();
});

test("confirm is a no-op outside the preview phase: closed stays closed, done stays done and does not re-publish", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const screen = createUndoScreen(bus);

  screen.confirm(); // closed: nothing to confirm
  assert.equal(screen.getSnapshot().phase, "closed");
  assert.equal(events.filter((e) => e.topic === "undo.applied").length, 0);

  screen.open("run-1");
  screen.confirm();
  screen.confirm(); // already done: must not double-apply or double-publish
  assert.equal(events.filter((e) => e.topic === "undo.applied").length, 1);
  screen.dispose();
});

test("close: dismisses from preview or from done back to closed, and is a no-op when already closed", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus);

  screen.open("run-1");
  screen.close();
  let snap = screen.getSnapshot();
  assert.equal(snap.phase, "closed");
  assert.equal(snap.runId, null);
  assert.deepEqual(snap.items, []);

  screen.open("run-1");
  screen.confirm();
  screen.close();
  snap = screen.getSnapshot();
  assert.equal(snap.phase, "closed");

  screen.close(); // already closed
  assert.equal(screen.getSnapshot().phase, "closed");
  screen.dispose();
});

test("a custom journalForRun scenario: an all-irreversible run previews with nothing restorable, and confirm restores nothing", () => {
  const bus = createMockBusClient();
  const events = collect(bus);
  const onlyIrreversible: UndoJournalEntry[] = [
    { seq: 1, inverse: { op: "irreversible", description: "posted the message to #general" } },
  ];
  const screen = createUndoScreen(bus, { journalForRun: () => onlyIrreversible });

  screen.open("run-slack");
  let snap = screen.getSnapshot();
  assert.equal(snap.hasItems, true);
  assert.equal(snap.restorableCount, 0);
  assert.equal(snap.irreversibleCount, 1);

  screen.confirm();
  snap = screen.getSnapshot();
  assert.equal(snap.items[0].applied, false);
  assert.equal(snap.doneSummary, undoScreenStrings.doneSummary(0));

  const applied = events.find((e) => e.topic === "undo.applied");
  assert.equal((applied!.payload as { restored: number }).restored, 0);
  screen.dispose();
});

test("an empty journal (nothing was ever journaled for this run) previews with hasItems false", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus, { journalForRun: () => [] });

  screen.open("run-readonly");
  const snap = screen.getSnapshot();
  assert.equal(snap.hasItems, false);
  assert.equal(snap.items.length, 0);
  assert.equal(snap.emptyLabel, undoScreenStrings.empty);
  screen.dispose();
});

test("open defensively sorts newest-first even when the journal source hands entries out of order", () => {
  const bus = createMockBusClient();
  const outOfOrder: UndoJournalEntry[] = [
    { seq: 1, inverse: { op: "delete_created", path: "a.txt" } },
    { seq: 3, inverse: { op: "delete_created", path: "c.txt" } },
    { seq: 2, inverse: { op: "delete_created", path: "b.txt" } },
  ];
  const screen = createUndoScreen(bus, { journalForRun: () => outOfOrder });

  screen.open("run-1");
  assert.deepEqual(
    screen.getSnapshot().items.map((i) => i.seq),
    [3, 2, 1],
  );
  screen.dispose();
});

test("open replaces whatever was previously open, and re-opening the same run resets to a fresh preview", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus);

  screen.open("run-1");
  screen.confirm();
  assert.equal(screen.getSnapshot().phase, "done");

  screen.open("run-2");
  const snap = screen.getSnapshot();
  assert.equal(snap.phase, "preview");
  assert.equal(snap.runId, "run-2");
  assert.ok(snap.items.every((i) => !i.applied), "a freshly opened run must not show anything as already applied");
  screen.dispose();
});

test("subscribe: notified on open, confirm, and close; dispose stops further notifications", () => {
  const bus = createMockBusClient();
  const screen = createUndoScreen(bus);
  const phases: string[] = [];
  screen.subscribe((snap) => phases.push(snap.phase));

  screen.open("run-1");
  screen.confirm();
  screen.close();
  assert.deepEqual(phases, ["preview", "done", "closed"]);

  screen.dispose();
  const before = phases.length;
  screen.open("run-2");
  assert.equal(phases.length, before, "no further notifications are expected once every listener has been removed, but open() still mutates its own state safely");
});

test("DEMO_JOURNAL_FIXTURE covers every PendingWrite kind crates/recorder/src/undo.rs documents", () => {
  const kinds = new Set(DEMO_JOURNAL_FIXTURE.map((e) => e.inverse.op));
  assert.deepEqual(
    kinds,
    new Set(["delete_created", "recreate_deleted", "reverse_move", "restore_overwritten", "restore_clipboard", "irreversible"]),
  );
});
