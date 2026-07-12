// User-facing strings for default mode.
//
// Every string in this file must use only the user-facing vocabulary from
// contracts/microcopy_glossary.json. scripts/microcopy_lint.mjs enforces
// this in CI (just check-microcopy) by scanning quoted string literals
// under ui/src for glossary internal terms. Jargon belongs behind the
// Advanced toggle: see ui/src/advanced/strings.ts.
//
// Screens covered here mirror docs/specs/ui.md: tray, command palette, run
// viewer, workflow library, grant prompt, drift card, settings, doctor.

export const commonStrings = {
  appName: "Operant",
  loading: "One moment",
  errorGeneric: "Something went wrong. Try again, or check your setup.",
};

// Labels for the shell's top-level screen switcher: docs/specs/design.md
// section 3's nav map (Dashboard, Library, Runs, Settings). "Runs" is the
// flight recorder screen (the run viewer); the command palette that used to
// live inline on this screen is now its own global floating overlay
// (ui/src/palette/, reachable from any screen via Ctrl+K/Cmd+K), per
// design.md section 3's Palette entry.
export const navStrings = {
  dashboard: "Dashboard",
  runs: "Runs",
  library: "Library",
  settings: "Settings",
};

// Home dashboard (docs/specs/design.md section 3): "the new default window
// view." title/placeholderBody were D1 tokens-and-shell's minimal themed
// placeholder before this screen had real content (ui/src/main.ts's
// op-screen-dashboard); placeholderBody is unused now that D4 (ui/src/
// dashboard/) fills it in, but stays defined, unrenamed, per this file's
// append-only rule during the campaign. Everything below placeholderBody is
// D4's: the hero line plus its sparkline's text equivalent, the Up
// next/Recent runs section titles and their own empty notes, the quiet
// first-run invite (design.md: "a quiet empty state that invites teaching
// the first workflow"), and each recent run's one-line outcome (the last
// two echo design.md section 3's own Toasts example, "Run complete, 14
// steps"). ui/src/dashboard/strings.ts holds this screen's smaller, more
// mechanical extras (today/tomorrow words, the weekday-name list, the
// status dot's hidden text), the same split ui/src/library/strings.ts and
// ui/src/tray/strings.ts already use against this file.
export const dashboardStrings = {
  title: "Dashboard",
  placeholderBody: "Your weekly summary is coming here soon. Head to Library to run a saved workflow, or Runs to watch one live.",
  heroLine: (hoursPhrase: string) => `Operant saved you ${hoursPhrase} this week`,
  sparklineSummary: (values: string) => `Minutes saved by week, oldest to newest: ${values}.`,
  upNextTitle: "Up next",
  upNextEmpty: "Nothing scheduled yet.",
  recentRunsTitle: "Recent runs",
  recentRunsEmpty: "No runs yet.",
  emptyInvite: "Nothing here yet. Teach Operant its first workflow from the command palette, or run one from Library, and it shows up here.",
  outcomeOk: (steps: number) => `Run complete, ${steps} step${steps === 1 ? "" : "s"}`,
  outcomeFailed: "Run did not finish",
};

// The shell header's dark/light/system control (docs/specs/design.md section
// 3's Settings > Appearance section names this same three-way choice; this
// packet wires it as a single compact cycling button in the header rather
// than a settings picker, see ui/src/theme/store.ts).
export const themeToggleStrings = {
  dark: "Dark",
  light: "Light",
  system: "Match system",
  hint: "Switch the color theme: dark, light, or match system",
};

export const trayStrings = {
  idle: "Idle",
  running: "Running",
  halted: "Stopped, needs you",
  savedTimeTooltip: (minutes: number) => `Saved about ${minutes} minutes this week`,
};

// Palette strings are now in the locale catalog; import from there.
// This is re-exported here for backward compatibility with code that imports
// from ui/src/strings/default.ts.
import { getLocaleCatalog } from "../locales/index.ts";

export const paletteStrings = getLocaleCatalog().paletteStrings;

