// The target-app picker (ADR 0003, A1 target-app selection): the step between
// the palette's "Teach this" row and the teach run itself. When the palette is
// up, Operant is the foreground window, so teaching against "the foreground
// app" would teach against Operant. This picker fixes that: after the goal is
// committed, the person chooses which real, already-open app to teach against.
//
// Pure and DOM-free, the same logic/DOM split ui/src/palette/state.ts and every
// other screen in ui/src use: ui/src/palette/targetAppView.ts only ever renders
// getSnapshot() and forwards key presses back through
// open/setWindows/moveSelection/confirm/close, and ui/src/main.ts owns fetching
// the window list and turning a confirm into the actual teach run. Selection
// tracks a row's own id (never an index), the same reason state.ts does: the row
// list is rebuilt from the fetched windows and an index would drift.
//
// Only reached on the real core path. In Demo/off-Tauri the window list is
// unavailable (ui/src/bus/commands.ts's listWindows returns null), so main.ts
// keeps the current stub-foreground teach path and never opens this picker.

import { getTargetAppStrings } from "./strings.ts";

/**
 * One open window as the core reports it: the process name (what a teach run
 * needs as its window_process context), the human-readable title, and a stable
 * id. The list arrives z-ordered topmost-first with Operant's own window
 * already excluded, so windows[0] is the app the person was last in.
 */
export interface TargetWindow {
  process: string;
  title: string;
  id: string;
}

/**
 * The id of the special first row that resolves to windows[0] (the topmost
 * non-Operant window). A stable, DOM-id-safe sentinel, kept distinct from any
 * real window id so confirm can tell the two apart.
 */
export const FRONT_APP_ROW_ID = "__front_app__";

export interface TargetAppRow {
  id: string;
  /** Primary line: the front-app row's plain-language label, or a window's own title. */
  title: string;
  /** Secondary line: a window's process name (or, on the front-app row, which app that resolves to). */
  subtitle?: string;
  /** The process name a teach run receives as its window_process when this row is confirmed. */
  process: string;
  /** True only for the special "use the app I have in front" row, so the view can mark it distinctly. */
  frontApp: boolean;
}

export interface TargetAppSnapshot {
  open: boolean;
  /** True while the window list is still being fetched (open, but setWindows has not arrived yet). */
  loading: boolean;
  /** The goal being taught, carried through so confirm hands it back with the chosen process, and so a cancel can return to it. */
  goal: string;
  rows: TargetAppRow[];
  selectedId: string | null;
  overlayLabel: string;
  heading: string;
  loadingLabel: string;
  emptyLabel: string;
  confirmHint: string;
  cancelHint: string;
}

export interface TargetAppConfirm {
  goal: string;
  windowProcess: string;
}

export interface TargetAppPicker {
  getSnapshot(): TargetAppSnapshot;
  subscribe(fn: (snap: TargetAppSnapshot) => void): () => void;
  /** Opens the picker for a goal, in the loading state until setWindows arrives. */
  open(goal: string): void;
  /**
   * Supplies the fetched window list: builds the rows (a front-app row plus one
   * per window) and defaults the selection back to the front-app row, which
   * resolves to windows[0]. An empty list leaves no rows to confirm.
   */
  setWindows(windows: readonly TargetWindow[]): void;
  moveSelection(delta: 1 | -1): void;
  /**
   * Confirms `rowId` (a mouse click naming an exact row) or the current
   * selection (a keyboard Enter). Returns the goal and the chosen row's process,
   * or null when there is nothing to confirm. Closes the picker on success.
   */
  confirm(rowId?: string): TargetAppConfirm | null;
  close(): void;
  dispose(): void;
}

export function createTargetAppPicker(): TargetAppPicker {
  let open = false;
  let goal = "";
  let windows: readonly TargetWindow[] | null = null;
  let selectedId: string | null = null;
  const listeners = new Set<(snap: TargetAppSnapshot) => void>();

  function buildRows(): TargetAppRow[] {
    if (!windows || windows.length === 0) return [];
    const strings = getTargetAppStrings();
    const front = windows[0];
    // The pre-selected smart default: teach against whatever app the person had
    // in front (windows[0]), never Operant. Its subtitle names that app so the
    // choice is not blind.
    const frontRow: TargetAppRow = {
      id: FRONT_APP_ROW_ID,
      title: strings.frontApp,
      subtitle: front.title,
      process: front.process,
      frontApp: true,
    };
    // Then every open window, so the person can pick a specific one instead.
    const windowRows: TargetAppRow[] = windows.map((w) => ({
      id: w.id,
      title: w.title,
      subtitle: w.process,
      process: w.process,
      frontApp: false,
    }));
    return [frontRow, ...windowRows];
  }

  /** Keeps selectedId pointing at a row that still exists, defaulting to the top (front-app) row otherwise. */
  function resolveSelection(rows: TargetAppRow[]): string | null {
    if (rows.length === 0) return null;
    if (selectedId !== null && rows.some((r) => r.id === selectedId)) return selectedId;
    return rows[0].id;
  }

  function snapshot(): TargetAppSnapshot {
    const rows = buildRows();
    selectedId = open ? resolveSelection(rows) : null;
    const strings = getTargetAppStrings();
    return {
      open,
      loading: open && windows === null,
      goal,
      rows,
      selectedId,
      overlayLabel: strings.overlayLabel,
      heading: strings.heading,
      loadingLabel: strings.loading,
      emptyLabel: strings.empty,
      confirmHint: strings.confirmHint,
      cancelHint: strings.cancelHint,
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
    open(nextGoal) {
      open = true;
      goal = nextGoal;
      windows = null;
      selectedId = null;
      emit();
    },
    setWindows(next) {
      windows = next;
      selectedId = null; // default back to the front-app row (windows[0])
      emit();
    },
    moveSelection(delta) {
      const rows = buildRows();
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
    confirm(rowId) {
      const rows = buildRows();
      const targetId = rowId ?? selectedId;
      const row = rows.find((r) => r.id === targetId) ?? rows[0];
      if (!row) return null;
      const result: TargetAppConfirm = { goal, windowProcess: row.process };
      open = false;
      windows = null;
      selectedId = null;
      emit();
      return result;
    },
    close() {
      open = false;
      windows = null;
      selectedId = null;
      emit();
    },
    dispose() {
      listeners.clear();
    },
  };
}
