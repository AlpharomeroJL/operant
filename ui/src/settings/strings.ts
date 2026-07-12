// Additional default-mode strings for the Settings screen, beyond the
// section titles already in ui/src/strings/default.ts (settingsStrings).
// Same rule as every default-mode catalog: only user-facing vocabulary from
// contracts/microcopy_glossary.json, enforced by scripts/microcopy_lint.mjs.

export const settingsDetailStrings = {
  modelNotConnected: "Not connected yet",
  voiceEnableToggle: "Let me talk to it",
  pushToTalkLabel: "Hold this key to talk",
  speakingRateLabel: "How fast it talks back",
  killSwitchCurrentLabel: (chord: string) => `Current shortcut: ${chord}`,
  killSwitchChangeButton: "Change shortcut",
  killSwitchCancelButton: "Cancel",
  killSwitchRecordingHint: "Press the new key combination now",
  killSwitchTooShort: "Hold at least one extra key, like Ctrl, plus another key.",
  purgeDone: "Deleted.",
  backupExportButton: "Save a backup file",
  backupImportButton: "Load a backup file",
  backupLastLabel: (when: string) => `Last backup: ${when}`,
  backupNever: "You have not made a backup yet.",
  backupInvalid: "That file does not look like an Operant backup.",
  autoUpdateToggle: "Check for updates automatically",
  autoUpdateHint: "Downloads the new version in the background and checks it before installing.",

  // D6 (docs/specs/design.md section 3.3): copy for the restyled Settings
  // screen's new Appearance and Advanced sidebar sections.
  appearanceThemeLabel: "Color theme",
  accentSyncToggle: "Match my Windows accent color",
  // "workflow file", "activity record", and "connected tools" are the exact
  // user-facing terms contracts/microcopy_glossary.json maps to DSL, audit
  // chain, and MCP respectively: this line describes what Advanced mode adds
  // without naming any of those three internal terms directly, so it stays
  // default-mode-safe (scripts/microcopy_lint.mjs) while still pointing at
  // the same three surfaces docs/specs/ui.md's Advanced toggle reveals.
  advancedSectionBody: "Advanced mode adds a workflow file view, an activity record, and connected tools.",
  advancedOpenButton: "Turn on Advanced mode",
  advancedOnHint: "Advanced mode is on. Find it in the header any time.",
  advancedCloseButton: "Turn off Advanced mode",
};
