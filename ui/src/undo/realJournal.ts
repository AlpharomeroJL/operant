// The real per-run undo journal source (F1b), closing the gap
// ./mockJournal.ts's header used to flag: contracts/bus_events.md's
// undo.previewed now carries an optional `items` field
// (ui/src/bus/types.ts's UndoJournalItemWire[]) with the real per-item
// restoration content, published by crates/recorder's
// Recorder::publish_undo_preview (crates/recorder/src/undo.rs) onto the
// real operant_core::Bus.
//
// This file is the other half of that wire on the TypeScript side:
// decodeJournalItems turns the wire shape back into this screen's own
// UndoJournalEntry/UndoInverse (./mockJournal.ts, unchanged), and
// createRealJournalSource remembers any such payload it sees on a
// BusClient, keyed by run id, so a later journalForRun(run_id) call
// (ui/src/undo/state.ts's own seam, also unchanged) can return it.
//
// No process-boundary transport carries a recorder-published bus event into
// this UI process yet (ui/src/bus/mockClient.ts's own header: a Tauri IPC
// bridge onto the Rust core's bus is still future work), so
// createRealJournalSource has nothing to remember in the app as shipped
// today: every run still falls through to ./mockJournal.ts's fixture via
// ui/src/main.ts's fallback below. The moment that transport exists and
// forwards a real undo.previewed envelope onto this same BusClient, a real
// run's real journal wins automatically, with no further change needed
// here, in ./state.ts, or in ui/src/main.ts. See ./realJournal.test.ts for
// both paths (a real payload present, and the fixture fallback) proven end
// to end today against a plain BusClient, independent of that future
// transport.

import type { BusClient } from "../bus/mockClient.ts";
import type { UndoJournalItemWire } from "../bus/types.ts";
import type { UndoInverse, UndoJournalEntry } from "./mockJournal.ts";

/**
 * Decode one wire item (contracts/bus_events.md's undo.previewed `items`)
 * into this screen's own UndoInverse (./mockJournal.ts): the inverse of
 * crates/recorder/src/undo.rs's Inverse::to_wire.
 */
function decodeInverse(wire: UndoJournalItemWire): UndoInverse {
  switch (wire.op) {
    case "delete_created":
      return { op: "delete_created", path: wire.path };
    case "recreate_deleted":
      return { op: "recreate_deleted", path: wire.path };
    case "reverse_move":
      return { op: "reverse_move", movedTo: wire.moved_to, original: wire.original };
    case "restore_overwritten":
      return { op: "restore_overwritten", path: wire.path };
    case "restore_clipboard":
      return { op: "restore_clipboard", hadPrior: wire.had_prior };
    case "irreversible":
      return { op: "irreversible", description: wire.description };
  }
}

/**
 * Decode a full `items` array (any order; callers must not assume the wire
 * already sorted it, the same distrust ./state.ts's own open() has of
 * whatever journalForRun hands it) into this screen's UndoJournalEntry[].
 */
export function decodeJournalItems(items: readonly UndoJournalItemWire[]): UndoJournalEntry[] {
  return items.map((item) => ({ seq: item.seq, inverse: decodeInverse(item) }));
}

export interface RealJournalSource {
  /**
   * Same signature CreateUndoScreenOptions.journalForRun wants, minus the
   * fixture fallback: undefined means nothing real has arrived yet for this
   * run id, and the caller (ui/src/main.ts) falls back to
   * ./mockJournal.ts's journalForRun itself.
   */
  journalForRun(runId: string): readonly UndoJournalEntry[] | undefined;
  /** Stops listening and forgets everything remembered so far. */
  dispose(): void;
}

/**
 * Subscribes to undo.previewed on `bus` and remembers, per run id, any
 * payload that carries a non-empty `items` field: a real per-run journal
 * from crates/recorder's Recorder::publish_undo_preview, once a transport
 * forwards it onto this bus (see this file's header). A payload with no
 * items (today, every undo.previewed this app itself publishes:
 * ./state.ts's own open() never sets items) is not remembered, so the
 * screen's own self-published counts can never be mistaken for real data.
 */
export function createRealJournalSource(bus: BusClient): RealJournalSource {
  const remembered = new Map<string, UndoJournalEntry[]>();

  const unsubscribe = bus.subscribe("undo.previewed", (event) => {
    if (event.topic !== "undo.previewed") return;
    const { run_id, items } = event.payload;
    if (!items || items.length === 0) return;
    remembered.set(run_id, decodeJournalItems(items));
  });

  return {
    journalForRun(runId) {
      return remembered.get(runId);
    },
    dispose() {
      unsubscribe();
      remembered.clear();
    },
  };
}
