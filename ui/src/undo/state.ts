// The Undo screen's state (docs/specs/design.md section 3): "From any
// completed run, 'Undo this run' opens a preview list of restorations in
// plain English with per-item checkmarks. Irreversible items are grayed with
// 'cannot be undone.' Confirm executes with the same filmstrip treatment in
// reverse." Pure and DOM-free, same split as ui/src/runViewer/state.ts and
// ui/src/grants/state.ts, so it runs under plain `node --test`.
//
// Reads the undo-journal shape via the journalForRun seam below
// (CreateUndoScreenOptions), which defaults to ./mockJournal.ts's fixture
// (see that file's header for the honest reason why); ui/src/main.ts feeds
// this seam ./undo/realJournal.ts's real per-run source ahead of the
// fixture (F1b), so a run with a real published journal renders that
// instead, fixture otherwise. Publishes the two bus topics
// contracts/bus_events.md already reserves for this feature
// (undo.previewed, undo.applied, ui/src/bus/types.ts) with real data derived
// from whatever journal was loaded, so a listener (the Advanced audit
// browser, ui/src/advanced/view.ts's mountAuditBrowser) sees this screen's
// actual activity, not a stub.

import type { BusClient } from "../bus/mockClient.ts";
import { journalForRun as defaultJournalForRun, isIrreversible, previewLine, appliedLine, type UndoJournalEntry } from "./mockJournal.ts";
import { undoScreenStrings } from "./strings.ts";

export type UndoPhase = "closed" | "preview" | "done";

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
  /** Opens the preview for a completed run's journal, newest-first (design.md section 3). Replaces whatever this screen was previously showing. */
  open(runId: string): void;
  /** Executes the undo: narrates every entry, restoring the reversible ones (an irreversible entry is only ever listed, never touched, mirroring crates/recorder/src/undo.rs's undo_run). No-op unless phase is "preview". Publishes undo.applied. */
  confirm(): void;
  /** Dismiss the screen (Cancel from preview, or Close from done). No-op if already closed. */
  close(): void;
  dispose(): void;
}

export interface CreateUndoScreenOptions {
  /** Override the journal lookup; tests inject a scenario. Defaults to ./mockJournal.ts's journalForRun. */
  journalForRun?: (runId: string) => readonly UndoJournalEntry[];
}

interface InternalState {
  phase: UndoPhase;
  runId: string | null;
  entries: readonly UndoJournalEntry[];
}

const INITIAL_STATE: InternalState = { phase: "closed", runId: null, entries: [] };

export function createUndoScreen(bus: BusClient, opts: CreateUndoScreenOptions = {}): UndoScreen {
  const loadJournal = opts.journalForRun ?? defaultJournalForRun;
  let state: InternalState = INITIAL_STATE;
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
      doneSummary: undoScreenStrings.doneSummary(restorableCount),
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function open(runId: string): void {
    // Newest-first, defensively re-sorted here rather than trusting the
    // fixture's (or a future real wire's) own ordering, the same distrust
    // Recorder::inverses_newest_first_seq has of raw storage order.
    const entries = [...loadJournal(runId)].sort((a, b) => b.seq - a.seq);
    state = { phase: "preview", runId, entries };
    bus.publish("undo.previewed", {
      run_id: runId,
      entries: entries.length,
      irreversible: entries.filter(isIrreversible).length,
    });
    emit();
  }

  function confirm(): void {
    if (state.phase !== "preview" || !state.runId) return;
    const narration = state.entries.map((entry) => appliedLine(entry.inverse));
    const restored = state.entries.filter((entry) => !isIrreversible(entry)).length;
    bus.publish("undo.applied", { run_id: state.runId, restored, narration });
    state = { ...state, phase: "done" };
    emit();
  }

  function close(): void {
    if (state.phase === "closed") return;
    state = INITIAL_STATE;
    emit();
  }

  function dispose(): void {
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
