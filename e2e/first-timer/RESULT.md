# V5 first-timer-release: RESULT

## STATUS: pass

The first-timer golden path is green against the real installed release
artifact (the preferred, full bar from `scratch/lanes/V5/brief.md`, not the
documented-partial fallback). `just ci` is green from repo root. Uninstall
was verified clean. One real, verified environment limitation is carried
forward, not hidden: this agent could not click through the one UAC prompt
needed to run the installer non-interactively, so this session ends with
Operant uninstalled on this machine (see DECISIONS).

## BAR OUTPUT

### 1. Dev-server harness (E1C, rerun as this lane's baseline)

`cd e2e/first-timer; npm test`

```
> operant-e2e-first-timer@1.0.0 test
> playwright test

Running 1 test using 1 worker

  ok 1 [chromium] tests\first-timer.spec.mjs:50:1 BAR: zero-code first-timer path never shows a code surface and finishes well under 15 minutes (5.7s)

  1 passed (8.2s)
```

### 2. Release-mode harness against the installed binary (this lane's actual deliverable)

`cd e2e/first-timer; NATIVE_DRIVER_PATH=<repo>\scratch\tools\msedgedriver\msedgedriver.exe node release/run.mjs`

First attempt failed with a real, diagnosable bug (not a tooling gap); fixed
in `release/run.mjs` (fresh-profile reset, stray-process cleanup, retry on
`StaleElementReferenceError`; see DECISIONS). Final state, run twice in a
row to confirm it is not a fluke:

```
[first-timer/release] reset app profile at C:\Users\jo312\AppData\Local\dev.operant.shell (simulating a freshly installed, never-run device)
[first-timer/release] spawning tauri-driver --native-driver D:\dev\operant\lanes\V5\scratch\tools\msedgedriver\msedgedriver.exe
[first-timer/release] connecting to C:\Users\jo312\AppData\Local\Programs\Operant\operant-shell.exe via tauri-driver on port 4444
[first-timer/release] step: waiting for wizard on launch
[first-timer/release] step: welcome screen
[first-timer/release] step: setup path (demo link)
[first-timer/release] step: demo run
[first-timer/release] step: sign in, mic check
[first-timer/release] step: guided teach
[first-timer/release] step: compile (Save as workflow)
[first-timer/release] step: schedule (Save this schedule)
[first-timer/release] step: run the saved workflow from the library
[first-timer/release] PASS: golden path completed against the installed binary in 6365ms
```

(Run 1: 6437ms. Run 2: 6365ms. Both well under the 900,000ms/15-minute
budget.)

### 3. `just ci` from repo root

Full workspace build, full test suite (all crates), JSON/em-dash/microcopy/
air-gap checks. Last lines:

```
node scripts/check_json.mjs
check-json: OK (17 JSON files valid)
node scripts/check_emdash.mjs
check-emdash: OK (617 files clean)
node scripts/microcopy_lint.mjs
microcopy-lint: OK (77 default-mode files clean)
node scripts/check_airgap.mjs
check-airgap: OK (replay is backend-free by crate graph)
CI GREEN
```

All cargo unit/integration/doc tests passed (hundreds of tests across every
crate, 0 failed) earlier in the same run.

### 4. Uninstall verification

`"%LOCALAPPDATA%\Programs\Operant\uninstall.exe" /CURRENTUSER /S`

```
Started PID 37348
Exited within 15s: True
```

Confirmed after exit: install directory gone, `HKCU` uninstall registry key
gone, `HKCU\Environment\Path` no longer references the install dir, and the
app's real data directory (`%LOCALAPPDATA%\dev.operant.shell`, holding the
WebView2 profile and the `first-task` workflow saved during run 2 above)
still exists, untouched.

### 5. Reinstall attempt (to restore state / verify the install half of the lifecycle)

`Operant_0.1.0_x64-setup.exe /CURRENTUSER /S`

```
Start-Process : This command cannot be run due to the error: The operation was canceled by the user.
```

A live `consent.exe` (the Windows UAC prompt host) was observed while this
was hung; this agent declined it (did not approve elevation) rather than
attempt to click through it, then confirmed no install resulted. See
DECISIONS.

## ARTIFACTS

- `e2e/first-timer/release/run.mjs`: the release-mode harness (pre-existing
  from earlier work in this lane; fixed and hardened here, see DECISIONS).
- `e2e/first-timer/release/README.md`: full setup instructions, including
  the UAC finding below and the reproducible-build/install commands.
- `e2e/first-timer/.output/first-timer-release-guided-teach-done.png`,
  `first-timer-release-run-complete.png`: screenshots from the passing
  release-mode run, taken directly from the installed binary.
- `e2e/first-timer/.output/first-timer-guided-teach-done.png`,
  `first-timer-run-complete.png`: screenshots from the dev-server rerun.
- `e2e/first-timer/package.json`, `package-lock.json`: added
  `selenium-webdriver` as a proper declared devDependency (it was present
  in `node_modules` but undeclared before this lane; `npm ci` would have
  silently dropped it) and a `test:release` script.
