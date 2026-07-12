// Extra default-mode strings for the Home dashboard, beyond
// ui/src/strings/default.ts's dashboardStrings (title, placeholderBody,
// heroLine, sparklineSummary, upNextTitle/Empty, recentRunsTitle/Empty,
// emptyInvite, outcomeOk/Failed). Same split as ui/src/library/strings.ts
// and ui/src/tray/strings.ts: the cross-cutting, shared-catalog strings
// live in default.ts, this screen's smaller, more mechanical extras live
// beside its state.ts.

export const dashboardCopyStrings = {
  // The recent-run status dot's visually-hidden text equivalent: shorter and
  // worded differently than default.ts's outcomeOk/outcomeFailed's visible
  // one-liner (same two-layers-of-specificity split as
  // ui/src/runViewer/state.ts's step dots: a hidden state word plus a
  // separate visible sentence).
  statusOkLabel: "Completed",
  statusFailedLabel: "Did not finish",
  today: "today",
  tomorrow: "tomorrow",
  inDays: (days: number) => `in ${days} days`,
  weekdayNames: ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"] as const,
};
