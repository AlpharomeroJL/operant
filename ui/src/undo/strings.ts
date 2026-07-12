// Default-mode strings for the Undo screen (docs/specs/design.md section 3;
// section 4's copy rules: sentence case, verbs on buttons say what happens,
// no apology, no exclamation points). Self-contained, the same split
// ui/src/library/strings.ts and ui/src/tray/strings.ts use: the shared
// "Undo this run" entry-point label ui/src/main.ts wires from the run
// viewer and from a toast lives in ui/src/strings/default.ts's
// undoEntryStrings instead, since this module's own view never renders
// that button itself.

export const undoScreenStrings = {
  title: "Undo this run",
  // Verb-first, says what happens (design.md section 4), not a bare "Confirm".
  confirm: "Restore these",
  cancel: "Cancel",
  close: "Close",
  empty: "Nothing to undo for this run.",
  doneSummary: (restored: number) => `Restored ${restored} item${restored === 1 ? "" : "s"}.`,
};
