// The palette's stateful controller: open/closed, the typed query, which
// row is currently selected, and the grouped/ranked results themselves
// (delegated to ./catalog.ts). Pure and DOM-free, same split as
// ui/src/runViewer/state.ts: ui/src/palette/view.ts only ever renders
// getSnapshot()'s output and forwards key presses back through
// open/close/setQuery/moveSelection/commit below. ui/src/main.ts owns
// turning a commit into an actual saved-workflow run, grant check, screen
// switch, or teach run: the bus/registry/screen-routing logic this module
// deliberately does not know about, the same seam ui/src/grants/state.ts's
// onAllow/onDeny and ui/src/library/state.ts's onScheduleResolved use.
//
// Selection tracks a row's own id, not its position in the list (the same
// choice ui/src/runViewer/state.ts makes with activeStepId rather than an
// index): the moment a keystroke changes the query, or the registry hands
// back a fresh workflow list, every row's position can shift, and an index
// would then silently point at a different row than the one the person was
// actually looking at.

import { matchEntries, type PaletteEntry, type PaletteResults, type PaletteRow, type PaletteRowKind } from "./catalog.ts";
import { createFrecencyStore, type FrecencyStore } from "./frecency.ts";
import { getPaletteStrings } from "./strings.ts";

export type PaletteIntent = "run" | "preview" | "details";

export interface PaletteCommit {
  intent: PaletteIntent;
  row: PaletteRow;
}

export interface PaletteSnapshot extends PaletteResults {
  open: boolean;
  query: string;
  selectedId: string | null;
  overlayLabel: string;
  inputLabel: string;
  placeholder: string;
  footer: { run: string; preview: string; details: string };
}

export interface CreatePaletteControllerOptions {
  frecency?: FrecencyStore;
  now?: () => number;
}

// Only a workflow row has anything to preview (dry run) or show details
// for: an action or a setting entry just does the one thing it does, and a
// teach row has not been saved as anything yet to preview or explain.
const INTENT_ALLOWED_KINDS: Record<PaletteIntent, ReadonlySet<PaletteRowKind>> = {
  run: new Set<PaletteRowKind>(["workflow", "action", "setting", "teach"]),
  preview: new Set<PaletteRowKind>(["workflow"]),
  details: new Set<PaletteRowKind>(["workflow"]),
};

export interface PaletteController {
  getSnapshot(): PaletteSnapshot;
  subscribe(fn: (snap: PaletteSnapshot) => void): () => void;
  open(): void;
  close(): void;
  setQuery(text: string): void;
  /** Refreshes the catalog (ui/src/main.ts calls this from registry.subscribe, so a newly taught workflow shows up next time the palette opens). */
  setEntries(entries: readonly PaletteEntry[]): void;
  moveSelection(delta: 1 | -1): void;
  /**
   * Commits a row for `intent`: `rowId` when given (a mouse click naming an
   * exact row, which may not be the one arrow keys last highlighted),
   * otherwise the currently selected row. Returns null when there is
   * nothing to commit, or when `intent` does not apply to that row's kind
   * (./state.test.ts). Recording frecency and closing the palette both
   * happen here, not in the caller, so ui/src/main.ts's handler only ever
   * has to decide what the commit *does*, never whether the palette should
   * still be open afterward.
   */
  commit(intent: PaletteIntent, rowId?: string): PaletteCommit | null;
  dispose(): void;
}

export function createPaletteController(opts: CreatePaletteControllerOptions = {}): PaletteController {
  const frecency = opts.frecency ?? createFrecencyStore(opts.now ? { now: opts.now } : {});
  let entries: readonly PaletteEntry[] = [];
  let open = false;
  let query = "";
  let selectedId: string | null = null;
  const listeners = new Set<(snap: PaletteSnapshot) => void>();

  function computeResults(): PaletteResults {
    const strings = getPaletteStrings();
    return matchEntries(entries, query, frecency, {
      groupWorkflows: strings.groupWorkflows,
      groupActions: strings.groupActions,
      groupRecent: strings.groupRecent,
      teachThis: strings.teachThis,
      teachHint: strings.hint,
    });
  }

  /** Keeps `selectedId` pointing at a row that actually exists in `rows`, defaulting to the top row whenever it does not (a fresh query, a just-opened palette, or the previously selected row having scrolled out of the result set entirely). */
  function resolveSelection(rows: PaletteRow[]): string | null {
    if (rows.length === 0) return null;
    if (selectedId !== null && rows.some((r) => r.id === selectedId)) return selectedId;
    return rows[0].id;
  }

  function snapshot(): PaletteSnapshot {
    const results = computeResults();
    // Selection only means anything while the palette is actually open (it
    // exists to drive arrow-key navigation and commit, both open-only
    // actions); resolving it while closed would silently pre-select
    // whatever the root view's top row happens to be the instant any other
    // state (entries, frecency) changes, well before anyone opened
    // anything.
    selectedId = open ? resolveSelection(results.rows) : null;
    const strings = getPaletteStrings();
    return {
      ...results,
      open,
      query,
      selectedId,
      overlayLabel: strings.overlayLabel,
      inputLabel: strings.placeholder,
      placeholder: strings.placeholder,
      footer: { run: strings.footerRun, preview: strings.footerPreview, details: strings.footerDetails },
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    open() {
      if (open) return;
      open = true;
      query = "";
      selectedId = null;
      emit();
    },
    close() {
      if (!open) return;
      open = false;
      query = "";
      selectedId = null;
      emit();
    },
    setQuery(text) {
      if (text === query) return;
      query = text;
      selectedId = null; // jump back to the top result, the same convention every command palette uses
      emit();
    },
    setEntries(next) {
      entries = next;
      emit();
    },
    moveSelection(delta) {
      const rows = computeResults().rows;
      if (rows.length === 0) {
        selectedId = null;
        return;
      }
      const currentIndex = Math.max(
        0,
        rows.findIndex((r) => r.id === selectedId),
      );
      const nextIndex = (currentIndex + delta + rows.length) % rows.length;
      selectedId = rows[nextIndex].id;
      emit();
    },
    commit(intent, rowId) {
      const rows = computeResults().rows;
      const targetId = rowId ?? selectedId;
      const row = rows.find((r) => r.id === targetId) ?? rows[0];
      if (!row) return null;
      if (!INTENT_ALLOWED_KINDS[intent].has(row.kind)) return null;

      if (row.kind !== "teach") frecency.record(row.id);
      open = false;
      query = "";
      selectedId = null;
      emit();
      return { intent, row };
    },
    dispose() {
      listeners.clear();
    },
  };
}
