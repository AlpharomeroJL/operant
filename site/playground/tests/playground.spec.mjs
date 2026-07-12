import { statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";

const here = fileURLToPath(new URL(".", import.meta.url));
const playgroundRoot = join(here, "..");
const MAX_TOTAL_BYTES = 5 * 1024 * 1024;

// Every file this page ships to a visitor: the wasm module, its JS glue,
// the checked-in fixtures, and the page itself. Deliberately a fixed list
// rather than a directory walk, so a stray large file added later fails
// this test instead of silently inflating the payload.
const SHIPPED_FILES = [
  "index.html",
  "playground.css",
  "playground.js",
  "pkg/operant_replay.js",
  "pkg/operant_replay_bg.wasm",
  "fixtures/compiled_workflow.json",
  "fixtures/webapp.html",
];

test("shipped payload stays under the 5 MB budget", () => {
  const total = SHIPPED_FILES.reduce(
    (sum, rel) => sum + statSync(join(playgroundRoot, rel)).size,
    0,
  );
  expect(total).toBeLessThan(MAX_TOTAL_BYTES);
});

test("a local build loads and replays the fixture web workflow", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#replay-btn")).toBeVisible();

  const frame = page.frameLocator("#fixture-frame");
  await expect(frame.locator("#invoice-count")).toHaveText("0");

  await page.locator("#replay-btn").click();

  const status = page.locator("#playground-status");
  await expect(status).toHaveClass(/status-pass/, { timeout: 15_000 });
  await expect(status).toContainText("Replay passed");
  await expect(status).toContainText("6 step(s) executed");

  // The narration actually drove the real iframe DOM, not just the wasm
  // call: the fixture app's own submit handler ran and persisted the
  // invoice.
  await expect(frame.locator("#invoice-count")).toHaveText("1");
  await expect(frame.locator("#invoice-list li")).toContainText("Acme Corp | $142.50");
});

test("replaying twice does not double-save the invoice", async ({ page }) => {
  await page.goto("/");
  await page.locator("#replay-btn").click();
  await expect(page.locator("#playground-status")).toHaveClass(/status-pass/, {
    timeout: 15_000,
  });

  await page.locator("#replay-btn").click();
  await expect(page.locator("#playground-status")).toHaveClass(/status-pass/, {
    timeout: 15_000,
  });

  const frame = page.frameLocator("#fixture-frame");
  await expect(frame.locator("#invoice-count")).toHaveText("1");
});
