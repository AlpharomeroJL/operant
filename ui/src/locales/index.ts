// Locale catalog loader with runtime locale switching support.
// Currently supports en and es locales for wizard and palette.

import * as en from "./en.ts";
import * as es from "./es.ts";

export type Locale = "en" | "es";

// Current active locale, defaulting to English
let currentLocale: Locale = "en";

const catalogs: Record<Locale, typeof en> = {
  en,
  es,
};

/**
 * Get the currently active locale.
 */
export function getLocale(): Locale {
  return currentLocale;
}

/**
 * Switch to a different locale.
 */
export function setLocale(locale: Locale): void {
  if (!catalogs[locale]) {
    throw new Error(`Unknown locale: ${locale}`);
  }
  currentLocale = locale;
}

/**
 * Get the active locale catalog.
 */
export function getLocaleCatalog(): typeof en {
  return catalogs[currentLocale];
}

/**
 * Verify that all keys in source exist in target and vice versa.
 * Throws if keys differ.
 */
export function verifyLocaleKeys(source: Record<string, any>, target: Record<string, any>, path: string = ""): void {
  const sourceKeys = new Set(Object.keys(source));
  const targetKeys = new Set(Object.keys(target));

  for (const key of sourceKeys) {
    if (!targetKeys.has(key)) {
      throw new Error(`Missing key in target: ${path}${key}`);
    }
    // Recursively check nested objects (but not functions)
    if (typeof source[key] === "object" && source[key] !== null && typeof target[key] === "object") {
      verifyLocaleKeys(source[key], target[key], `${path}${key}.`);
    }
  }

  for (const key of targetKeys) {
    if (!sourceKeys.has(key)) {
      throw new Error(`Extra key in target: ${path}${key}`);
    }
  }
}
