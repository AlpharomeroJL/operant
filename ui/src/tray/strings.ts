// Default-mode strings for the tray beyond ui/src/strings/default.ts's
// trayStrings (idle, running, halted, savedTimeTooltip). Extended for
// docs/specs/design.md section 3, Tray (BINDING): the amber/gray glyph-state
// labels the spec's own prose names ("amber pulse recording," "gray play
// replaying"), and the click-to-open menu's copy (the top three frecent
// workflows as Quick Runs, then Open, Pause all, and the panic row
// docs/ARCHITECTURE.md's C20 calls "global panic hotkey plus tray button").
//
// Kept in this module-local file rather than ui/src/strings/default.ts
// because nothing outside ui/src/tray needs any of it (runStateStrings, the
// one other consumer of the older trayStrings, has its own idle/running/
// paused/halted/done vocabulary and no notion of recording vs replaying);
// the same default.ts-vs-screen's-own-strings.ts split ui/src/library/
// strings.ts and ui/src/dashboard/strings.ts already use.

export const trayNotificationStrings = {
  haltedTitle: "Operant stopped",
  haltedBody: "It stopped and needs you before it can continue.",
  weeklyDigestTitle: "Your weekly time saved",
  // The unit label beside the restyled digest's mono stat figure (F11,
  // ui/src/tray/view.ts): short by design, next to a number, not a sentence
  // (design.md section 4's sentence-case rule governs prose, not a unit tag).
  digestUnit: "min",
  dismiss: "Dismiss",
  // docs/ARCHITECTURE.md's C20 guardian set: "global panic hotkey plus tray
  // button... freezes input synthesis... Tray turns red; recovery is an
  // explicit human resume." Distinct from haltedTitle/haltedBody above: a
  // halted run is one run stopping on its own (a failed checkpoint, a
  // person, an error); this is every run freezing at once because someone
  // used the panic control. Recovery is per-run (docs/specs/guardian.md:
  // "Resume is per-run and explicit"), not a single tray action, so this
  // does not point at one that does not exist yet.
  killswitchTitle: "Emergency stop engaged",
  killswitchBody: "Every run is frozen until you resume it by hand.",
};

// The tray's click-to-open menu (design.md section 3, Tray: "Menu: the top
// three frecent workflows as one-click Quick Runs, then Open, Pause all,
// and a panic row").
export const trayMenuStrings = {
  // Short glyph-state labels for ui/src/tray/state.ts's GLYPH_LABELS.
  // "Idle" and "Stopped, needs you" already exist as ui/src/strings/
  // default.ts's trayStrings.idle/halted (kept there since runStateStrings,
  // a different screen's catalog, reads them too); recording/replaying are
  // new and tray-only. "Recording" reuses the exact word ui/src/runViewer's
  // own REC chip already uses for the same concept (runViewerStrings.
  // recChipAria: "Recording, teaching a new workflow").
  recordingLabel: "Recording",
  replayingLabel: "Replaying",
  menuLabel: "Tray menu",
  quickRunsTitle: "Quick runs",
  // design.md section 4: "Empty states invite one specific action."
  quickRunsEmptyLabel: "Run a saved workflow once, and it shows up here.",
  openLabel: "Open",
  pauseAllLabel: "Pause all",
  panicLabel: "Stop everything",
  panicHint: "Freezes every run right now, the same as the emergency stop shortcut.",
};
