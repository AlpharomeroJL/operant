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
  autoUpdateHint: "Downloads the new version in the background and checks it before installing. Turns off on its own when you're offline.",
};
