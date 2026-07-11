// ffmpeg two-pass palettegen GIF pipeline (see
// .claude/skills/operant-capture/SKILL.md for the recipe this implements):
// max width 800px, under 8 MB, 12 fps. Pass 1 builds an optimized palette
// from the source video; pass 2 applies it. Two passes give noticeably
// cleaner color than a single-pass gif encode, which matters for UI
// screenshots (text edges, thin borders).

import { execFile } from 'node:child_process';
import { mkdtemp, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const FFMPEG = process.env.FFMPEG_BIN || 'ffmpeg';

function run(args) {
  return new Promise((resolve, reject) => {
    execFile(FFMPEG, args, { maxBuffer: 64 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) {
        err.stderr = stderr;
        reject(err);
        return;
      }
      resolve({ stdout, stderr });
    });
  });
}

/**
 * Convert a video file to a GIF via two-pass palettegen.
 * @param {string} inputPath source video (e.g. mp4 from ffmpeg gdigrab)
 * @param {string} outputPath destination .gif path
 * @param {{width?: number, fps?: number, maxBytes?: number}} [opts]
 */
export async function videoToGif(inputPath, outputPath, opts = {}) {
  const width = opts.width ?? 800;
  const fps = opts.fps ?? 12;
  const maxBytes = opts.maxBytes ?? 8 * 1024 * 1024;

  const workDir = await mkdtemp(join(tmpdir(), 'operant-gif-'));
  const palettePath = join(workDir, 'palette.png');
  const scaleFilter = `scale='min(${width},iw)':-1:flags=lanczos`;

  try {
    await run([
      '-y', '-i', inputPath,
      '-vf', `fps=${fps},${scaleFilter},palettegen`,
      '-update', '1',
      palettePath,
    ]);

    await run([
      '-y', '-i', inputPath, '-i', palettePath,
      '-filter_complex', `fps=${fps},${scaleFilter}[x];[x][1:v]paletteuse`,
      outputPath,
    ]);

    const info = await stat(outputPath);
    if (info.size > maxBytes) {
      throw new Error(
        `videoToGif: ${outputPath} is ${info.size} bytes, over the ${maxBytes} byte cap ` +
        `(width=${width}, fps=${fps}). Reduce width, fps, or clip duration.`
      );
    }
    return { path: outputPath, bytes: info.size };
  } finally {
    await rm(workDir, { recursive: true, force: true });
  }
}
