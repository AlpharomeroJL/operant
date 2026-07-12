// The bottom-right toast system (docs/specs/design.md section 3's Toasts:
// "Bottom-right, one line, verb-first... Amber only when an action is
// invited"). Pure and DOM-free, same split as ui/src/tray/state.ts:
// ui/src/toasts/view.ts is the only thing that touches the DOM.
//
// F1 first wired a single toast directly into ui/src/main.ts to unblock the
// undo entry point before this lane existed: a module-level activeToast
// variable plus inline renderToast()/dismissToast() functions, driven by an
// inline bus.subscribe("run.completed", ...) sitting in main.ts itself
// rather than in a state module of its own, the one screen whose bus wiring
// was not already split out this way. This module is that same behavior,
// consolidated into the state/view split every other screen already uses
// (ui/src/tray/state.ts's createTray(bus) is the closest sibling); F11
// deletes main.ts's own copies and instantiates this instead.
//
// Tracks only the latest toast, the same "one at a time" simplification
// ui/src/tray/state.ts's run-tracking and ui/src/runViewer/state.ts's
// latest-run tracking make elsewhere: design.md's Toasts section describes
// a single bottom-right line, never a stack, and there is exactly one
// trigger (run.completed) today.
//
// Bus-driven only, deliberately: like ui/src/tray/state.ts's own
// pushNotification, nothing here is exposed for an outside caller to push a
// toast directly. A future screen that wants one adds a case to handle()
// below (or, if that stops fitting, widens this module's own public
// surface) rather than main.ts growing a second ad-hoc toast the way F1's
// did.

import type { BusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { dashboardStrings, undoEntryStrings } from "../strings/default.ts";

export interface ToastAction {
  /** Verb-first per design.md section 4 ("Undo this run", not "Undo"). */
  label: string;
}

export interface Toast {
  id: string;
  /** One line, verb-first (design.md section 3: "Saved as workflow", "Run
   * complete, 14 steps"). */
  message: string;
  /** Amber only because an action is invited (design.md section 3);
   * undefined for a message-only toast, which ui/src/toasts/view.ts paints
   * with plain ink instead of spending the signal color on it (design.md
   * section 1's one-warm-color rule). */
  action?: ToastAction;
  /** Which run the action (if any) applies to: what ui/src/main.ts's
   * onAction callback opens the undo screen for. Undefined whenever action
   * is, so the view never has an action with nothing for it to act on. */
  runId?: string;
}

export interface ToastSnapshot {
  toast: Toast | null;
}

export interface Toasts {
  getSnapshot(): ToastSnapshot;
  subscribe(fn: (snap: ToastSnapshot) => void): () => void;
  /** Clears the current toast, if any. Called once its own action is taken
   * (ui/src/main.ts, same as an outcome-driven auto-clear elsewhere in this
   * app) or a screen otherwise decides it no longer needs showing. */
  dismiss(): void;
  dispose(): void;
}

export function createToasts(bus: BusClient): Toasts {
  let toast: Toast | null = null;
  let seq = 0;
  const listeners = new Set<(snap: ToastSnapshot) => void>();

  function snapshot(): ToastSnapshot {
    return { toast };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function show(input: { message: string; action?: ToastAction; runId?: string }): void {
    seq += 1;
    toast = { id: `toast${seq}`, message: input.message, action: input.action, runId: input.runId };
    emit();
  }

  function dismiss(): void {
    if (!toast) return;
    toast = null;
    emit();
  }

  /**
   * design.md section 3's own example ("Run complete, 14 steps") is this
   * exact outcome; the message text reuses dashboardStrings' existing
   * run-outcome copy (ui/src/dashboard/view.ts's own recent-runs rows say
   * the same thing) rather than inventing a second wording for it, same as
   * F1's version did. Both outcomes still invite "Undo this run": even a
   * run that did not finish may have executed steps worth reversing.
   */
  function handle(event: BusEvent): void {
    switch (event.topic) {
      case "run.completed": {
        const { run_id, outcome, steps } = event.payload;
        show({
          message: outcome === "ok" ? dashboardStrings.outcomeOk(steps) : dashboardStrings.outcomeFailed,
          action: { label: undoEntryStrings.undoThisRun },
          runId: run_id,
        });
        return;
      }
      default:
        return;
    }
  }

  const unsubscribe = bus.subscribe("*", handle);

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    dismiss,
    dispose() {
      unsubscribe();
      listeners.clear();
    },
  };
}
