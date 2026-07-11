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

// Labels for the shell's top-level screen switcher. docs/specs/ui.md lists
// its screens as a flat set (tray, command palette, run viewer, workflow
// library, grant prompt, drift card, settings, doctor) without dictating how
// a person moves between them; "Run" below covers the palette-plus-run-
// viewer pair together, since neither is useful without the other.
export const navStrings = {
  run: "Run",
  library: "Library",
  settings: "Settings",
};

export const trayStrings = {
  idle: "Idle",
  running: "Running",
  halted: "Stopped, needs you",
  savedTimeTooltip: (minutes: number) => `Saved about ${minutes} minutes this week`,
};

export const paletteStrings = {
  placeholder: "Tell it what to do",
  submit: "Teach it",
  hint: "Press Enter to start teaching it from what's on screen right now",
};

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
  advancedToggle: "Advanced",
};

export const doctorStrings = {
  title: "Check my setup",
  fixButton: "Fix it",
};
