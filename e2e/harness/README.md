# E2E Capture Harness

Shared rig for turning either a browser page or a native OS window into a
PNG or a GIF. Other packets and V1 reuse this instead of hand-rolling
Playwright config or an ffmpeg pipeline per feature.

## What it does

- **Browser-driven capture**: `src/serve.mjs` serves
  `contracts/fixtures/webapp/` (`index.html`, `drift.html`) over local HTTP.
  Playwright's `webServer` config starts it automatically, so tests just
  `page.goto('/')` and `page.screenshot()`.
- **Native-window capture**: `src/native-window.mjs` launches Notepad (the
  "victim app" -- always present on Windows, stable title) and captures it
  with ffmpeg's `gdigrab` device, since Playwright cannot see windows
  outside its own browser.
- **GIF pipeline**: `src/gif.mjs` runs ffmpeg's two-pass palettegen recipe
  (palette pass, then paletteuse): max width 800px, 12 fps, under 8 MB.
  Callers pass any source video (browser recording or the native `gdigrab`
  capture) and get a checked GIF back.

## Run

```bash
npm install   # also runs `playwright install chromium` via postinstall
npm test
```

`npm test` runs both Playwright specs under `tests/`:

- `webapp.spec.mjs` -- loads the fixture invoice app and its drift variant
  in headless Chromium, screenshots each to `.output/`.
- `native-capture.spec.mjs` -- launches Notepad, captures one PNG and one
  GIF of it end to end, asserts the GIF is under 8 MB and the PNG exists.
  Requires a real interactive desktop session (same requirement as headed
  browser tests); on a headless runner with no desktop it skips rather than
  failing, since it is proving the toolchain works on a machine that has a
  screen, not asserting every CI runner does.

Captured files land in `.output/` (gitignored, regenerated per run).

## Requirements

- Node >= 18
- `ffmpeg` on `PATH` (override with `FFMPEG_BIN`), built with the
  `gdigrab` input device (the default on Windows builds)
- Windows, for the native-window capture path (`notepad.exe`, `gdigrab`,
  `taskkill.exe`). The browser-driven path is platform-independent.

## Reuse

```js
import { videoToGif } from './src/gif.mjs';
import { launchNotepad, closeNotepad, captureWindowPng, recordWindowVideo } from './src/native-window.mjs';
```

`videoToGif(inputPath, outputPath, { width, fps, maxBytes })` throws if the
result exceeds `maxBytes` (default 8 MB), so callers get a hard failure
instead of a silently oversized asset.
