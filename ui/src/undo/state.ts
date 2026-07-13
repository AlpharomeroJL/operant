// The Undo screen's state (docs/specs/design.md section 3): "From any
// completed run, 'Undo this run' opens a preview list of restorations in
// plain English with per-item checkmarks. Irreversible items are grayed with
// 'cannot be undone.' Confirm executes with the same filmstrip treatment in
// reverse." Pure and DOM-free, same split as ui/src/runViewer/state.ts and
// ui/src/grants/state.ts, so it runs under plain `node --test`.
//
// B10 inverted this screen onto the real journal (contracts/ipc.md,
// docs/specs/ipc-bridge.md section 8b). It no longer self-fabricates:
//   open(runId)  sends the preview_undo command (UndoCommands.previewUndo);
//                the core answers by publishing the real undo.previewed with
//                per-item `items`, which the subscription below decodes
//                (ui/src/undo/realJournal.ts) into the restoration list.
//   confirm()    sends the undo_run command (UndoCommands.undoRun); the core
//                executes and echoes undo.applied, and the screen flips to its
//                reverse filmstrip off that echoed event, never off its own
//                publish.
// The screen only ever reacts to undo.previewed / undo.applied; it publishes
// neither. In dev/Demo the commands are ui/src/undo/realJournal.ts's
// createMockUndoCommands (a core stand-in over ./mockJournal.ts's fixture);
// ui/src/main.ts swaps in a real invoke-backed UndoCommands once the Tauri
// bridge exists, with no change here. A listener such as the Advanced audit
// browser (ui/src/advanced/view.ts's mountAuditBrowser) still sees the same
// two topics, now carrying the core's real data.

