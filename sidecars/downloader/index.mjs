// Resumable, checksum-verified file downloader.
//
// Used by the zero-code wizard's "Download a free brain" setup path to fetch
// a local model file. See README.md in this directory for the full protocol
// (progress events, checksum verification, resume semantics) that cli.mjs
// exposes to non-Node callers.
//
// Zero dependencies beyond Node core (node:http, node:https, node:fs,
// node:crypto, node:path, node:url) so it runs headless with no toolchain
// install, per the lane brief.

import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Error raised by this module. `code` is one of the constants below. */
export class DownloadError extends Error {
  constructor(code, message, extra = {}) {
    super(message);
    this.name = "DownloadError";
    this.code = code;
    Object.assign(this, extra);
  }
}

export const DownloadErrorCode = Object.freeze({
  BAD_ARGS: "BAD_ARGS",
  NOT_FOUND: "NOT_FOUND",
  HTTP_ERROR: "HTTP_ERROR",
  READ_ERROR: "READ_ERROR",
  WRITE_ERROR: "WRITE_ERROR",
  ABORTED: "ABORTED",
  CHECKSUM_MISMATCH: "CHECKSUM_MISMATCH",
  SUMS_ENTRY_MISSING: "SUMS_ENTRY_MISSING",
  UNKNOWN: "UNKNOWN",
});

const EM_DASH = String.fromCharCode(0x2014);

/** Streams `filePath` through SHA-256 and returns the lowercase hex digest. */
export function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    const stream = fs.createReadStream(filePath);
    stream.on("error", reject);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("end", () => resolve(hash.digest("hex")));
  });
}

/**
 * Parses a standard `sha256sum`-style checksum file: one entry per line,
 * `<64-hex-digest>  <filename>` (two-space text mode) or
 * `<64-hex-digest> *<filename>` (binary mode). Blank lines and `#` comments
 * are ignored. Returns a Map from filename to lowercase hex digest.
 */
export function parseSumsFile(text) {
  const map = new Map();
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    const m = /^([0-9a-fA-F]{64})\s+\*?(.+)$/.exec(line);
    if (!m) continue;
    const [, hex, name] = m;
    map.set(name.trim(), hex.toLowerCase());
  }
  return map;
}

function lookupSha256InSumsFile(sumsFile, filename) {
  let text;
  try {
    text = fs.readFileSync(sumsFile, "utf8");
  } catch (err) {
    throw new DownloadError("NOT_FOUND", `sums file not found: ${sumsFile}`, { cause: err });
  }
  const map = parseSumsFile(text);
  const hex = map.get(filename);
  if (!hex) {
    throw new DownloadError(
      "SUMS_ENTRY_MISSING",
      `no checksum entry for "${filename}" in ${sumsFile}`
    );
  }
  return hex;
}

function isAbortError(err) {
  return Boolean(err) && (err.name === "AbortError" || err.code === "ABORT_ERR");
}