- `e2e/first-timer/RESULT.md`: this file.

## DECISIONS

- Fixed a real bug in the inherited `release/run.mjs`, not just a tooling
  gap: it assumed a "fresh device" (empty `localStorage`) on every run, but
  drives a real installed app whose WebView2 profile persists on disk across
  runs; a second run against a non-fresh profile threw a confusing
  `stale element reference` error partway through instead of a clean
  first-screen mismatch. Fix: delete the app's WebView2 profile directory
  and kill any stray `operant-shell.exe`/`tauri-driver.exe`/
  `msedgedriver.exe` before every run. ADR-worthy if this pattern recurs in
  other release-artifact suites: any E2E driving a persistent installed app
  (not a disposable browser context) needs this same reset, on purpose, not
  by accident.
- Also hardened `findVisible`/added `clickWhenVisible` in `run.mjs` to retry
  on `StaleElementReferenceError` (bounded by the existing per-step
  timeout): this app re-renders live during the scripted interaction
  (streaming narration, a status indicator), so a located element can be
  detached before the next command uses it. Standard WebDriver-against-a-
  live-UI pattern, not a product bug.
- The task brief that dispatched this lane assumed a full build+install
  might not be runnable headless here at all. Reality was more specific:
  build, install-so-far-that-it-already-existed, and drive-via-tauri-driver
  all worked; a *fresh* install does not, because launching the installer
  triggers a Windows UAC elevation prompt that cannot be answered
  non-interactively, even when passed `/CURRENTUSER /S`. This is asymmetric
  with the generated uninstaller, which runs non-elevated cleanly with the
  matching flag. This agent verified the golden path against the real,
  already-installed binary (2 green runs, screenshots) before touching
  uninstall, so the core V5 bar does not depend on being able to reinstall.
- Honest state left on this machine: Operant is currently **uninstalled**
  as a direct result of this lane's uninstall verification step (item 4
  above), because this agent could not click through the one UAC prompt
  needed to reinstall it (item 5) and declined to attempt any workaround
  (installing it either needs a human at the keyboard for that one prompt,
  or a code-signed installer plus a policy that trusts it without prompting,
  neither of which is this agent's call to make). The exact one-line
  restore command is in `release/README.md`'s step 2.
- Uninstall does not delete real user data, but not for the reason the
  product intends: `release/nsis/installer-hooks.nsh`'s
  `NSIS_HOOK_PREUNINSTALL` prompts to remove `$APPDATA\Operant`, but this
  app's real WebView2/localStorage data lives at
  `%LOCALAPPDATA%\dev.operant.shell` (the Tauri `identifier`), a different
  path that the hook never touches either way. Read-only for this lane
  (owned paths are `e2e/first-timer` only); flagged as a FOLLOWUP.
- Confirmed independently of this lane's own scope, while reading
  `release/KEYS.md` per the brief's READ list: the NSIS installer itself has
  no Authenticode (OS-level) code signature (no certificate available on
  this build machine, per that file); only the auto-updater's Ed25519
  signature is real. "The signed installer" in this lane's dispatch prompt
  and in `campaign/MEGA_PROMPT.md`'s V5 line refers to that updater
  signature, not to Windows SmartScreen trusting the installer binary
  itself; a first run of the real installer shows an "unknown publisher"
  SmartScreen warning. Already documented plainly in `release/KEYS.md` and
  `release/RELEASE_NOTES_TEMPLATE.md` by earlier work; repeated here only so
  this lane's own report does not imply otherwise.

## FOLLOWUPS

1. `release/nsis/installer-hooks.nsh`'s user-data prompt targets
   `$APPDATA\Operant`; point it at the real per-identifier data dir (or
   read wherever the Rust core actually persists workflows/recordings,
   which may differ from the raw WebView2 profile) so "Remove saved
   workflows and recordings too?" does something.
2. Investigate whether the installer can be built to request
   `RequestExecutionLevel user` (or an equivalent non-elevated manifest) for
   the `/CURRENTUSER` path specifically, so a scripted/CI silent install is
   possible without a human clicking a UAC prompt; the generated
   uninstaller already manages this, so a per-mode manifest split is
   evidently possible for this NSIS template.
3. A person with a GUI session should run
   `Operant_0.1.0_x64-setup.exe /CURRENTUSER /S`, approve the one UAC
   prompt, and optionally rerun `npm run test:release` once more against
   that fresh install; this lane already proved the harness itself is
   correct and repeatable (2 green runs) against the same binary before it
   was uninstalled.
4. Consider adding a `release-mode` label to whatever the orchestrator's
   final-gate tooling reads for item 4 of `campaign/MEGA_PROMPT.md` section
   5 ("First-timer path (V5) green on the release artifact"), pointing at
   `npm run test:release` in this directory, so future reruns do not need
   to rediscover the `NATIVE_DRIVER_PATH`/profile-reset setup from scratch.
