// Release-target proof of the first-timer golden path (docs/specs/zero-code.md),
// driven against the actual installed release binary, not the Vite dev server.
//
// e2e/first-timer/tests/first-timer.spec.mjs proves the same golden path with
// Playwright's page.goto() against `npm run dev`. That approach does not apply
// here: an installed Tauri app is a real native window, not a browser tab, so
// there is no URL to navigate to. This script drives the installed .exe
// through tauri-driver (the WebDriver-protocol harness Tauri ships for this
// exact purpose: https://v2.tauri.app/develop/tests/webdriver/), using
// selenium-webdriver as the W3C WebDriver client, since Playwright's own API
// speaks CDP, not WebDriver, and cannot attach to a tauri-driver session.
//
// Same checkpoints as the dev-server suite, same order, reusing the exact
// button/heading/narration text this app renders (verified against
// ui/src/wizard/view.ts, ui/src/runViewer/view.ts, ui/src/library/view.ts,
// ui/src/locales/en.ts): wizard demo path, demo run, guided teach, compile
// (Save as workflow), schedule (Save this schedule), then run the saved
// workflow from the Library screen. Same two invariants: no code/terminal
// surface ever visible (#op-mode-toggle stays aria-pressed="false",
// #op-advanced-panel stays hidden), and the whole path finishes inside the
// 15-minute budget.
//
// Prerequisites (see e2e/first-timer/release/README.md for the full setup):
//   - The NSIS installer built and silently installed with `/CURRENTUSER /S`
//     (see release/REPRODUCIBLE.md); this script does not install anything,
//     it only drives whatever is already at OPERANT_APP_PATH.
//   - `tauri-driver` on PATH (`cargo install tauri-driver --locked`).
//   - `msedgedriver.exe` matching the installed WebView2/Edge version, on
//     PATH or pointed at via NATIVE_DRIVER_PATH
//     (https://v2.tauri.app/develop/tests/webdriver/, Windows uses
//     Microsoft Edge WebDriver against the WebView2 runtime); a version
//     mismatch hangs the session instead of erroring, so this script also
//     enforces a hard connect timeout to fail fast instead of hanging CI).
//   - A real Windows desktop session: WebView2 needs an interactive window
//     station to render into. This will not work in a Session-0 / true
//     headless CI runner without something like a virtual display; see the
//     README and RESULT.md for what was and was not verified in this lane's
//     environment.
import { Builder, By, Capabilities, until } from 'selenium-webdriver';
import { spawn, execFileSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { mkdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';

const outDir = fileURLToPath(new URL('../.output', import.meta.url));
const BUDGET_MS = 15 * 60 * 1000;
const STEP_TIMEOUT_MS = 20_000;
const CONNECT_TIMEOUT_MS = 30_000;

const APP_PATH = process.env.OPERANT_APP_PATH
  || join(process.env.LOCALAPPDATA || '', 'Programs', 'Operant', 'operant-shell.exe');
const TAURI_DRIVER_PORT = Number(process.env.TAURI_DRIVER_PORT) || 4444;
const NATIVE_DRIVER_PATH = process.env.NATIVE_DRIVER_PATH || null;
const TAURI_DRIVER_BIN = process.env.TAURI_DRIVER_BIN || 'tauri-driver';
// Tauri's `identifier` (ui/src-tauri/tauri.conf.json, read-only to this lane)
// is also the folder name WebView2 uses for its per-app profile
// (localStorage, cache, crash handler state, ...) under %LOCALAPPDATA%. This
// script drives a real installed app, not a disposable Playwright browser
// context, so that profile survives across runs unless removed: without this
// reset, a second run inherits whatever the first run left in localStorage
// (for example an already-set "wizard completed" flag) and the "fresh
// device" assumption every checkpoint below depends on silently stops
// holding, producing confusing failures (observed: a stale-element error
// with no obvious cause) instead of a clean first-screen match.
const APP_IDENTIFIER = process.env.OPERANT_APP_IDENTIFIER || 'dev.operant.shell';
const APP_PROFILE_DIR = join(process.env.LOCALAPPDATA || '', APP_IDENTIFIER);

function log(msg) {
  console.log(`[first-timer/release] ${msg}`);
}

function killStrayProcesses() {
  for (const name of ['operant-shell.exe', 'tauri-driver.exe', 'msedgedriver.exe']) {
    try {
      execFileSync('taskkill', ['/F', '/IM', name, '/T'], { stdio: 'ignore' });
      log(`killed a stray ${name} from a previous run`);
    } catch {
      // Not running; nothing to do.
    }
  }
}

async function resetAppProfile() {
  if (!APP_PROFILE_DIR || APP_PROFILE_DIR === process.env.LOCALAPPDATA) return;
  try {
    await rm(APP_PROFILE_DIR, { recursive: true, force: true });
    log(`reset app profile at ${APP_PROFILE_DIR} (simulating a freshly installed, never-run device)`);
  } catch (err) {
    log(`warning: could not reset app profile at ${APP_PROFILE_DIR}: ${err.message}`);
  }
}

async function waitForPort(port, timeoutMs) {
  const startedAt = Date.now();
  for (;;) {
    const ok = await new Promise((resolve) => {
      const socket = net.createConnection({ port, host: '127.0.0.1' });
      socket.once('connect', () => {
        socket.destroy();
        resolve(true);
      });
      socket.once('error', () => resolve(false));
    });
    if (ok) return;
    if (Date.now() - startedAt > timeoutMs) {
      throw new Error(`tauri-driver did not open port ${port} within ${timeoutMs}ms`);
    }
    await delay(300);
  }
}

function startTauriDriver() {
  const args = [];
  if (TAURI_DRIVER_PORT !== 4444) args.push('--port', String(TAURI_DRIVER_PORT));
  if (NATIVE_DRIVER_PATH) args.push('--native-driver', NATIVE_DRIVER_PATH);
  log(`spawning ${TAURI_DRIVER_BIN} ${args.join(' ')}`);
  const child = spawn(TAURI_DRIVER_BIN, args, { stdio: ['ignore', 'pipe', 'pipe'] });
  let stderrTail = '';
  child.stderr.on('data', (chunk) => {
    stderrTail = (stderrTail + chunk.toString()).slice(-4000);
  });
  child.on('error', (err) => {
    log(`tauri-driver failed to start: ${err.message}`);
  });
  return { child, getStderrTail: () => stderrTail };
}

// XPath text helpers: this app renders plain-text leaf nodes for headings,
// buttons, and narration lines (see the view.ts files cited above), so
// exact-text XPath matching mirrors what Playwright's getByRole/getByText
// checks in the dev-server spec without needing a Testing-Library-style
// query layer on top of raw WebDriver.
function xpathLiteral(text) {
  if (!text.includes("'")) return `'${text}'`;
  if (!text.includes('"')) return `"${text}"`;
  const parts = text.split("'").map((p) => `'${p}'`);
  return `concat(${parts.join(", \"'\", ")})`;
}

// This app re-renders live (narration lines streaming in on timers, a
// connection-status dot, etc.), so the element `elementLocated` returns can
// be detached from the DOM by the time a later command (the visibility
// check, or a click) reaches it: WebDriver reports that as "stale element
// reference", not as a normal not-found/not-visible timeout. Retrying the
// whole locate-then-act sequence on that specific error, rather than letting
// it bubble up as a hard failure, is the standard fix (the element is not
// gone, just replaced; the next attempt finds its replacement).
function isStaleElementError(err) {
  return Boolean(err) && (err.name === 'StaleElementReferenceError' || /stale element reference/i.test(err.message || ''));
}

async function findVisible(driver, scopeXpath, timeoutMs = STEP_TIMEOUT_MS) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const remaining = Math.max(1, deadline - Date.now());
    try {
      const el = await driver.wait(until.elementLocated(By.xpath(scopeXpath)), remaining, `not found: ${scopeXpath}`);
      await driver.wait(until.elementIsVisible(el), Math.max(1, deadline - Date.now()), `not visible: ${scopeXpath}`);
      return el;
    } catch (err) {
      if (!isStaleElementError(err) || Date.now() >= deadline) throw err;
    }
  }
}

