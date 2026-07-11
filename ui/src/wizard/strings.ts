// Default-mode copy for the onboarding wizard (docs/specs/zero-code.md's
// five screens: welcome, "How should Operant think?", mic check, guided
// first task, schedule).
//
// welcomeStrings, setupPathStrings, micCheckStrings, scheduleStrings, and
// downloadErrorStrings are extracted from ui/src/locales/en.ts and localized
// via locale switching (getLocale/setLocale in ui/src/locales/index.ts).
// For manual changes, update the source in ui/src/locales/en.ts.
//
// guidedTaskStrings is new: docs/specs/zero-code.md leaves this screen's
// copy to whichever lane wires the run in (this one), since it is the one
// screen that depends on a live run rather than being static text.
//
// Every string here must use only user-facing vocabulary from
// contracts/microcopy_glossary.json; scripts/microcopy_lint.mjs enforces
// this in CI by scanning quoted string literals under ui/src.

import { getLocaleCatalog } from "../locales/index.ts";
import * as enCatalog from "../locales/en.ts";

// For backward compatibility, export the English defaults directly.
// Tests that need locale switching should call getWizardStrings().
export const welcomeStrings = enCatalog.welcomeStrings;
export const setupPathStrings = enCatalog.setupPathStrings;
export const providerDisplayNames = enCatalog.providerDisplayNames;
export const micCheckStrings = enCatalog.micCheckStrings;
export const guidedTaskStrings = enCatalog.guidedTaskStrings;
export const scheduleStrings = enCatalog.scheduleStrings;
export const downloadErrorStrings = enCatalog.downloadErrorStrings;
export const wizardShellStrings = enCatalog.wizardShellStrings;

// For locale-aware access (e.g., in tests), use this function to get the
// current locale's strings.
export function getWizardStrings() {
  const catalog = getLocaleCatalog();
  return {
    welcomeStrings: catalog.welcomeStrings,
    setupPathStrings: catalog.setupPathStrings,
    providerDisplayNames: catalog.providerDisplayNames,
    micCheckStrings: catalog.micCheckStrings,
    guidedTaskStrings: catalog.guidedTaskStrings,
    scheduleStrings: catalog.scheduleStrings,
    downloadErrorStrings: catalog.downloadErrorStrings,
    wizardShellStrings: catalog.wizardShellStrings,
  };
}
