// Tour and contextual hint strings for default mode. All strings must use
// only the user-facing vocabulary from contracts/microcopy_glossary.json.

export const tourStrings = {
  // Main tour callouts for each step
  paletteTitle: "Tell it what to do",
  paletteMessage: "Type here to teach it from what is on your screen right now",

  runViewerTitle: "Watch it work",
  runViewerMessage: "See each step as it runs and what it finds on your screen",

  libraryTitle: "Your saved workflows",
  libraryMessage: "Find and run your saved workflows from here anytime",
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
