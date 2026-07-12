// The tray (docs/specs/design.md section 3, Tray, BINDING: "Glyph states:
// idle outline, amber pulse recording, gray play replaying, red kill. Menu:
// the top three frecent workflows as one-click Quick Runs, then Open, Pause
// all, and a panic row.") and its notifications (FR-U8's weekly digest; a
// nudge when a run halts and needs a human, per docs/ARCHITECTURE.md's
// "Tray turns red; recovery is an explicit human resume"). Pure and DOM-free,
// same split as ui/src/runViewer/state.ts: the real OS tray icon, its real
// menu, and OS notification toasts live in ui/src-tauri, out of this lane's
// owned path (ui/src only); this computes what they should show.
//
// docs/specs/ui.md's older tray line ("status glyph: idle, running,
// halted-red") only had one non-idle state. design.md section 3 splits
// "running" into the amber "recording" (an explore/teach run under way) and
// the gray "replaying" (a saved workflow running, no AI) glyphs, the same
// mode split ui/src/runViewer/state.ts's own runChip already draws off
// (modelOn is true only for RUN_MODE_EXPLORE; replay and dry both read as
// "not exploring"). "kill" covers both a halted run (any reason) and the
// kill switch itself (docs/ARCHITECTURE.md's C20: "global panic hotkey plus
// tray button... Tray turns red"): both need a human before anything
// continues, so both paint the same red glyph; the notification each raises
// still says which one actually happened, and docs/specs/guardian.md's
// "Resume is per-run and explicit" is why there is no single "un-halt"
// action offered here for either cause.
//
// Tracks only the latest run, the same simplification
// ui/src/runViewer/state.ts makes (a run.started event fully replaces
// whatever came before); concurrent runs are not yet a UI-shell concern.
// pauseAll below inherits that same simplification: it pauses the one
// tracked run, not a real multi-run queue this shell does not have yet.

import type { BusClient } from "../bus/mockClient.ts";
import type { BusEvent, RunMode } from "../bus/types.ts";
import { RUN_MODE_EXPLORE } from "../bus/types.ts";
import { trayStrings } from "../strings/default.ts";
import { trayNotificationStrings, trayMenuStrings } from "./strings.ts";
import { frecencyScore } from "../palette/frecency.ts";
import { createMockRegistry, type MockRegistry } from "../library/mockRegistry.ts";

export type TrayGlyphState = "idle" | "recording" | "replaying" | "kill";

