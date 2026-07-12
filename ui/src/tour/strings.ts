// Tour and contextual hint strings for default mode. All strings must use
// only the user-facing vocabulary from contracts/microcopy_glossary.json.

// H1: re-pointed at the new screen map (docs/specs/design.md section 3's nav
// -- Dashboard, Library, Runs, Settings) so the tour completes on the new
// nav instead of the pre-redesign steps ("palette" was an inline part of
// the run viewer screen before it became its own floating overlay,
// ui/src/palette/; "runViewer" is renamed "runs" to match ui/src/main.ts's
// own Screen type and the nav button labels themselves, ui/src/strings/
// default.ts's navStrings).
export const tourStrings = {
  // Main tour callouts for each step, walked in nav order.
  dashboardTitle: "Start here",
  dashboardMessage: "Teach your first workflow from the dashboard, or press Ctrl+K anytime to tell it what to do",

  libraryTitle: "Your saved workflows",
  libraryMessage: "Find and run your saved workflows from here anytime",

  runsTitle: "Watch it work",
  runsMessage: "See each step as it runs and what it finds on your screen",

  settingsTitle: "Make it yours",
  settingsMessage: "Tune thinking engines, voice, and privacy, or switch between light and dark",
};

export const hintStrings = {
  // Contextual hints that retire after first use
  paletteHint: "Press Enter to teach it",
  runViewerPauseHint: "Pause here if you need to tell it something different",
  runViewerResumeHint: "Click to continue running",
  libraryRunHint: "Click Run to use a saved workflow",
  libraryScheduleHint: "Set up a schedule to run it automatically",
  libraryExplainHint: "See exactly what this workflow does",
};