function classifySource(url) {
  if (/^https:\/\//i.test(url)) return "https";
  if (/^http:\/\//i.test(url)) return "http";
  if (/^file:\/\//i.test(url)) return "file-url";
  return "file-path";
}

function parseTotalFromContentRange(headerVal) {
  if (!headerVal) return null;
  const m = /\/(\d+)\s*$/.exec(headerVal);
  return m ? Number(m[1]) : null;
}

// ---------------------------------------------------------------------------
// HTTP(S) transfer
// ---------------------------------------------------------------------------

/**
 * Fetches `url` into `partPath`, resuming via `Range: bytes=<existingBytes>-`
 * when `existingBytes > 0`. If the server ignores the Range header and
 * answers 200 instead of 206, the part file is rewritten from scratch rather
 * than corrupted by blind appending. Resolves `{ bytesReceived, bytesTotal }`
 * once the response body has been fully flushed to disk (does not verify
 * checksum; the caller does that).
 */
function downloadHttp({ url, partPath, existingBytes, onProgress, signal, maxRedirects = 5 }) {
  return new Promise((resolve, reject) => {
    const urlObj = new URL(url);
    const transport = urlObj.protocol === "https:" ? https : http;
    const wantsRange = existingBytes > 0;
    const headers = wantsRange ? { Range: `bytes=${existingBytes}-` } : {};

    let settled = false;
    let out = null;

    const settleResolve = (value) => {
      if (settled) return;
      settled = true;
      resolve(value);
    };
    const settleReject = (err) => {
      if (settled) return;
      settled = true;
      reject(err);
    };

    const finishWith = (fn) => {
      if (out) {
        out.end(fn);
      } else {
        fn();
      }
    };

    const req = transport.request(urlObj, { headers, signal }, (res) => {
      const status = res.statusCode || 0;

      if (status >= 300 && status < 400 && res.headers.location) {
        res.resume();
        if (maxRedirects <= 0) {
          settleReject(new DownloadError("HTTP_ERROR", "too many redirects"));
          return;
        }
        const nextUrl = new URL(res.headers.location, urlObj).toString();
        downloadHttp({ url: nextUrl, partPath, existingBytes, onProgress, signal, maxRedirects: maxRedirects - 1 }).then(
          settleResolve,
          settleReject
        );
        return;
      }

      if (wantsRange && status === 416) {
        // The server is saying "nothing left past existingBytes": the part
        // file already covers the whole resource. This happens when a prior
        // run finished writing the part file but was interrupted before the
        // checksum + rename step. Treat as already-fetched; the caller's
        // checksum step is what decides whether that content is actually
        // valid, not this transport layer.
        res.resume();
        const bytesTotal = parseTotalFromContentRange(res.headers["content-range"]) ?? existingBytes;
        settleResolve({ bytesReceived: existingBytes, bytesTotal });
        return;
      }

      let append;
      let bytesTotal;
      let bytesReceived;

      if (wantsRange && status === 206) {
        append = true;
        bytesReceived = existingBytes;
        bytesTotal =
          parseTotalFromContentRange(res.headers["content-range"]) ??
          (existingBytes + Number(res.headers["content-length"] || 0) || null);
      } else if (status === 200) {
        // Either a fresh download, or the server does not support Range and
        // sent the whole file again: either way, start the part file over.
        append = false;
        bytesReceived = 0;
        bytesTotal = Number(res.headers["content-length"] || 0) || null;
      } else {
        res.resume();
        settleReject(new DownloadError("HTTP_ERROR", `unexpected status ${status}`));
        return;
      }

      out = fs.createWriteStream(partPath, { flags: append ? "a" : "w" });

      const onStreamAbortish = (err) => {
        if (settled) return;
        if (isAbortError(err) || (signal && signal.aborted)) {
          finishWith(() =>
            settleReject(new DownloadError("ABORTED", "download aborted", { bytesReceived }))
          );
        } else {
          finishWith(() => settleReject(new DownloadError("HTTP_ERROR", err ? err.message : "response aborted")));
        }
      };

      res.on("data", (chunk) => {
        bytesReceived += chunk.length;
        if (onProgress) {
          onProgress({
            bytesReceived,
            bytesTotal,
            percent: bytesTotal ? (bytesReceived / bytesTotal) * 100 : null,
          });
        }
      });
      res.on("error", onStreamAbortish);
      res.on("aborted", () => onStreamAbortish(null));
      res.pipe(out);

      out.on("error", (err) => finishWith(() => settleReject(new DownloadError("WRITE_ERROR", err.message))));
      out.on("finish", () => settleResolve({ bytesReceived, bytesTotal }));
    });

    req.on("error", (err) => {
      if (isAbortError(err) || (signal && signal.aborted)) {
        finishWith(() =>
          settleReject(new DownloadError("ABORTED", "download aborted before completion", { bytesReceived: existingBytes }))
        );
      } else {
        finishWith(() => settleReject(new DownloadError("HTTP_ERROR", err.message)));
      }
    });

    req.end();
  });
}

// ---------------------------------------------------------------------------
// file:// / bare-path transfer
// ---------------------------------------------------------------------------

/**
 * Copies `srcPath` into `partPath` starting at `existingBytes`, i.e. a
 * byte-offset append resume for local-file sources (mirrors the HTTP Range
 * path above for the "copy from a file URL" case).
 */
function downloadFileSource({ srcPath, partPath, existingBytes, onProgress, signal }) {
  return new Promise((resolve, reject) => {
    let stat;
    try {
      stat = fs.statSync(srcPath);
    } catch (err) {
      reject(new DownloadError("NOT_FOUND", `source not found: ${srcPath}`, { cause: err }));
      return;
    }

    const bytesTotal = stat.size;
    if (existingBytes >= bytesTotal) {
      resolve({ bytesReceived: existingBytes, bytesTotal });
      return;
    }

    let settled = false;
    let bytesReceived = existingBytes;
    const src = fs.createReadStream(srcPath, { start: existingBytes });
    const out = fs.createWriteStream(partPath, { flags: existingBytes > 0 ? "a" : "w" });

    const settleResolve = (value) => {
      if (settled) return;
      settled = true;
      resolve(value);
    };
    const settleReject = (err) => {
      if (settled) return;
      settled = true;
      reject(err);
    };

    const onAbort = () => {
      src.destroy();
      out.end(() => settleReject(new DownloadError("ABORTED", "download aborted", { bytesReceived })));
    };

    if (signal) {
      if (signal.aborted) {
        onAbort();
        return;
      }
      signal.addEventListener("abort", onAbort, { once: true });
    }

    src.on("data", (chunk) => {
      bytesReceived += chunk.length;
      if (onProgress) {
        onProgress({
          bytesReceived,
          bytesTotal,
          percent: bytesTotal ? (bytesReceived / bytesTotal) * 100 : null,
        });
      }
    });
    src.on("error", (err) => {
      out.end(() => settleReject(new DownloadError("READ_ERROR", err.message)));
    });
    out.on("error", (err) => {
      settleReject(new DownloadError("WRITE_ERROR", err.message));
    });
    out.on("finish", () => settleResolve({ bytesReceived, bytesTotal }));

    src.pipe(out);
  });
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Downloads `url` to `dest`, resuming an interrupted attempt and verifying
 * SHA-256 before the file is considered real. Fails closed: on a checksum
 * mismatch the partial file is deleted and `dest` is never written or left
 * in place, so a caller can never observe a corrupt file at `dest`.
 *
 * @param {object} opts
 * @param {string} opts.url - `http://`, `https://`, `file://`, or a bare filesystem path.
 * @param {string} opts.dest - Destination file path.
 * @param {string} [opts.sha256] - Expected lowercase hex digest. Takes priority over `sumsFile`.
 * @param {string} [opts.sumsFile] - Path to a SHA256SUMS-style file; looked up by `basename(dest)`.
 * @param {(p: {bytesReceived: number, bytesTotal: number|null, percent: number|null}) => void} [opts.onProgress]
 * @param {AbortSignal} [opts.signal] - Abort to pause: the partial file is kept for a later resume.
 * @param {string} [opts.partSuffix] - Suffix for the in-progress file. Default `.part`.
 * @returns {Promise<{path: string, bytesWritten: number, bytesTotal: number|null, sha256: string, resumedFrom: number, alreadyComplete: boolean}>}
 */
export async function downloadFile(opts) {
  const { url, dest, sha256: expectedShaOpt, sumsFile, onProgress, signal, partSuffix = ".part" } = opts || {};

  if (!url) throw new DownloadError("BAD_ARGS", "url is required");
  if (!dest) throw new DownloadError("BAD_ARGS", "dest is required");
  if (signal && signal.aborted) throw new DownloadError("ABORTED", "aborted before start", { bytesReceived: 0 });

  const partPath = dest + partSuffix;
  const filename = path.basename(dest);

  const expectedSha256 = expectedShaOpt
    ? expectedShaOpt.toLowerCase()
    : sumsFile
      ? lookupSha256InSumsFile(sumsFile, filename)
      : null;

  // Idempotent short-circuit: a prior run already produced a verified dest.
  if (expectedSha256 && fs.existsSync(dest)) {
    const existingHash = await sha256File(dest);
    if (existingHash === expectedSha256) {
      const size = fs.statSync(dest).size;
      if (onProgress) onProgress({ bytesReceived: size, bytesTotal: size, percent: 100 });
      return {
        path: dest,
        bytesWritten: size,
        bytesTotal: size,
        sha256: existingHash,
        resumedFrom: 0,
        alreadyComplete: true,
      };
    }
  }

  await fs.promises.mkdir(path.dirname(dest), { recursive: true });

  let existingBytes = 0;
  try {
    existingBytes = (await fs.promises.stat(partPath)).size;
  } catch {
    existingBytes = 0;
  }

  const kind = classifySource(url);
  let result;
  try {
    if (kind === "http" || kind === "https") {
      result = await downloadHttp({ url, partPath, existingBytes, onProgress, signal });
    } else {
      const srcPath = kind === "file-url" ? fileURLToPath(url) : url;
      result = await downloadFileSource({ srcPath, partPath, existingBytes, onProgress, signal });
    }
  } catch (err) {
    if (err instanceof DownloadError) {
      if (err.resumedFrom === undefined) err.resumedFrom = existingBytes;
      throw err;
    }
    throw new DownloadError("UNKNOWN", err.message, { cause: err, resumedFrom: existingBytes });
  }

  const actualSha256 = await sha256File(partPath);
  if (expectedSha256 && actualSha256 !== expectedSha256) {
    await fs.promises.rm(partPath, { force: true });
    throw new DownloadError(
      "CHECKSUM_MISMATCH",
      `checksum mismatch for ${filename}: expected ${expectedSha256}, got ${actualSha256}`,
      { expected: expectedSha256, actual: actualSha256, resumedFrom: existingBytes }
    );
  }

  // Replace any stale dest (e.g. left over from a run with no checksum to
  // trust it against) atomically with the freshly verified part file.
  await fs.promises.rm(dest, { force: true });
  await fs.promises.rename(partPath, dest);

  return {
    path: dest,
    bytesWritten: result.bytesReceived,
    bytesTotal: result.bytesTotal,
    sha256: actualSha256,
    resumedFrom: existingBytes,
    alreadyComplete: false,
  };
}

// Guard against accidentally reintroducing the character check-emdash.mjs
// forbids repo-wide; defined by code point, not a literal, same as that script.
export const FORBIDDEN_EM_DASH = EM_DASH;
