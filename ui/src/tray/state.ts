// The tray (docs/specs/ui.md: "tray (status glyph: idle, running,
// halted-red, saved-time tooltip)") and its notifications (FR-U8's weekly
// digest; a nudge when a run halts and needs a human, per
// docs/ARCHITECTURE.md's "Tray turns red; recovery is an explicit human
// resume"). Pure and DOM-free, same split as ui/src/runViewer/state.ts: the
// real OS tray icon and OS notification toasts live in ui/src-tauri, out of
// this lane's owned path (ui/src only); this computes what they should show.
//
// Tracks only the latest run, the same simplification
// ui/src/runViewer/state.ts makes (a run.started event fully replaces
// whatever came before); concurrent runs are not yet a UI-shell concern.

import type { BusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { trayStrings } from "../strings/default.ts";
import { trayNotificationStrings } from "./strings.ts";

export type TrayGlyphState = "idle" | "running" | "halted-red";

const GLYPH_LABELS: Record<TrayGlyphState, string> = {
  idle: trayStrings.idle,
  running: trayStrings.running,
  "halted-red": trayStrings.halted,
};

export interface TrayNotification {
  id: string;
  title: string;
  body: string;
  /**
   * Set only for the weekly time-saved digest (FR-U8), so
   * ui/src/tray/view.ts can give it a restyled, mono tabular-figure stat
   * (F11, design.md section 2's Type: "Numeric and step data: IBM Plex
   * Mono, tabular figures... for timers, counts") distinct from a plain
   * alert like run.halted's. Captured at push time rather than read back
   * from the live minutesSavedThisWeek/tooltip above deliberately: if a
   * second weekly digest arrives before the first is dismissed, each one
   * must keep showing its own week's figure, not both snapping to
   * whichever is most recent. Every other notification leaves this
   * undefined and keeps the plain, unrestyled treatment.
   */
  minutesSaved?: number;
}

export interface TraySnapshot {
  glyph: TrayGlyphState;
  glyphLabel: string;
  minutesSavedThisWeek: number;
  tooltip: string;
  notifications: TrayNotification[];
}

export interface Tray {
  getSnapshot(): TraySnapshot;
  subscribe(fn: (snap: TraySnapshot) => void): () => void;
  dismissNotification(id: string): void;
  dispose(): void;
}

export function createTray(bus: BusClient): Tray {
  let glyph: TrayGlyphState = "idle";
  let minutesSavedThisWeek = 0;
  let notifications: TrayNotification[] = [];
  let notificationSeq = 0;
  const listeners = new Set<(snap: TraySnapshot) => void>();

  function snapshot(): TraySnapshot {
    return {
      glyph,
      glyphLabel: GLYPH_LABELS[glyph],
      minutesSavedThisWeek,
      tooltip: trayStrings.savedTimeTooltip(minutesSavedThisWeek),
      notifications,
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function pushNotification(title: string, body: string, minutesSaved?: number): void {
    notificationSeq += 1;
    notifications = [...notifications, { id: `n${notificationSeq}`, title, body, minutesSaved }];
  }

  function handle(event: BusEvent): void {
    switch (event.topic) {
      case "run.started":
      case "run.resumed":
        glyph = "running";
        emit();
        return;
      case "run.paused":
        // Still under way, waiting on a human redirect; distinct from
        // halted, which needs a human before anything continues at all.
        return;
      case "run.completed":
        glyph = "idle";
        emit();
        return;
      case "run.halted":
        glyph = "halted-red";
        pushNotification(trayNotificationStrings.haltedTitle, trayNotificationStrings.haltedBody);
        emit();
        return;
      case "metrics.week.rolled":
        minutesSavedThisWeek = event.payload.minutes_saved_total;
        pushNotification(
          trayNotificationStrings.weeklyDigestTitle,
          trayStrings.savedTimeTooltip(minutesSavedThisWeek),
          minutesSavedThisWeek,
        );
        emit();
        return;
      default:
        return;
    }
  }

  const unsubscribe = bus.subscribe("*", handle);

  function dismissNotification(id: string): void {
    const before = notifications.length;
    notifications = notifications.filter((n) => n.id !== id);
    if (notifications.length !== before) emit();
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    dismissNotification,
    dispose() {
      unsubscribe();
      listeners.clear();
    },
  };
}
