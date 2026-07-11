// Browser-driven capture smoke test: proves the harness can serve
// contracts/fixtures/webapp over local HTTP and take a real PNG screenshot
// through Playwright, the same path README/product capture will reuse.
import { test, expect } from '@playwright/test';
import { mkdir, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const outDir = fileURLToPath(new URL('../.output', import.meta.url));

test.beforeAll(async () => {
  await mkdir(outDir, { recursive: true });
});

test('captures a PNG of the fixture invoice app', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('h1')).toHaveText('Operant Fixture Invoices');

  const outPath = join(outDir, 'webapp-index.png');
  await page.screenshot({ path: outPath });

  const info = await stat(outPath);
  expect(info.size).toBeGreaterThan(0);
});

test('captures a PNG of the drift fixture variant', async ({ page }) => {
  await page.goto('/drift.html');
  await expect(page.locator('#store-btn')).toBeVisible();

  const outPath = join(outDir, 'webapp-drift.png');
  await page.screenshot({ path: outPath });

  const info = await stat(outPath);
  expect(info.size).toBeGreaterThan(0);
});