export const runViewerStrings = {
  title: "What it's doing",
  modelOn: "Thinking live",
  modelOff: "Running from memory, no thinking needed",
  stop: "Stop",
  pause: "Pause",
  resume: "Resume",
  intervenePlaceholder: "Tell it what to do differently",
  interveneSubmit: "Send",
  // Shown for a step row the shell cannot describe yet (for example, one
  // that arrived without the detail the plain-English renderer needs). n is
  // the step's 1-based position in the list.
  stepFallback: (n: number) => `Step ${n}`,
  stepStatus: {
    pending: "Waiting",
    ok: "Done",
    failed: "Did not work",
    retried: "Trying again",
  },
  // Per-step duration, shown in mono on the row (docs/specs/design.md section
  // 3: "duration in mono"). ms is the wall time from run.step.executed.
  stepDuration: (ms: number) => `${ms} ms`,
  // The flight recorder's mode chips (docs/specs/design.md section 3). A teach
  // run shows the amber REC chip; its tooltip is one of exactly two places the
  // word AI appears in-app (docs/specs/design.md section 4's copy rule). A run
  // of a saved workflow shows the quiet gray chip below; design.md fixes its
  // exact wording as the one sanctioned place a saved-workflow run is described
  // as using no AI, only ever to say it is not using one (section 4). Note:
  // scripts/microcopy_lint.mjs flags the word in this fixed phrase generically;
  // design.md is binding here and wins until amended (see this packet's notes).
  recChip: "REC",
  recChipAria: "Recording, teaching a new workflow",
  recChipTooltip: "Operant is using your AI engine to learn this",
  replayChip: "no AI, exact replay",
  replayChipTooltip: "Playing back saved steps exactly, with no thinking",
  // The filmstrip of redacted step thumbnails above the step list
  // (docs/specs/design.md section 3). Each frame's accessible name is the
  // step's own plain-English sentence so the strip is navigable by keyboard
  // and screen reader, not just by sight.
  filmstripLabel: "Steps so far",
  frameAria: (n: number, sentence: string) => `Step ${n}: ${sentence}`,
  // The thumbnails are placeholders on purpose: no captured pixels ship
  // (docs/specs/design.md section 3, "the point is no sensitive pixels
  // ship"). This visually-hidden note tells a screen-reader user so.
  thumbnailRedacted: "Redacted preview",
  // A safety check that did not pass, shown inline in the step list as a card
  // rather than a modal (docs/specs/design.md section 3). Copy follows section
  // 4's error rule: what happened, then one thing to do, calm, no apology.
  gateFailedTitle: "A safety check didn't pass",
  gateFailedBody: "Operant stopped before finishing this step. Look at what it was about to do, then decide whether to keep going.",
};

// The run viewer's own human-language run states. idle and running and
// halted mirror the tray glyph (trayStrings); done and paused are specific
// to this screen, since the tray spec (docs/specs/ui.md) does not need
// them. Kept as one small catalog so every place that shows a run's state
// reads from the same five words.
export const runStateStrings = {
  idle: trayStrings.idle,
  running: trayStrings.running,
  paused: "Paused, waiting for you",
  halted: trayStrings.halted,
  done: "Done",
};

export const workflowLibraryStrings = {
  title: "Your saved workflows",
  lastRun: (when: string) => `Last run ${when}`,
  minutesSaved: (minutes: number) => `${minutes} minutes saved`,
  run: "Run",
  schedule: "Schedule",
  explain: "Explain",
  empty: "No workflows yet. Teach it something to save your first one.",
  // D4's restyle (docs/specs/design.md section 3, Library: "Search filters
  // live"). searchLabel is the input's accessible name; searchPlaceholder is
  // its short visible placeholder; noMatches is shown instead of `empty`
  // above when a search query matches nothing in an otherwise non-empty
  // library, so typing a search never reads as "you have no workflows."
  searchLabel: "Search your workflows",
  searchPlaceholder: "Search",
  noMatches: "No workflows match your search.",
};

export const grantPromptStrings = {
  title: "This workflow needs permission",
  allow: "Allow",
  deny: "Deny",
};

export const driftCardStrings = {
  title: "Something on screen moved",
  question: "Update the workflow?",
  update: "Update",
  notNow: "Not now",
};

export const settingsStrings = {
  title: "Settings",
  modelSectionTitle: "Model",
  voiceSectionTitle: "Voice",
  killSwitchSectionTitle: "Emergency stop shortcut",
  privacySectionTitle: "Privacy",
  watchAndSuggestToggle: "Watch for repeated actions and offer to help",
  purgeButton: "Delete what's been watched",
  backupSectionTitle: "Backup and export",
  updatesSectionTitle: "Updates",
  advancedToggle: "Advanced",
};

export const doctorStrings = {
  title: "Check my setup",
  fixButton: "Fix it",
};

// The Undo screen's entry points (docs/specs/design.md section 3's Undo
// screen and Toasts sections; section 4 fixes this exact button verb:
// "Verbs on buttons say what happens: ... 'Undo this run'"). ui/src/main.ts
// wires this one shared label at both call sites (the button beside a
// completed run in the run viewer, and the action on the toast a completed
// run raises) so neither drifts from the other or from ui/src/undo/
// strings.ts's own screen title, which uses the same words. Everything else
// the screen itself shows lives in that module's own strings.ts, the same
// split as ui/src/library/strings.ts and ui/src/tray/strings.ts against
// this file.
export const undoEntryStrings = {
  undoThisRun: "Undo this run",
};
