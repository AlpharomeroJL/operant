// Fixture Playwright spec for `operant import playwright` (X9): a small,
// deterministic goto/fill/fill/click/expect flow against the fixture
// invoice form (contracts/fixtures/webapp/index.html), the same fixture
// the browser adapter (L9A) and the compiler (L8A) already exercise.
//
// This file is consumed by `cli/src/commands/import/playwright.rs`'s own
// hand-rolled parser (a basic subset of Playwright, not a full AST
// implementation), not by the real Playwright runner: it lives under
// contracts/fixtures/, outside e2e/harness/playwright.config.mjs's test
// directory, on purpose.
import { test, expect } from '@playwright/test';

test('fills the invoice form and saves it', async ({ page }) => {
  await page.goto('../webapp/index.html');
  await page.fill('#customer', 'Acme Corp');
  await page.fill('#amount', '142.50');
  await page.click('#save-btn');
  await expect(page.locator('#customer')).toHaveValue('Acme Corp');
});
