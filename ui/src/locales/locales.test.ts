// Locale catalog tests: verify that all locales have matching keys.

import { test } from "node:test";
import assert from "node:assert/strict";
import * as en from "./en.ts";
import * as es from "./es.ts";
import { verifyLocaleKeys, getLocale, setLocale, getLocaleCatalog } from "./index.ts";
import { getWizardStrings } from "../wizard/strings.ts";

test("all locale keys in en exist in es", () => {
  // Verify that Spanish catalog has all the same keys as English
  verifyLocaleKeys(en, es);
});

test("all locale keys in es exist in en", () => {
  // Verify that English catalog has all the same keys as Spanish
  verifyLocaleKeys(es, en);
});

test("locale switching works correctly", () => {
  // Set to Spanish
  setLocale("es");
  assert.equal(getLocale(), "es");

  const esStrings = getLocaleCatalog();
  assert.equal(esStrings.welcomeStrings.heading, "Bienvenido a Operant");

  // Switch back to English
  setLocale("en");
  assert.equal(getLocale(), "en");

  const enStrings = getLocaleCatalog();
  assert.equal(enStrings.welcomeStrings.heading, "Welcome to Operant");
});

test("wizard switches locale properly", () => {
  // Default to English
  setLocale("en");
  let wizardStrings = getWizardStrings();
  assert.equal(wizardStrings.welcomeStrings.heading, "Welcome to Operant");

  // Switch to Spanish
  setLocale("es");
  wizardStrings = getWizardStrings();
  assert.equal(wizardStrings.welcomeStrings.heading, "Bienvenido a Operant");

  // Verify palette strings also switch
  assert.equal(wizardStrings.paletteStrings?.placeholder, undefined);

  // Reset to English
  setLocale("en");
});

test("invalid locale throws error", () => {
  assert.throws(() => {
    setLocale("fr" as any);
  }, /Unknown locale/);
});
