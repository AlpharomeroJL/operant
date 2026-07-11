// Palette UI strings for the command interface.
// Imported from the locale catalog to support i18n.

import { getLocaleCatalog } from "../locales/index.ts";

export function getPaletteStrings() {
  return getLocaleCatalog().paletteStrings;
}