import type { BusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { journalForRun as defaultJournalForRun, isIrreversible, previewLine, appliedLine, type UndoJournalEntry } from "./mockJournal.ts";
import { createMockUndoCommands, decodeJournalItems } from "./realJournal.ts";
import { undoScreenStrings } from "./strings.ts";

export type UndoPhase = "closed" | "preview" | "done";

/**
 * How the screen sends its two commands to the core (contracts/ipc.md 5c):
 * open() -> previewUndo (preview_undo), confirm() -> undoRun (undo_run). The
 * screen never assumes these publish anything itself; it waits for the core to
 * echo undo.previewed / undo.applied back onto the bus. ui/src/main.ts injects
 * the implementation: createMockUndoCommands in dev/Demo, a real invoke-backed
 * sender under Tauri.
 */
export interface UndoCommands {
  /** Ask the core to publish this run's undo.previewed(items[]) (contracts/ipc.md preview_undo). */
  previewUndo(runId: string): void;
  /** Ask the core to execute the undo and echo undo.applied (contracts/ipc.md undo_run). */
  undoRun(runId: string): void;
}

export interface UndoItemView {
  seq: number;
  /** The preview sentence while previewing, the past-tense narration once done; always the "Cannot be undone: ..." line for an irreversible entry. */
  text: string;
  irreversible: boolean;
  /** True only once this reversible entry has actually been restored (phase "done"); always false for an irreversible entry, which is never touched. */
  applied: boolean;
}

export interface UndoScreenSnapshot {
  phase: UndoPhase;
  runId: string | null;
  title: string;
  items: readonly UndoItemView[];
  hasItems: boolean;
  restorableCount: number;
  irreversibleCount: number;
  confirmLabel: string;
  cancelLabel: string;
  closeLabel: string;
  emptyLabel: string;
  doneSummary: string;
}

export interface UndoScreen {
  getSnapshot(): UndoScreenSnapshot;
  /** Notified with a fresh snapshot after open/confirm/close. */
  subscribe(fn: (snapshot: UndoScreenSnapshot) => void): () => void;
  /** Opens the preview for a completed run: sends preview_undo, then renders the real restorations once the core echoes undo.previewed (design.md section 3). Replaces whatever this screen was previously showing. */
  open(runId: string): void;
  /** Executes the undo: sends undo_run and, once the core echoes undo.applied, narrates every entry (restoring the reversible ones; an irreversible entry is only ever listed, never touched, mirroring crates/recorder/src/undo.rs's undo_run). No-op unless phase is "preview". */
  confirm(): void;
  /** Dismiss the screen (Cancel from preview, or Close from done). No-op if already closed. */
  close(): void;
  dispose(): void;
}

export interface CreateUndoScreenOptions {
  /**
   * How open()/confirm() reach the core (contracts/ipc.md 5c). Defaults to
   * ui/src/undo/realJournal.ts's createMockUndoCommands over `journalForRun`
   * below, so the screen renders standalone in dev/Demo and under `node
   * --test`. ui/src/main.ts injects a real invoke-backed sender under Tauri.
   */
  commands?: UndoCommands;
  /**
   * Only used to build the default mock commands (ignored when `commands` is
   * given): the journal the dev/Demo core stand-in previews from. Defaults to
   * ./mockJournal.ts's fixture; tests inject a scenario.
   */
  journalForRun?: (runId: string) => readonly UndoJournalEntry[];
}

interface InternalState {
  phase: UndoPhase;
  runId: string | null;
  entries: readonly UndoJournalEntry[];
  /** The restored count from the echoed undo.applied, or null before it arrives; drives the done summary from the core's own number. */
  restored: number | null;
}

const INITIAL_STATE: InternalState = { phase: "closed", runId: null, entries: [], restored: null };

export function createUndoScreen(bus: BusClient, opts: CreateUndoScreenOptions = {}): UndoScreen {
  const commands = opts.commands ?? createMockUndoCommands(bus, opts.journalForRun ?? defaultJournalForRun);
  let state: InternalState = INITIAL_STATE;
  // True from the moment confirm() sends undo_run until its undo.applied is
  // echoed back: guards against a double-submit (a second confirm() before the
  // core answers must not fire undo_run twice) and gates which undo.applied we
  // accept (only the one we asked for).
  let awaitingApply = false;
  const listeners = new Set<(snapshot: UndoScreenSnapshot) => void>();

  function snapshot(): UndoScreenSnapshot {
    const done = state.phase === "done";
    const items: UndoItemView[] = state.entries.map((entry) => {
      const irreversible = isIrreversible(entry);
      return {
        seq: entry.seq,
        text: done ? appliedLine(entry.inverse) : previewLine(entry.inverse),
        irreversible,
        applied: done && !irreversible,
      };
    });
    const restorableCount = items.filter((item) => !item.irreversible).length;
    return {
      phase: state.phase,
      runId: state.runId,
      title: undoScreenStrings.title,
      items,
      hasItems: items.length > 0,
      restorableCount,
      irreversibleCount: items.length - restorableCount,
      confirmLabel: undoScreenStrings.confirm,
      cancelLabel: undoScreenStrings.cancel,
      closeLabel: undoScreenStrings.close,
      emptyLabel: undoScreenStrings.empty,
      // Prefer the core's own restored count from the echoed undo.applied,
      // falling back to the reversible-entry count before it arrives.
      doneSummary: undoScreenStrings.doneSummary(state.restored ?? restorableCount),
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function handle(event: BusEvent): void {
    if (event.topic === "undo.previewed") {
      // Only the preview for the run this screen is currently opening, and
      // only while still previewing it. A payload without items (an older
      // publisher; contracts/bus_events.md keeps `items` optional) carries
      // nothing to render, so it is ignored rather than emptying the list.
      if (state.phase !== "preview" || event.payload.run_id !== state.runId) return;
      const items = event.payload.items;
      if (!items) return;
      // Newest-first, defensively re-sorted here rather than trusting the
      // core's own ordering, the same distrust Recorder::
      // inverses_newest_first_seq has of raw storage order.
      const entries = [...decodeJournalItems(items)].sort((a, b) => b.seq - a.seq);
      state = { ...state, entries };
      emit();
      return;
    }
    if (event.topic === "undo.applied") {
      // Only the applied echo for the undo this screen actually asked for.
      if (!awaitingApply || event.payload.run_id !== state.runId) return;
      awaitingApply = false;
      state = { ...state, phase: "done", restored: event.payload.restored };
      emit();
      return;
    }
  }

  const unsubscribe = bus.subscribe("undo", handle);

  function open(runId: string): void {
    // Enter the preview for this run and ask the core for its restorations.
    // No emit here: the list has no content to show until the echoed
    // undo.previewed arrives (the handler above emits then), so the modal
    // opens on real data instead of flashing an empty "nothing to undo".
    awaitingApply = false;
    state = { phase: "preview", runId, entries: [], restored: null };
    commands.previewUndo(runId);
  }

  function confirm(): void {
    if (state.phase !== "preview" || !state.runId || awaitingApply) return;
    // Send undo_run and wait: the done phase is entered by the echoed
    // undo.applied (the handler above), never here.
    awaitingApply = true;
    commands.undoRun(state.runId);
  }

  function close(): void {
    if (state.phase === "closed") return;
    awaitingApply = false;
    state = INITIAL_STATE;
    emit();
  }

  function dispose(): void {
    unsubscribe();
    listeners.clear();
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    open,
    confirm,
    close,
    dispose,
  };
}
