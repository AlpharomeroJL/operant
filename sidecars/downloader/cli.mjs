#!/usr/bin/env node
// Sidecar entry point: wraps downloadFile() (index.mjs) as a subprocess a
// non-Node host (the Tauri UI, via a sidecar-style spawn) can invoke.
//
// Protocol summary (full detail in README.md):
//   - One download per invocation.
//   - stdout carries ONLY newline-delimited JSON envelopes, one per line:
//       { "v": 1, "seq": <int>, "ts": <ISO8601>, "topic": <string>, "payload": {} }
//   - stderr carries human-readable diagnostics only (usage errors); never
//     part of the protocol, safe to ignore or log verbatim.
//   - Exit codes: 0 success, 1 download failed, 2 bad usage, 130 interrupted.
//
// Zero dependencies beyond Node core, per the lane brief.

import fs from "node:fs";
import { downloadFile, DownloadError } from "./index.mjs";

const USAGE = `Usage: node cli.mjs --url <url> --dest <path> [--sums <file> | --sha256 <hex>] [options]

Downloads <url> to <path>, resuming an interrupted attempt instead of starting
over, and verifying the result against a SHA256SUMS-style file (--sums) or an
explicit digest (--sha256). Progress and the result are reported as
newline-delimited JSON events on stdout; this is the sidecar protocol
documented in README.md. Diagnostics for bad usage go to stderr only.

Required:
  --url <url>             http://, https://, file://, or a bare filesystem path
  --dest <path>            destination file path

Verification (skipping both disables checksum verification entirely):
  --sums <path>            a SHA256SUMS-style file; looked up by basename(dest)
  --sha256 <hex>           expected digest directly; takes priority over --sums

Options:
  --part-suffix <s>            suffix for the in-progress file (default: .part)
  --progress-interval-ms <n>   minimum ms between download.progress events (default: 100)
  --help, -h                    print this message and exit 0

Exit codes: 0 success, 1 download failed, 2 bad usage, 130 interrupted (SIGINT/SIGTERM).
`;

function parseArgs(argv) {
  const out = { help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") {
      out.help = true;
      continue;
    }
    if (!arg.startsWith("--")) {
      throw new Error(`unexpected argument: ${arg}`);
    }
    const eq = arg.indexOf("=");
    if (eq !== -1) {
      out[arg.slice(2, eq)] = arg.slice(eq + 1);
      continue;
    }
    const key = arg.slice(2);
    const value = argv[i + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`missing value for --${key}`);
    }
    out[key] = value;
    i++;
  }
  return out;
}

let seq = 0;
/** Writes one envelope as a single NDJSON line on stdout. */
function emit(topic, payload) {
  const envelope = { v: 1, seq: seq++, ts: new Date().toISOString(), topic, payload };
  process.stdout.write(`${JSON.stringify(envelope)}\n`);
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`${err.message}\n\n${USAGE}`);
    process.exitCode = 2;
    return;
  }

  if (args.help) {
    process.stdout.write(USAGE);
    process.exitCode = 0;
    return;
  }

  if (!args.url || !args.dest) {
    process.stderr.write(`--url and --dest are required\n\n${USAGE}`);
    process.exitCode = 2;
    return;
  }

  const partSuffix = args["part-suffix"] || ".part";
  const progressIntervalMs = args["progress-interval-ms"] !== undefined ? Number(args["progress-interval-ms"]) : 100;
  const dest = args.dest;

  let resumedFromHint = 0;
  try {
    resumedFromHint = fs.statSync(dest + partSuffix).size;
  } catch {
    resumedFromHint = 0;
  }

  emit("download.started", {
    url: args.url,
    dest,
    sums: args.sums || null,
    sha256: args.sha256 || null,
    resumedFrom: resumedFromHint,
  });

  const controller = new AbortController();
  let interrupted = false;
  const requestPause = (signalName) => {
    if (interrupted) return;
    interrupted = true;
    // Best effort: SIGTERM is not delivered on Windows (the process is
    // simply terminated), so this handler may never run there. That is
    // fine; a hard kill is equally safe. See README.md, "Pausing a
    // download".
    controller.abort(new Error(`paused by ${signalName}`));
  };
  process.on("SIGINT", () => requestPause("SIGINT"));
  process.on("SIGTERM", () => requestPause("SIGTERM"));

  let lastEmitMs = 0;
  const onProgress = (progress) => {
    const now = Date.now();
    const isFinal = progress.bytesTotal != null && progress.bytesReceived >= progress.bytesTotal;
    if (!isFinal && now - lastEmitMs < progressIntervalMs) return;
    lastEmitMs = now;
    emit("download.progress", progress);
  };

  try {
    const result = await downloadFile({
      url: args.url,
      dest,
      sha256: args.sha256,
      sumsFile: args.sums,
      partSuffix,
      signal: controller.signal,
      onProgress,
    });
    emit("download.completed", result);
    process.exitCode = 0;
  } catch (err) {
    if (err instanceof DownloadError && err.code === "ABORTED" && interrupted) {
      emit("download.paused", { resumedFrom: err.resumedFrom ?? null });
      process.exitCode = 130;
      return;
    }
    if (err instanceof DownloadError) {
      emit("download.failed", { code: err.code, message: err.message, resumedFrom: err.resumedFrom ?? null });
      process.exitCode = 1;
      return;
    }
    emit("download.failed", { code: "UNKNOWN", message: err.message, resumedFrom: null });
    process.exitCode = 1;
  }
}

main();
