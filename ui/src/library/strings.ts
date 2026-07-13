// Extra default-mode strings for the workflow library, beyond
// ui/src/strings/default.ts's workflowLibraryStrings (title, lastRun,
// minutesSaved, run, schedule, explain, empty).

export const libraryStrings = {
  neverRun: "Not run yet",
  // Shown after a Schedule press when the core reports it cannot schedule yet
  // (the upsert_trigger command answers not_implemented, contracts/ipc.md
  // section 5g). design.md section 4's error rule: say what happened plainly,
  // no false promise, no apology. This must never claim a schedule was created,
  // because none was.
  scheduleUnavailable: (title: string) => `Scheduling isn't available yet, so "${title}" can't be set to run on its own. This is coming in a later update.`,
  closeExplain: "Close",
};
