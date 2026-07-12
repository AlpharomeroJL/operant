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
// section 3's nav map (Dashboard, Library, Runs, Settings). "Runs" covers the
// palette-plus-run-viewer pair together (the command palette and flight
// recorder), since neither is useful without the other; a future packet may
// split the palette into its own floating overlay per design.md, at which
// point this label stops covering it.
export const navStrings = {
  dashboard: "Dashboard",
  runs: "Runs",
  library: "Library",
  settings: "Settings",
};

// Home dashboard (docs/specs/design.md section 3): "the new default window
// view." Its real content (a plain-language hero line, sparkline, Up next,
// Recent runs) is a later packet's job; this is D1 tokens-and-shell's
// minimal themed placeholder so the nav has somewhere to route to in the
// meantime (see ui/src/main.ts).
export const dashboardStrings = {
  title: "Dashboard",
  placeholderBody: "Your weekly summary is coming here soon. Head to Library to run a saved workflow, or Runs to watch one live.",
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
