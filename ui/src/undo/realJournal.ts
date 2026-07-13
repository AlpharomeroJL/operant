// The TypeScript half of the real per-run undo wire (F1b + B10). Two jobs
// live here now that the undo screen is inverted onto real commands
// (contracts/ipc.md's preview_undo / undo_run):
//
//   1. decodeJournalItems: turn contracts/bus_events.md's undo.previewed
//      `items` (ui/src/bus/types.ts's UndoJournalItemWire[], published by
//      crates/recorder's Recorder::publish_undo_preview,
//      crates/recorder/src/undo.rs) back into this screen's own
//      UndoJournalEntry/UndoInverse (./mockJournal.ts). ui/src/undo/state.ts
//      calls this from its undo.previewed subscription, so the screen renders
//      the core's real per-item restorations with no translation layer.
//
//   2. createMockUndoCommands: the dev/Demo stand-in for the core's undo
//      commands. B10 inverted state.ts's open()/confirm() so they no longer
//      self-fabricate the preview or self-publish the applied event; they now
//      send the preview_undo / undo_run commands and react to the
//      undo.previewed / undo.applied the core echoes back. Outside Tauri there
//      is no core process (ui/src/bus/mockClient.ts's own header: the Tauri
//      IPC bridge onto the Rust core's bus is still future work), so this
//      factory plays the core's part on the mock bus, publishing the same two
//      events crates/recorder would, sourced from a journal lookup
//      (./mockJournal.ts's fixture in dev/Demo). encodeJournalItems is the
//      inverse of decodeJournalItems it uses to build the wire `items`.
//
// The moment the real transport exists and forwards a real undo.previewed /
// undo.applied onto this same BusClient, ui/src/main.ts swaps this mock
// command sender for a real (invoke-backed) UndoCommands with no change to
// state.ts: the screen already reacts to whatever the bus carries. See
// ./realJournal.test.ts for the decode, the encode round-trip, and the mock
// commands driving the inverted screen end to end.

import type { BusClient } from "../bus/mockClient.ts";
import type { UndoInverseWire, UndoJournalItemWire } from "../bus/types.ts";
import { appliedLine, isIrreversible, type UndoInverse, type UndoJournalEntry } from "./mockJournal.ts";
import type { UndoCommands } from "./state.ts";

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
 * already sorted it, the same distrust ui/src/undo/state.ts's own
 * undo.previewed handler has of whatever the core sends) into this screen's
 * UndoJournalEntry[].
 */
export function decodeJournalItems(items: readonly UndoJournalItemWire[]): UndoJournalEntry[] {
  return items.map((item) => ({ seq: item.seq, inverse: decodeInverse(item) }));
}

/** Encode one UndoInverse back to its wire shape: the inverse of decodeInverse, so the mock core below emits exactly what crates/recorder would. */
function encodeInverse(inverse: UndoInverse): UndoInverseWire {
  switch (inverse.op) {
    case "delete_created":
      return { op: "delete_created", path: inverse.path };
    case "recreate_deleted":
      return { op: "recreate_deleted", path: inverse.path };
    case "reverse_move":
      return { op: "reverse_move", moved_to: inverse.movedTo, original: inverse.original };
    case "restore_overwritten":
      return { op: "restore_overwritten", path: inverse.path };
    case "restore_clipboard":
      return { op: "restore_clipboard", had_prior: inverse.hadPrior };
    case "irreversible":
      return { op: "irreversible", description: inverse.description };
  }
}

/** Encode entries to the undo.previewed `items` wire shape (seq flattened alongside the tagged union, as crates/core/src/bus/events.rs's #[serde(flatten)] emits). */
export function encodeJournalItems(entries: readonly UndoJournalEntry[]): UndoJournalItemWire[] {
  return entries.map((entry) => ({ seq: entry.seq, ...encodeInverse(entry.inverse) }));
}

/**
 * The dev/Demo stand-in for the core's undo commands (contracts/ipc.md's
 * preview_undo / undo_run). Each command publishes onto `bus` the exact event
 * crates/recorder would in response, sourced from `journalForRun`:
 *
 *   previewUndo(runId) -> undo.previewed with real per-item `items` (what
 *     Recorder::publish_undo_preview emits), so ui/src/undo/state.ts's
 *     subscription decodes and renders the real restorations.
 *   undoRun(runId)     -> undo.applied with the restored count and the
 *     newest-first narration (what Recorder::undo_run's echo carries), so the
 *     screen flips to its reverse filmstrip off the echoed event.
 *
 * The screen sorts newest-first itself, so this deliberately publishes in the
 * journal's own order (unsorted), the same latitude a real core has.
 * ui/src/main.ts injects this in dev/Demo and swaps it for a real invoke-backed
 * UndoCommands once the Tauri bridge lands, with no screen change.
 */
export function createMockUndoCommands(
  bus: BusClient,
  journalForRun: (runId: string) => readonly UndoJournalEntry[],
): UndoCommands {
  return {
    previewUndo(runId: string): void {
      const entries = journalForRun(runId);
      bus.publish("undo.previewed", {
        run_id: runId,
        entries: entries.length,
        irreversible: entries.filter(isIrreversible).length,
        items: encodeJournalItems(entries),
      });
    },
    undoRun(runId: string): void {
      const entries = journalForRun(runId);
      const restored = entries.filter((entry) => !isIrreversible(entry)).length;
      const narration = entries.map((entry) => appliedLine(entry.inverse));
      bus.publish("undo.applied", { run_id: runId, restored, narration });
    },
  };
}
