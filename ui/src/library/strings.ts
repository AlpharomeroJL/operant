// Extra default-mode strings for the workflow library, beyond
// ui/src/strings/default.ts's workflowLibraryStrings (title, lastRun,
// minutesSaved, run, schedule, explain, empty).

export const libraryStrings = {
  neverRun: "Not run yet",
  scheduleNotice: (title: string) => `Scheduling for "${title}" is not set up yet.`,
  closeExplain: "Close",
};
