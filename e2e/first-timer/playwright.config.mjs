// Playwright config for the first-timer golden-path proof. Boots the real
// ui/ shell (Vite dev server, same app main.ts wires up for a person) and
// drives it headless, the same shape as e2e/harness's own config (fixed
// port, single worker, webServer auto-started) but pointed at ui/ instead
// of the fixture webapp server.
//
// ui/ is a separate npm workspace with its own devDependencies (vite,
// typescript); the webServer command installs those before starting the
// dev server so `npm install && npm test` from this package alone is
// enough to run the suite, with no separate manual `ui/` setup step.

import { defineConfig, devices } from '@playwright/test';
import { fileURLToPath } from 'node:url';

const PORT = Number(process.env.FIRST_TIMER_PORT) || 4415;
const UI_DIR = fileURLToPath(new URL('../../ui', import.meta.url));

export default defineConfig({
  testDir: './tests',
  timeout: 90_000,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  outputDir: './test-results',
  webServer: {
    command: `npm install --no-audit --no-fund && npm run dev -- --port ${PORT} --strictPort --host 127.0.0.1`,
    cwd: UI_DIR,
    port: PORT,
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    headless: true,
    trace: 'off',
    screenshot: 'off',
    video: 'off',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
