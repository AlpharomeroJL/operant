// Palette UI strings for the command interface.
// Imported from the locale catalog to support i18n.

import { getLocaleCatalog } from "../locales/index.ts";

export function getPaletteStrings() {
  return getLocaleCatalog().paletteStrings;
}

/** Strings for the target-app picker (ui/src/palette/targetApp.ts), the step where the person chooses which open app a teach run should target. */
export function getTargetAppStrings() {
  return getLocaleCatalog().targetAppStrings;
}
