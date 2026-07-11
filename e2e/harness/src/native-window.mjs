// OS-level screenshot fallback for native windows that Playwright cannot
// see (it only drives browser pages). Uses Notepad as the "victim app":
// something guaranteed present on any Windows dev/CI box, with a stable,
// predictable window title, so this module never depends on product code.
//
// Capture is via ffmpeg's gdigrab device (Windows GDI screen/window
// grabber), which needs a real interactive desktop session -- same
// requirement Playwright's headed mode would have. Headless CI without a
// desktop session cannot run this; callers should skip rather than fail
// in that case (see tests/native-capture.spec.mjs).

import { spawn, execFile } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

const NOTEPAD_TITLE = 'Untitled - Notepad';

function execFileP(cmd, args) {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { windowsHide: true, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) {
        err.stderr = stderr;
        reject(err);
        return;
      }
      resolve({ stdout, stderr });
    });
  });
}

async function findNotepadPid() {
  const { stdout } = await execFileP('powershell.exe', [
    '-NoProfile', '-Command',
    "Get-Process -Name Notepad -ErrorAction SilentlyContinue | " +
    "Where-Object { $_.MainWindowHandle -ne 0 } | " +
    "Select-Object -First 1 -ExpandProperty Id",
  ]);
  const pid = Number(stdout.trim());
  return Number.isFinite(pid) && pid > 0 ? pid : null;
}

/**
 * Launch Notepad and wait for its window to be ready for capture.
 * @returns {Promise<{pid: number, title: string}>}
 */
export async function launchNotepad({ timeoutMs = 10_000 } = {}) {
  spawn('notepad.exe', [], { detached: true, stdio: 'ignore' }).unref();

  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const pid = await findNotepadPid();
    if (pid) return { pid, title: NOTEPAD_TITLE };
    await delay(250);
  }
  throw new Error('launchNotepad: notepad.exe window did not become ready in time');
}

/** Force-close a process started with launchNotepad. */
export async function closeNotepad(pid) {
  try {
    await execFileP('taskkill.exe', ['/PID', String(pid), '/F']);
  } catch {
    // Already gone; nothing to clean up.
  }
}

// Crop to even width/height: libx264 (used for the intermediate video) and
// several GIF filters reject odd dimensions.
const EVEN_CROP = 'crop=trunc(iw/2)*2:trunc(ih/2)*2';

/** Single-frame PNG screenshot of the named window. */
export async function captureWindowPng(title, outPath) {
  await execFileP(process.env.FFMPEG_BIN || 'ffmpeg', [
    '-y',
    '-f', 'gdigrab', '-framerate', '1', '-i', `title=${title}`,
    '-vf', EVEN_CROP,
    '-vframes', '1', '-update', '1',
    outPath,
  ]);
}

/** Short video recording of the named window, for GIF conversion. */
export async function recordWindowVideo(title, outPath, { seconds = 2, fps = 12 } = {}) {
  await execFileP(process.env.FFMPEG_BIN || 'ffmpeg', [
    '-y',
    '-f', 'gdigrab', '-framerate', String(fps), '-i', `title=${title}`,
    '-t', String(seconds),
    '-vf', EVEN_CROP,
    '-pix_fmt', 'yuv420p',
    outPath,
  ]);
}

export { NOTEPAD_TITLE };
