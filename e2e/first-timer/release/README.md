# First-Timer Golden Path E2E, Release Target (NFR-7)

Reruns `e2e/first-timer`'s golden path (see the parent `README.md`) against the
actual installed release binary instead of the Vite dev server to validate the
first-timer experience against the released build: download the installer,
install it, and use the app.

## Why this is a separate script, not a Playwright config change

`tests/first-timer.spec.mjs` drives the app with Playwright's `page.goto()`
against `npm run dev`. An installed Tauri app is a real native window, not a
browser tab; there is no URL to load. `release/run.mjs` instead drives the
installed `.exe` through
[`tauri-driver`](https://v2.tauri.app/develop/tests/webdriver/), the
WebDriver-protocol harness Tauri ships for exactly this, using
`selenium-webdriver` as the W3C WebDriver client (Playwright's own client
speaks CDP, not WebDriver, and cannot attach to a `tauri-driver` session).
Same checkpoints, same order, same two invariants (no code/terminal surface,
whole path under the 15-minute budget) as the dev-server suite; see the
comment block at the top of `run.mjs` for the full checkpoint list.

## One-time setup

1. **Build the installer** (skip if `release/REPRODUCIBLE.md` already
   documents a build you trust, or one already exists under
   `$CARGO_TARGET_DIR/release/bundle/nsis/`):

   ```
   cd ui
   npm ci
   cd src-tauri
   cargo tauri build -b nsis --ci
   ```

   Produces `Operant_<version>_x64-setup.exe` under
   `$CARGO_TARGET_DIR/release/bundle/nsis/` (`$CARGO_TARGET_DIR` defaults to
   `D:/dev/operant-target` per the root `justfile`, overridable by env var,
   see the note on this lane's own environment below).

2. **Install it silently.** Per `ui/src-tauri/tauri.conf.json`'s
   `bundle.windows.nsis.installMode: "both"`, the installer accepts a scope
   flag; the intended least-privilege, no-admin path is the per-user install:

   ```
   Operant_<version>_x64-setup.exe /CURRENTUSER /S
   ```

   **This needs one interactive click on this lane's machine.** Verified in
   this lane (see `../RESULT.md`): launching the installer this way still
   triggers a Windows UAC elevation prompt (`consent.exe`), even with
   `/CURRENTUSER` given, and that prompt cannot be answered from a
   non-interactive automation session (`Start-Process` fails immediately
   with "The operation was canceled by the user" if nothing is present to
   answer it, or leaves a live `consent.exe` waiting indefinitely if
   something is). The installer's `/CURRENTUSER` flag picks the install
   *location* once the process is already running; it does not change
   whether Windows requires elevation to start that process at all, which
   appears to be fixed by the installer's own manifest (this repo's
   `release/nsis/installer-hooks.nsh` does not set an execution level; this
   is Tauri's/NSIS's base "both" mode template, outside this lane's owned
   paths). The generated uninstaller does not have this problem (see below),
   so this asymmetry is specific to the installer stub, not to NSIS silent
   mode in general. A real human with a real desktop session needs to run
   the command above and click "Yes" once; after that, `run.mjs` needs no
   further interaction.

   `run.mjs` does not install anything itself; it only drives whatever is
   already at `OPERANT_APP_PATH` (default:
   `%LOCALAPPDATA%\Programs\Operant\operant-shell.exe`). This verifies the
   real first-timer flow: download the signed installer, run it, and use the
   app, not a dev build wearing the release path's clothes.

   Uninstalling afterward (`"%LOCALAPPDATA%\Programs\Operant\uninstall.exe" /CURRENTUSER /S`)
   does not hit the same wall: verified in this lane, it runs non-elevated,
   completes in well under a minute with no visible UI, removes the install
   directory, removes the registry uninstall entry, and cleans the per-user
   `PATH` entry the installer added. It does not touch this app's real user
   data either way (see `../RESULT.md` for why that is true for a reason
   worth a FOLLOWUP, not because the uninstall prompt logic is reliably
   correct).

3. **`tauri-driver` on `PATH`**: `cargo install tauri-driver --locked`.

4. **`msedgedriver.exe` matching the installed WebView2 runtime version.**
   Check the runtime version from
   `C:\Program Files (x86)\Microsoft\EdgeWebView\Application\<version>\`,
   download the matching build from
   <https://developer.microsoft.com/en-us/microsoft-edge/tools/webdriver/>,
   and either put it on `PATH` or point `NATIVE_DRIVER_PATH` at it. A version
   mismatch hangs the WebDriver session instead of erroring cleanly, which is
   why `run.mjs` also enforces its own hard connect timeout
   (`CONNECT_TIMEOUT_MS`) so a mismatch fails fast instead of hanging CI.
   This repository does not vendor `msedgedriver.exe` (it is a Microsoft
   binary, not a project dependency); fetching it is a manual, one-time step
   for whoever sets up a machine to run this suite, not something `run.mjs`
   or `npm install` does automatically.

## Run

```bash
npm install       # once; installs selenium-webdriver alongside @playwright/test
npm run test:release
```

Equivalent to `node release/run.mjs`. Override any of these via environment
variable if your setup differs from the defaults:

| Variable | Default | Purpose |
|---|---|---|
| `OPERANT_APP_PATH` | `%LOCALAPPDATA%\Programs\Operant\operant-shell.exe` | Path to the installed binary to drive. |
| `OPERANT_APP_IDENTIFIER` | `dev.operant.shell` | Must match `ui/src-tauri/tauri.conf.json`'s `identifier`; used only to locate and reset the WebView2 profile folder before each run (see below). |
| `TAURI_DRIVER_PORT` | `4444` | Port `tauri-driver` listens on. |
| `TAURI_DRIVER_BIN` | `tauri-driver` | Binary name/path if not on `PATH`. |
| `NATIVE_DRIVER_PATH` | none (falls back to `PATH`) | Explicit path to `msedgedriver.exe`. |

Screenshots at the guided-teach and final checkpoints land in `../.output/`
(`first-timer-release-*.png`, gitignored, regenerated per run), alongside the
dev-server suite's own screenshots.

## What "fresh device" means for a real install, and why the script resets it

The dev-server suite gets a clean Playwright browser context every run for
free. This suite drives a real installed app, so its WebView2 profile
(`%LOCALAPPDATA%\<identifier>\`, holding `localStorage` among other things)
persists on disk across runs. Every checkpoint in the golden path assumes a
"fresh device, nothing configured yet" starting state (that is the entire
point of a first-timer path), so `run.mjs` deletes that profile directory
before launching, every run, to put the installed app back into a genuinely
first-run state. It also best-effort `taskkill`s any `operant-shell.exe`,
`tauri-driver.exe`, or `msedgedriver.exe` left over from an interrupted
previous run before starting, so a crashed run does not wedge the next one
(stale processes holding the WebDriver port, or holding the profile
directory open so the reset above fails).

Without the profile reset, a second run inherits whatever the first run left
in `localStorage` and silently stops matching a first-timer's actual
experience; observed symptom in this lane before the reset was added: a
`stale element reference` WebDriver error with no obvious cause, not a clean
"wizard did not appear" failure, because the app was rendering a different
(post-wizard) screen than the script's selectors expected.

## Resilience to live re-renders (stale element reference)

This app re-renders while the suite is mid-interaction (streaming narration
lines, a connection-status indicator), so a `WebElement` handle obtained by
one WebDriver command can be detached by the time the next command uses it;
WebDriver reports that as `StaleElementReferenceError`, not as a normal
not-found/not-visible timeout. `findVisible`/`clickWhenVisible` in
`run.mjs` retry the whole locate-then-act sequence on that specific error
(bounded by the same per-step timeout, so a real hang still fails instead of
retrying forever); the `driver.wait(...)` polling loops for the two
transition checks (wizard dismissal, "Last run just now") treat a stale read
as "not yet" and keep polling rather than failing outright. This is the
standard fix for testing a live-updating UI over WebDriver, not specific to
this app.

## This lane's environment (V5)

Documented here rather than asserted as a universal fact, per
`release/REPRODUCIBLE.md`'s own convention of recording exactly what was and
was not exercised on the machine this ran on:

- `cargo tauri --version` reported `tauri-cli 2.11.4`; a real
  `cargo tauri build -b nsis --ci` installer and a real per-user silent
  install (`/CURRENTUSER /S`) both already existed on this machine at the
  start of this lane (built and installed by earlier campaign work; see
  `release/REPRODUCIBLE.md`'s L14A note). This run drove that existing
  install rather than rebuilding/reinstalling from scratch; `just ci` was
  still run fresh from repo root as this lane's own bar item (see
  `../RESULT.md`).
- `tauri-driver` was already on `PATH`
  (resolved to `D:\dev\cargo\bin\tauri-driver.exe`).
- A `msedgedriver.exe` matching the installed WebView2 runtime
  (`150.0.4078.65`, confirmed via both the runtime's own version folder and
  the driver's file version info) was already present at
  `scratch/tools/msedgedriver/msedgedriver.exe` in this worktree (outside
  `e2e/first-timer`, left there by earlier exploratory work in this same
  lane; `scratch/` is gitignored, so it does not ship, and a clean machine
  needs step 4 above done manually).
- A real interactive Windows desktop session was available (WebView2 needs
  an interactive window station to render into; this will not work in a
  Session-0 / true headless CI runner without something like a virtual
  display), but not one this agent could click a UAC consent prompt in: see
  the installer note above. Uninstall does not need that click; install
  does. Net effect for this lane specifically: this agent ended the lane
  with Operant uninstalled (it drove and verified the golden path against
  the real install first, then verified uninstall, then could not
  click through the one UAC prompt needed to reinstall). A person with a
  keyboard and mouse restores it with the single command in step 2 above
  plus one click; nothing else in this lane depends on it staying installed.
- See `../RESULT.md` for the actual pass/fail record of the run(s) against
  this environment, and for the full uninstall/reinstall findings.