const GLYPH_LABELS: Record<TrayGlyphState, string> = {
  idle: trayStrings.idle,
  recording: trayMenuStrings.recordingLabel,
  replaying: trayMenuStrings.replayingLabel,
  kill: trayStrings.halted,
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

/** One entry in the menu's Quick Runs section: a saved workflow, ranked by frecency (see quickRuns() below). */
export interface TrayQuickRun {
  name: string;
  title: string;
}

export interface TraySnapshot {
  glyph: TrayGlyphState;
  glyphLabel: string;
  minutesSavedThisWeek: number;
  tooltip: string;
  notifications: TrayNotification[];
  /** Whether the click-to-open menu (design.md section 3) is currently showing. */
  menuOpen: boolean;
  /** The menu's own accessible name (its trigger button's aria-label also folds in glyphLabel/tooltip; ui/src/tray/view.ts). */
  menuLabel: string;
  quickRunsTitle: string;
  /** Up to three, highest frecency first; empty until any saved workflow has been run at least once. */
  quickRuns: TrayQuickRun[];
  quickRunsEmptyLabel: string;
  openLabel: string;
  pauseAllLabel: string;
  /** False when there is no tracked run to pause; ui/src/tray/view.ts disables the menu item rather than offering a dead click. */
  canPauseAll: boolean;
  panicLabel: string;
  panicHint: string;
}

export interface Tray {
  getSnapshot(): TraySnapshot;
  subscribe(fn: (snap: TraySnapshot) => void): () => void;
  dismissNotification(id: string): void;
  toggleMenu(): void;
  closeMenu(): void;
  /** Pauses the one tracked run, if any is currently under way (publishes run.paused). No-op otherwise. */
  pauseAll(): void;
  /** The panic row / kill switch (docs/ARCHITECTURE.md's C20): publishes killswitch.engaged. The glyph and its notification follow reactively, the same publish-then-react-off-the-bus pattern ui/src/runViewer/state.ts's stop() uses for run.halted. */
  panic(): void;
  dispose(): void;
}

export interface CreateTrayOptions {
  /** Shared with ui/src/library/state.ts's registry in ui/src/main.ts, so Quick Runs show the same plain-language titles Library does for the same workflow name. Defaults to its own seeded registry so the tray still renders standalone (tests). */
  registry?: MockRegistry;
  now?: () => number;
}

interface WorkflowUsage {
  count: number;
  lastUsedAt: number;
}

/** design.md section 3: "the top three frecent workflows." */
const MAX_QUICK_RUNS = 3;

function glyphForMode(mode: RunMode): TrayGlyphState {
  return mode === RUN_MODE_EXPLORE ? "recording" : "replaying";
}

export function createTray(bus: BusClient, opts: CreateTrayOptions = {}): Tray {
  const registry = opts.registry ?? createMockRegistry();
  const now = opts.now ?? (() => Date.now());

  let glyph: TrayGlyphState = "idle";
  // The run this tray is currently tracking, for pauseAll() to act on and
  // canPauseAll to reflect; cleared whenever there is nothing left to pause
  // (completed, halted, or the kill switch froze everything).
  let currentRunId: string | null = null;
  // Remembered because run.resumed (contracts/bus_events.md) carries no
  // mode of its own: resuming has to repaint the same recording/replaying
  // glyph the run started with, not re-derive it from a payload that does
  // not have it.
  let currentMode: RunMode | null = null;
  let minutesSavedThisWeek = 0;
  let notifications: TrayNotification[] = [];
  let notificationSeq = 0;
  let menuOpen = false;
  // Same "how often and how recently" concept design.md section 3's Palette
  // entry names ("Recents ranked by frecency"), applied here to saved-
  // workflow runs instead of palette picks; ../palette/frecency.ts's own
  // scoring function is reused rather than reimplemented, kept as this
  // module's own small map rather than that module's localStorage-backed
  // store since a palette pick and a workflow actually being run are
  // different events (this only ever counts the latter, off the same bus
  // events ui/src/dashboard/state.ts and ui/src/library/state.ts already
  // listen to for their own recent-run bookkeeping).
  const workflowUsage = new Map<string, WorkflowUsage>();
  const listeners = new Set<(snap: TraySnapshot) => void>();

  /** Same shape as ui/src/dashboard/state.ts's titleFor: the registry's plain-language description, falling back to the raw name for an unknown workflow. Kept local rather than shared, the same "small, screen-owned pure function" call dashboard/state.ts's own header comment makes for its formatRelative. */
  function titleFor(name: string): string {
    return registry.get(name)?.manifest.description || name;
  }

  function quickRuns(): TrayQuickRun[] {
    const nowMs = now();
    return Array.from(workflowUsage.entries())
      .map(([name, usage]) => ({ name, score: frecencyScore(usage, nowMs) }))
      .filter((entry) => entry.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, MAX_QUICK_RUNS)
      .map(({ name }) => ({ name, title: titleFor(name) }));
  }

  function snapshot(): TraySnapshot {
    return {
      glyph,
      glyphLabel: GLYPH_LABELS[glyph],
      minutesSavedThisWeek,
      tooltip: trayStrings.savedTimeTooltip(minutesSavedThisWeek),
      notifications,
      menuOpen,
      menuLabel: trayMenuStrings.menuLabel,
      quickRunsTitle: trayMenuStrings.quickRunsTitle,
      quickRuns: quickRuns(),
      quickRunsEmptyLabel: trayMenuStrings.quickRunsEmptyLabel,
      openLabel: trayMenuStrings.openLabel,
      pauseAllLabel: trayMenuStrings.pauseAllLabel,
      canPauseAll: currentRunId !== null,
      panicLabel: trayMenuStrings.panicLabel,
      panicHint: trayMenuStrings.panicHint,
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

  /** design.md section 3's "frecent workflows": bumps count and refreshes lastUsedAt for a workflow that just started running. Only ever called with a workflow_name (an explore/teach run has none, same guard ui/src/dashboard/state.ts and ui/src/library/state.ts use). */
  function recordWorkflowRun(name: string): void {
    const existing = workflowUsage.get(name);
    workflowUsage.set(name, { count: (existing?.count ?? 0) + 1, lastUsedAt: now() });
  }

  function handle(event: BusEvent): void {
    switch (event.topic) {
      case "run.started":
        currentRunId = event.payload.run_id;
        currentMode = event.payload.mode;
        glyph = glyphForMode(event.payload.mode);
        if (event.payload.workflow_name) recordWorkflowRun(event.payload.workflow_name);
        emit();
        return;
      case "run.resumed":
        currentRunId = event.payload.run_id;
        if (currentMode) glyph = glyphForMode(currentMode);
        emit();
        return;
      case "run.paused":
        // Still under way, waiting on a human redirect; distinct from
        // halted/kill, which need a human before anything continues at all.
        // currentRunId stays set: pauseAll publishing run.paused again for
        // an already-paused run is a harmless no-op re-send, not a bug.
        return;
      case "run.completed":
        currentRunId = null;
        currentMode = null;
        glyph = "idle";
        emit();
        return;
      case "run.halted":
        currentRunId = null;
        currentMode = null;
        glyph = "kill";
        pushNotification(trayNotificationStrings.haltedTitle, trayNotificationStrings.haltedBody);
        emit();
        return;
      case "killswitch.engaged":
        currentRunId = null;
        currentMode = null;
        glyph = "kill";
        pushNotification(trayNotificationStrings.killswitchTitle, trayNotificationStrings.killswitchBody);
        emit();
        return;
      case "killswitch.released":
        glyph = "idle";
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

  function toggleMenu(): void {
    menuOpen = !menuOpen;
    emit();
  }

  function closeMenu(): void {
    if (!menuOpen) return;
    menuOpen = false;
    emit();
  }

  function pauseAll(): void {
    if (!currentRunId) return;
    bus.publish("run.paused", { run_id: currentRunId, by: "human" });
  }

  function panic(): void {
    bus.publish("killswitch.engaged", { at_ms: now() });
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    dismissNotification,
    toggleMenu,
    closeMenu,
    pauseAll,
    panic,
    dispose() {
      unsubscribe();
      listeners.clear();
    },
  };
}
