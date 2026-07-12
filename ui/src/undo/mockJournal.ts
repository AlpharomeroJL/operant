// A client-side stand-in for the undo journal crates/recorder/src/undo.rs
// records (C20). docs/specs/design.md section 3's Undo screen: "From any
// completed run, 'Undo this run' opens a preview list of restorations in
// plain English... Confirm executes with the same filmstrip treatment in
// reverse." This file mirrors that Rust module's `Inverse` tagged union and
// its preview_line()/applied_line() wording exactly (see undo.rs's own doc
// comments), so what this screen shows reads as what the real journal
// already proves it can do end to end (crates/recorder/tests/undo_journal.rs's
// byte-identical-restore test): create/delete/move/overwrite file inverses,
// a clipboard-restore inverse, and an irreversible marker for a step with no
// safe inverse (email send, form submit, side-effectful shell).
//
// F1b CLOSED THE CONTRACT HALF OF THIS GAP: contracts/bus_events.md's
// undo.previewed now carries an optional `items` field
// (ui/src/bus/types.ts's UndoJournalItemWire[]) with the real per-item
// journal content, and crates/recorder's Recorder::publish_undo_preview
// (crates/recorder/src/undo.rs) genuinely publishes it onto a real
// operant_core::Bus (proved end to end, real temp directory included, by
// crates/recorder/tests/undo_journal.rs). ./realJournal.ts decodes that wire
// shape back into this file's own UndoJournalEntry/UndoInverse, and
// ui/src/main.ts feeds ui/src/undo/state.ts's `journalForRun` seam that real
// source ahead of this fixture (real data wins when present).
//
// STILL HONEST ABOUT WHAT REMAINS: no process-boundary transport carries a
// recorder-published bus event into this UI process yet
// (ui/src/bus/mockClient.ts's own header: a Tauri IPC bridge onto the Rust
// core's bus is still future work), so every run in the app as shipped today
// still falls through to this fixture, exactly as before. The moment that
// transport exists and forwards a real undo.previewed envelope onto this
// same BusClient, a real run's real journal wins automatically, with no
// further change needed in ./state.ts or ui/src/main.ts. See
// ./realJournal.test.ts for both paths (a real payload present, and this
// fixture fallback) proven end to end today against a plain BusClient.

/** Mirrors crates/recorder/src/undo.rs's `Inverse` enum, one variant per PendingWrite kind that module documents. */
export type UndoInverse =
  | { op: "delete_created"; path: string }
  | { op: "recreate_deleted"; path: string }
  | { op: "reverse_move"; movedTo: string; original: string }
  | { op: "restore_overwritten"; path: string }
  | { op: "restore_clipboard"; hadPrior: boolean }
  | { op: "irreversible"; description: string };

/** One `undo_journal` row: mirrors Recorder::list_undo's entry shape (seq plus the decoded inverse), minus the fields (run id, blob hash) this screen never needs to show. */
export interface UndoJournalEntry {
  seq: number;
  inverse: UndoInverse;
}

/** True when this entry has no reversal to perform (crates/recorder/src/undo.rs: Inverse::is_irreversible). */
export function isIrreversible(entry: UndoJournalEntry): boolean {
  return entry.inverse.op === "irreversible";
}

/** Future-tense preview sentence, wording mirrored from Inverse::preview_line (crates/recorder/src/undo.rs). */
export function previewLine(inverse: UndoInverse): string {
  switch (inverse.op) {
    case "delete_created":
      return `Would delete the file the run created: ${inverse.path}`;
    case "recreate_deleted":
      return `Would recreate the deleted file from its saved copy: ${inverse.path}`;
    case "reverse_move":
      return `Would move ${inverse.movedTo} back to ${inverse.original}`;
    case "restore_overwritten":
      return `Would restore the previous contents of ${inverse.path}`;
    case "restore_clipboard":
      return inverse.hadPrior
        ? "Would restore the previous clipboard contents"
        : "Would clear the clipboard (it was empty before the run)";
    case "irreversible":
      return `Cannot be undone: ${inverse.description}`;
  }
}

/** Completed-action narration, wording mirrored from Inverse::applied_line (crates/recorder/src/undo.rs). */
export function appliedLine(inverse: UndoInverse): string {
  switch (inverse.op) {
    case "delete_created":
      return `Deleted the file the run created: ${inverse.path}`;
    case "recreate_deleted":
      return `Recreated the deleted file from its saved copy: ${inverse.path}`;
    case "reverse_move":
      return `Moved ${inverse.movedTo} back to ${inverse.original}`;
    case "restore_overwritten":
      return `Restored the previous contents of ${inverse.path}`;
    case "restore_clipboard":
      return inverse.hadPrior
        ? "Restored the previous clipboard contents"
        : "Cleared the clipboard (it was empty before the run)";
    case "irreversible":
      return `Cannot be undone: ${inverse.description}`;
  }
}

// The canned fixture: the same mix of write kinds crates/recorder/tests/
// undo_journal.rs proves end to end (create, move, overwrite, delete, plus
// one irreversible step), here with a clipboard write too so every
// PendingWrite kind undo.rs documents is represented at least once.
// Newest-first (seq descending), the same order Recorder::
// inverses_newest_first_seq sorts the real journal into.
export const DEMO_JOURNAL_FIXTURE: readonly UndoJournalEntry[] = [
  { seq: 6, inverse: { op: "restore_clipboard", hadPrior: true } },
  { seq: 5, inverse: { op: "irreversible", description: "sent the invoice email to boss@example.com" } },
  { seq: 4, inverse: { op: "delete_created", path: "receipt.txt" } },
  { seq: 3, inverse: { op: "restore_overwritten", path: "invoice.txt" } },
  { seq: 2, inverse: { op: "reverse_move", movedTo: "Archive/draft.txt", original: "draft.txt" } },
  { seq: 1, inverse: { op: "recreate_deleted", path: "old_notes.txt" } },
];

/**
 * The journal for a completed run, in any order (state.ts always sorts
 * newest-first itself, defensively, the same way Recorder::
 * inverses_newest_first_seq never trusts storage order either). No per-run
 * fidelity exists client-side yet (see this file's header), so every run id
 * reads the same demo journal: enough to prove the preview/confirm/
 * irreversible UI end to end, honestly not a claim that this specific run's
 * actual files are listed.
 */
export function journalForRun(_runId: string): readonly UndoJournalEntry[] {
  return DEMO_JOURNAL_FIXTURE;
}