// Locate-and-click as one retryable unit: a click can itself land on an
// element that goes stale between findVisible() returning it and the click
// being dispatched, so the retry has to wrap both steps together, not just
// the lookup.
async function clickWhenVisible(driver, scopeXpath, timeoutMs = STEP_TIMEOUT_MS) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const remaining = Math.max(1, deadline - Date.now());
    try {
      const el = await findVisible(driver, scopeXpath, remaining);
      await el.click();
      return;
    } catch (err) {
      if (!isStaleElementError(err) || Date.now() >= deadline) throw err;
    }
  }
}

function headingXpath(scope, text) {
  return `${scope}//h2[normalize-space(text())=${xpathLiteral(text)}]`;
}
function buttonXpath(scope, text) {
  return `${scope}//button[normalize-space(text())=${xpathLiteral(text)}]`;
}
function textXpath(scope, text) {
  return `${scope}//*[normalize-space(text())=${xpathLiteral(text)}]`;
}
function narrationXpath(scope, text) {
  return `${scope}//span[contains(concat(' ', normalize-space(@class), ' '), ' op-step__sentence ') and normalize-space(text())=${xpathLiteral(text)}]`;
}

async function main() {
  await mkdir(outDir, { recursive: true });
  killStrayProcesses();
  await resetAppProfile();

  const { child: driverProc, getStderrTail } = startTauriDriver();
  let driver = null;
  let failure = null;
  const startedAt = Date.now();

  try {
    await waitForPort(TAURI_DRIVER_PORT, CONNECT_TIMEOUT_MS);

    const capabilities = new Capabilities();
    capabilities.set('tauri:options', { application: APP_PATH });
    capabilities.setBrowserName('wry');

    log(`connecting to ${APP_PATH} via tauri-driver on port ${TAURI_DRIVER_PORT}`);
    driver = await new Builder()
      .withCapabilities(capabilities)
      .usingServer(`http://127.0.0.1:${TAURI_DRIVER_PORT}/`)
      .build();

    const WIZARD = "//div[contains(concat(' ', normalize-space(@class), ' '), ' op-wizard ') and @role='dialog']";

    async function assertNoCodeSurface() {
      const toggle = await findVisible(driver, "//*[@id='op-mode-toggle']");
      const pressed = await toggle.getAttribute('aria-pressed');
      if (pressed !== 'false') throw new Error(`expected #op-mode-toggle aria-pressed=false, got ${pressed}`);
      const panel = await driver.findElement(By.id('op-advanced-panel'));
      const hidden = await panel.getAttribute('hidden');
      if (hidden === null) throw new Error('expected #op-advanced-panel to have the hidden attribute');
    }

    // Fresh device: nothing in localStorage, so the wizard shows on launch.
    log('step: waiting for wizard on launch');
    await findVisible(driver, WIZARD);
    await assertNoCodeSurface();

    // Screen 1: welcome.
    log('step: welcome screen');
    await findVisible(driver, headingXpath(WIZARD, 'Welcome to Operant'));
    await clickWhenVisible(driver, buttonXpath(WIZARD, 'Continue'));

    // Screen 2: setup path -> wizard demo path.
    log('step: setup path (demo link)');
    await findVisible(driver, headingXpath(WIZARD, 'How should Operant think?'));
    await assertNoCodeSurface();
    await clickWhenVisible(driver, buttonXpath(WIZARD, 'Just show me a demo'));

    // Demo run.
    log('step: demo run');
    await findVisible(driver, textXpath(WIZARD, 'Watching a quick demo'));
    await findVisible(driver, textXpath(WIZARD, 'Done. Here is everything it just did.'), STEP_TIMEOUT_MS);
    await assertNoCodeSurface();
    await clickWhenVisible(driver, buttonXpath(WIZARD, 'Set it up for real'));

    // Back at setup path: sign in, reach mic check, skip it.
    log('step: sign in, mic check');
    await findVisible(driver, headingXpath(WIZARD, 'How should Operant think?'));
    await clickWhenVisible(driver, buttonXpath(WIZARD, 'Sign in with ChatGPT'));

    await findVisible(driver, headingXpath(WIZARD, "Let's check your microphone"));
    await assertNoCodeSurface();
    await clickWhenVisible(driver, buttonXpath(WIZARD, 'Skip for now'));

    // Guided teach: real (non-demo) run against the practice invoice page.
    log('step: guided teach');
    await findVisible(driver, textXpath(WIZARD, "Let's try your first task"));
    await findVisible(driver, narrationXpath(WIZARD, 'Type "Acme Co" into "Customer"'), STEP_TIMEOUT_MS);
    await findVisible(driver, narrationXpath(WIZARD, 'Type "420.00" into "Amount"'), STEP_TIMEOUT_MS);
    await findVisible(driver, narrationXpath(WIZARD, 'Type "2026-01-15" into "Date"'), STEP_TIMEOUT_MS);
    await findVisible(driver, narrationXpath(WIZARD, 'Click "Save invoice"'), STEP_TIMEOUT_MS);
    await findVisible(driver, textXpath(WIZARD, 'Done. Here is everything it just did.'), STEP_TIMEOUT_MS);
    await assertNoCodeSurface();
    await driver.takeScreenshot().then((data) => writeScreenshot('first-timer-release-guided-teach-done.png', data));

    // Compile: Save as workflow.
    log('step: compile (Save as workflow)');
    await clickWhenVisible(driver, buttonXpath(WIZARD, 'Save as workflow'));
    await findVisible(driver, headingXpath(WIZARD, 'Want this to run by itself?'));
    await assertNoCodeSurface();

    // Schedule: choose daily and save, dismissing the wizard for good.
    log('step: schedule (Save this schedule)');
    const dailyLabelXpath = `${WIZARD}//label[contains(concat(' ', normalize-space(@class), ' '), ' op-wizard-schedule__option ') and contains(., 'Every day')]//input[@type='radio']`;
    await clickWhenVisible(driver, dailyLabelXpath);
    await clickWhenVisible(driver, buttonXpath(WIZARD, 'Save this schedule'));

    await driver.wait(async () => {
      try {
        const backdrop = await driver.findElement(By.id('op-wizard-backdrop'));
        return (await backdrop.getAttribute('hidden')) !== null;
      } catch (err) {
        if (isStaleElementError(err)) return false; // mid re-render; poll again
        throw err;
      }
    }, STEP_TIMEOUT_MS, 'wizard did not dismiss after Save this schedule');
    await assertNoCodeSurface();

    // Run: the compiled workflow now lives in the library; run it zero-code.
    log('step: run the saved workflow from the library');
    await clickWhenVisible(driver, "//*[@id='op-nav-library']");
    const cardXpath = "//article[contains(concat(' ', normalize-space(@class), ' '), ' op-library-card ') and @aria-label='first-task']";
    await findVisible(driver, cardXpath);
    await assertNoCodeSurface();
    await clickWhenVisible(driver, `${cardXpath}//button[normalize-space(text())='Run']`);
    await driver.wait(async () => {
      try {
        const el = await driver.findElement(By.xpath(`${cardXpath}//span[contains(concat(' ', normalize-space(@class), ' '), ' op-library-card__last-run ')]`));
        return (await el.getText()) === 'Last run just now';
      } catch (err) {
        if (isStaleElementError(err)) return false; // mid re-render; poll again
        throw err;
      }
    }, STEP_TIMEOUT_MS, 'library card did not report "Last run just now" after Run');
    await assertNoCodeSurface();
    await driver.takeScreenshot().then((data) => writeScreenshot('first-timer-release-run-complete.png', data));

    const elapsedMs = Date.now() - startedAt;
    if (elapsedMs >= BUDGET_MS) {
      throw new Error(`golden path took ${elapsedMs}ms, over the ${BUDGET_MS}ms budget`);
    }

    log(`PASS: golden path completed against the installed binary in ${elapsedMs}ms`);
  } catch (err) {
    failure = err;
  } finally {
    if (driver) {
      try {
        await driver.quit();
      } catch {
        // best effort
      }
    }
    driverProc.kill();
    await delay(300);
    if (!driverProc.killed) {
      try {
        driverProc.kill('SIGKILL');
      } catch {
        // best effort
      }
    }
  }

  if (failure) {
    console.error(`[first-timer/release] FAIL: ${failure.message}`);
    const tail = getStderrTail();
    if (tail) console.error(`[first-timer/release] tauri-driver stderr tail:\n${tail}`);
    process.exitCode = 1;
  }
}

async function writeScreenshot(name, base64Data) {
  const { writeFile } = await import('node:fs/promises');
  await writeFile(join(outDir, name), base64Data, 'base64');
}

main();
