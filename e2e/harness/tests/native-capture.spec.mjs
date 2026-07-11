// End-to-end proof of the OS-level capture fallback: launches Notepad (a
// real native window Playwright cannot see), captures one PNG and one GIF
// of it via ffmpeg gdigrab + the two-pass palettegen pipeline, and asserts
// the outputs are usable (PNG exists, GIF under the 8 MB cap). This is the
// path other packets and V1 reuse for native-window README assets that
// aren't browser pages.
//
// gdigrab needs a real interactive desktop session. On a headless runner
// with no desktop (no HWND ever appears), this test skips instead of
// failing -- it is proving the toolchain works on a machine that has one,
// not asserting every CI runner has a screen.
import { test, expect } from '@playwright/test';
import { mkdir, mkdtemp, stat, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import {
  launchNotepad,
  closeNotepad,
  captureWindowPng,
  recordWindowVideo,
} from '../src/native-window.mjs';
import { videoToGif } from '../src/gif.mjs';

const outDir = fileURLToPath(new URL('../.output', import.meta.url));
const MAX_GIF_BYTES = 8 * 1024 * 1024;

test.beforeAll(async () => {
  await mkdir(outDir, { recursive: true });
});

test('captures a native Notepad window as PNG and GIF', async () => {
  test.setTimeout(60_000);

  let notepad;
  try {
    notepad = await launchNotepad();
  } catch (err) {
    test.skip(true, `no interactive desktop session available for gdigrab: ${err.message}`);
    return;
  }

  const workDir = await mkdtemp(join(tmpdir(), 'operant-native-capture-'));
  try {
    const pngPath = join(outDir, 'notepad.png');
    await captureWindowPng(notepad.title, pngPath);

    const rawVideoPath = join(workDir, 'notepad-raw.mp4');
    await recordWindowVideo(notepad.title, rawVideoPath, { seconds: 2, fps: 12 });

    const gifPath = join(outDir, 'notepad.gif');
    const gif = await videoToGif(rawVideoPath, gifPath, { width: 800, fps: 12, maxBytes: MAX_GIF_BYTES });

    const pngInfo = await stat(pngPath);
    expect(pngInfo.size).toBeGreaterThan(0);

    expect(gif.bytes).toBeGreaterThan(0);
    expect(gif.bytes).toBeLessThan(MAX_GIF_BYTES);
  } finally {
    await closeNotepad(notepad.pid);
    await rm(workDir, { recursive: true, force: true });
  }
});
