// Static "quick actions" and "settings" catalog entries the palette fuzzy-
// matches over (docs/specs/design.md section 3, Palette: "fuzzy match over
// workflows, quick actions, and settings"). Workflow entries come from the
// live registry instead (ui/src/main.ts builds those); this file only ever
// covers the fixed, always-available screen-navigation shortcuts and the
// settings sections a person might jump straight to. Pure data, no DOM, no
// bus: ui/src/main.ts owns turning a chosen action's id into the actual
// screen switch or store call (the same seam ./catalog.ts's PaletteEntry
// deliberately stays agnostic about).
//
// Reuses existing, already microcopy-lint-approved strings (navStrings,
// settingsStrings, themeToggleStrings) rather than inventing new copy for
// the same concepts a second time, the same "do not duplicate copy across
// catalogs" rule ui/src/dashboard/state.ts's own header comment follows for
// sharing Library's registry instance.

import { navStrings, settingsStrings, themeToggleStrings } from "../strings/default.ts";
import { getPaletteStrings } from "./strings.ts";
import type { PaletteEntry } from "./catalog.ts";

export const PALETTE_ACTION_ID = {
  navDashboard: "action.nav.dashboard",
  navLibrary: "action.nav.library",
  navRuns: "action.nav.runs",
  navSettings: "action.nav.settings",
  cycleTheme: "action.theme.cycle",
} as const;

export const PALETTE_SETTING_ID = {
  model: "setting.model",
  voice: "setting.voice",
  killSwitch: "setting.killswitch",
  privacy: "setting.privacy",
  backup: "setting.backup",
} as const;

/** The four screen switches (design.md section 3's nav map) plus the theme cycle, as palette-searchable quick actions. */
export function buildQuickActionEntries(): PaletteEntry[] {
  const strings = getPaletteStrings();
  return [
    { id: PALETTE_ACTION_ID.navDashboard, kind: "action", title: navStrings.dashboard, subtitle: strings.openScreenHint },
    { id: PALETTE_ACTION_ID.navLibrary, kind: "action", title: navStrings.library, subtitle: strings.openScreenHint },
    { id: PALETTE_ACTION_ID.navRuns, kind: "action", title: navStrings.runs, subtitle: strings.openScreenHint },
    { id: PALETTE_ACTION_ID.navSettings, kind: "action", title: navStrings.settings, subtitle: strings.openScreenHint },
    { id: PALETTE_ACTION_ID.cycleTheme, kind: "action", title: strings.themeActionTitle, subtitle: themeToggleStrings.hint },
  ];
}

/**
 * The Settings screen's own section titles (ui/src/strings/default.ts's
 * settingsStrings), as palette-searchable entries. This screen is a single
 * scrollable page rather than separately routable sections (see
 * ui/src/settings/view.ts), so every one of these opens the same Settings
 * screen; picking one is a shortcut to get there, not a deep link to a
 * particular section within it.
 */
export function buildSettingsEntries(): PaletteEntry[] {
  const strings = getPaletteStrings();
  return [
    { id: PALETTE_SETTING_ID.model, kind: "setting", title: settingsStrings.modelSectionTitle, subtitle: strings.settingsHint },
    { id: PALETTE_SETTING_ID.voice, kind: "setting", title: settingsStrings.voiceSectionTitle, subtitle: strings.settingsHint },
    { id: PALETTE_SETTING_ID.killSwitch, kind: "setting", title: settingsStrings.killSwitchSectionTitle, subtitle: strings.settingsHint },
    { id: PALETTE_SETTING_ID.privacy, kind: "setting", title: settingsStrings.privacySectionTitle, subtitle: strings.settingsHint },
    { id: PALETTE_SETTING_ID.backup, kind: "setting", title: settingsStrings.backupSectionTitle, subtitle: strings.settingsHint },
  ];
}
