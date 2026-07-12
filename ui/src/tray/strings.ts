// Default-mode strings for tray notifications, beyond
// ui/src/strings/default.ts's trayStrings (idle, running, halted,
// savedTimeTooltip).

export const trayNotificationStrings = {
  haltedTitle: "Operant stopped",
  haltedBody: "It stopped and needs you before it can continue.",
  weeklyDigestTitle: "Your weekly time saved",
  // The unit label beside the restyled digest's mono stat figure (F11,
  // ui/src/tray/view.ts): short by design, next to a number, not a sentence
  // (design.md section 4's sentence-case rule governs prose, not a unit tag).
  digestUnit: "min",
  dismiss: "Dismiss",
};
