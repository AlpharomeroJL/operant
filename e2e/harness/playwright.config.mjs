// Playwright config for the shared capture rig. Two kinds of tests live
// under tests/: browser-driven ones that load contracts/fixtures/webapp
// (served locally by src/serve.mjs, started automatically below), and
// native-window ones that skip the browser entirely and drive Notepad via
// src/native-window.mjs + ffmpeg. Both run under `npm test`.

import { defineConfig, devices } from '@playwright/test';

const PORT = Number(process.env.HARNESS_PORT) || 4173;

export default defineConfig({
  testDir: './tests',
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  outputDir: './test-results',
  webServer: {
    command: `node src/serve.mjs --port ${PORT}`,
    port: PORT,
    reuseExistingServer: !process.env.CI,
    timeout: 15_000,
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
