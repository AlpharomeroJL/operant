// The release-blocking first-timer E2E (docs/specs/zero-code.md): a person
// with nothing set up drives the wizard demo path, watches a demo run,
// teaches Operant a real task, saves it, runs the saved workflow, and
// schedules it, entirely through the real ui/ shell (ui/src/main.ts) in a
// headless browser. Two things must hold at every checkpoint:
//
//   1. No code or terminal surface is ever visible. The shell has exactly
//      one such surface, the Advanced toggle (#op-mode-toggle) and the
//      panel it reveals (#op-advanced-panel, holding the DSL editor, raw
//      manifest, audit browser, and MCP config per
//      ui/src/advanced/state.ts). This suite never clicks that toggle, and
//      asserts its state stays closed after every step, catching any future
//      wiring bug that pops it open by accident.
//   2. The whole scripted interaction finishes fast. It uses the wizard's
//      own mock planner (ui/src/wizard/guidedTask.ts, timer-driven, no live
//      model) and the library's own mocked replay (ui/src/library/state.ts),
//      never a real model call, so this is a deterministic ceiling check,
//      not a benchmark: it proves the golden path does not hang or hit a
//      real network/model dependency, comfortably inside a 15-minute
//      budget.
//
// Screen flow (ui/src/wizard/state.ts's WizardScreenId), matching this
// lane's brief order:
//   welcome -> setup_path (demo link) -> guided_task[demo] (demo run) ->
//   setup_path -> mic_check -> guided_task[real] (guided teach) ->
//   schedule (compile, via Save as workflow) -> [wizard dismissed] ->
//   library (run the saved workflow) -> schedule already set above.
//
// The wizard's own schedule screen is the last screen inside the modal;
// finishing it is the only way to dismiss the modal and reach the main
// shell's Library screen where a saved workflow can actually be run
// (ui/src/main.ts's #op-wizard-backdrop covers the whole shell while the
// wizard is open). So "run" happens right after "schedule" is chosen and
// saved, not literally between "compile" and "schedule": there is no
// in-modal path to a second run. The workflow is still fully compiled,
// scheduled, and proven runnable, in that order, before this test ends.
import { test, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const outDir = fileURLToPath(new URL('../.output', import.meta.url));
const BUDGET_MS = 15 * 60 * 1000;
const STEP_TIMEOUT = 20_000;

test.beforeAll(async () => {
  await mkdir(outDir, { recursive: true });
});

test('BAR: zero-code first-timer path never shows a code surface and finishes well under 15 minutes', async ({ page }) => {
  const startedAt = Date.now();

  async function assertNoCodeSurface() {
    await expect(page.locator('#op-mode-toggle')).toHaveAttribute('aria-pressed', 'false');
    await expect(page.locator('#op-advanced-panel')).toBeHidden();
  }

  // Fresh device: nothing in localStorage, so the wizard shows on load and
  // Advanced mode has never been touched.
  await page.goto('/');
  const wizard = page.locator('.op-wizard[role="dialog"]');
  await expect(wizard).toBeVisible();
  await assertNoCodeSurface();

  // Screen 1: welcome. Every guided-task assertion below is scoped to the
  // wizard modal, not the bare page: main.ts's own "What it's doing" run
  // viewer shares the same mocked bus (ui/src/bus/mockClient.ts) as the
  // wizard's internal one (ui/src/runViewer/state.ts), so the same narrated
  // step sentences render twice, once in each panel, while the wizard is
  // open.
  await expect(wizard.getByRole('heading', { name: 'Welcome to Operant' })).toBeVisible();
  await wizard.getByRole('button', { name: 'Continue', exact: true }).click();

  // Screen 2: setup path. "Wizard demo path": the quiet demo link, watch it
  // work with zero grants before configuring anything.
  await expect(wizard.getByRole('heading', { name: 'How should Operant think?' })).toBeVisible();
  await assertNoCodeSurface();
  await wizard.getByRole('button', { name: 'Just show me a demo' }).click();

  // "Demo run": the guided task streams in demo mode.
  await expect(wizard.getByText('Watching a quick demo')).toBeVisible();
  await expect(wizard.getByText('Done. Here is everything it just did.')).toBeVisible({ timeout: STEP_TIMEOUT });
  await assertNoCodeSurface();
  await wizard.getByRole('button', { name: 'Set it up for real' }).click();

  // Back at setup path: sign in with ChatGPT to reach the mic check, then
  // skip it (this suite proves the click path, not the mic hardware) to
  // start the real guided-teach run.
  await expect(wizard.getByRole('heading', { name: 'How should Operant think?' })).toBeVisible();
  await wizard.getByRole('button', { name: 'Sign in with ChatGPT' }).click();

  await expect(wizard.getByRole('heading', { name: "Let's check your microphone" })).toBeVisible();
  await assertNoCodeSurface();
  await wizard.getByRole('button', { name: 'Skip for now' }).click();

  // "Guided teach": a real (non-demo) run against the practice invoice page
  // (contracts/fixtures/webapp/index.html), narrated step by step from the
  // real renderer, never a raw template.
  await expect(wizard.getByText("Let's try your first task")).toBeVisible();
  await expect(wizard.getByText('Type "Acme Co" into "Customer"')).toBeVisible({ timeout: STEP_TIMEOUT });
  await expect(wizard.getByText('Type "420.00" into "Amount"')).toBeVisible({ timeout: STEP_TIMEOUT });
  await expect(wizard.getByText('Type "2026-01-15" into "Date"')).toBeVisible({ timeout: STEP_TIMEOUT });
  await expect(wizard.getByText('Click "Save invoice"')).toBeVisible({ timeout: STEP_TIMEOUT });
  await expect(wizard.getByText('Done. Here is everything it just did.')).toBeVisible({ timeout: STEP_TIMEOUT });
  await assertNoCodeSurface();
  await page.screenshot({ path: join(outDir, 'first-timer-guided-teach-done.png') });

  // "Compile": Save as workflow, ending the guided task on exactly one
  // button, per docs/specs/zero-code.md.
  await wizard.getByRole('button', { name: 'Save as workflow' }).click();
  await expect(wizard.getByRole('heading', { name: 'Want this to run by itself?' })).toBeVisible();
  await assertNoCodeSurface();

  // "Schedule": choose daily and save it, which dismisses the wizard modal
  // for good on this device.
  await wizard.getByRole('radio', { name: 'Every day' }).check();
  await wizard.getByRole('button', { name: 'Save this schedule' }).click();
  await expect(wizard).toBeHidden();
  await assertNoCodeSurface();

  // "Run": the compiled workflow now lives in the library
  // (ui/src/library/state.ts picked up the workflow.compiled event); run it
  // zero-code from there to prove the saved workflow actually replays, not
  // just that it compiled.
  await page.locator('#op-nav-library').click();
  const card = page.locator('.op-library-card[aria-label="first-task"]');
  await expect(card).toBeVisible();
  await assertNoCodeSurface();
  await card.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(card.locator('.op-library-card__last-run')).toHaveText('Last run just now');
  await assertNoCodeSurface();
  await page.screenshot({ path: join(outDir, 'first-timer-run-complete.png') });

  const elapsedMs = Date.now() - startedAt;
  expect(elapsedMs).toBeLessThan(BUDGET_MS);
});
