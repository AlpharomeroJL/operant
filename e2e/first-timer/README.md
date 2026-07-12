# First-Timer Golden Path E2E

Release-blocking proof of the zero-code first-timer path (docs/specs/zero-code.md):
a person with nothing configured drives the wizard demo path, watches a demo run,
teaches Operant a real task, saves it, runs it, and schedules it, entirely through
the real `ui/` shell (`ui/src/main.ts`) in a headless browser.

## What it asserts

- No code or terminal surface is ever visible. The shell has exactly one such
  surface (the Advanced toggle, `#op-mode-toggle`, and the panel it reveals,
  `#op-advanced-panel`); this suite never opens it and checks its state stays
  closed after every step.
- The whole scripted interaction finishes well inside a 15-minute budget. The
  wizard's guided task (`ui/src/wizard/guidedTask.ts`) and the library's saved-
  workflow replay (`ui/src/library/state.ts`) both use timer-driven mocks, never
  a live model call, so this is a ceiling check on the click path, not a
  benchmark.

Screen flow, matching the brief's order (wizard demo path, demo run, guided
teach, compile, run, schedule):

```
welcome -> setup_path (demo link) -> guided_task[demo]   (demo run)
        -> setup_path -> mic_check -> guided_task[real]  (guided teach)
        -> schedule (Save as workflow = compile)
        -> [wizard dismissed on Save this schedule]
        -> library screen -> run the saved workflow      (run)
```

The wizard's own schedule screen is the last screen inside the modal;
finishing it is the only way to dismiss the modal and reach the main shell's
Library screen, where a saved workflow can actually be run
(`#op-wizard-backdrop` covers the whole shell while the wizard is open). So
"run" happens right after "schedule" is chosen and saved, not literally
between compile and schedule: there is no in-modal path to a second run.

## Run

```bash
npm install   # also runs `playwright install chromium` via postinstall
npm test
```

`npm test` starts the real `ui/` app via its own Vite dev server (installing
`ui/`'s own dependencies first if needed, so no separate manual setup step is
required) and drives it headless in Chromium. Screenshots at the guided-teach
and final checkpoints land in `.output/` (gitignored, regenerated per run).

Override the dev server port with `FIRST_TIMER_PORT` if the default (4415)
collides with something else on the machine.

## Fixtures

- `contracts/fixtures/webapp/index.html`: the practice invoice page the
  guided task narrates against ("Type ... into Customer/Amount/Date", "Click
  Save invoice"). This suite does not load that page directly (the wizard's
  guided task simulates the run on the bus, the same way
  `ui/src/bus/mockClient.ts`'s own demo does); it asserts on the narrated
  sentences the real renderer produces from that scripted interaction.
- The demo-mode fixture workflow wired in `ui/src/wizard/guidedTask.ts`
  (`GUIDED_TASK_STEPS`, goal "Fill out a sample invoice on the practice page").

## Reuse

Package layout and Playwright config mirror `e2e/harness`: fixed default
port, single worker, `postinstall` installs the Chromium browser. Screenshot
capture into `.output/` follows the same convention as
`e2e/harness/tests/webapp.spec.mjs`. `e2e/harness`'s GIF pipeline
(`src/gif.mjs`) is available for richer capture later (see
`.claude/skills/operant-capture/SKILL.md`) but is not wired in here: it needs
`ffmpeg` on `PATH`, and this suite's pass/fail bar does not depend on it.
