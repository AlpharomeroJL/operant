// Bar test for the wizard-downloader sidecar. Run directly:
//   node sidecars/downloader/test/downloader.test.mjs
// (also runs via `npm test`, i.e. `node --test test/downloader.test.mjs`).
//
// Covers, mostly against a real 127.0.0.1 HTTP server (no network access):
//   1. fresh download: progress events, byte-identical output, checksum ok.
//   2. resumable interruption: abort partway, restart, the restart re-fetches
//      only the missing tail (tracked via the server's per-request byte
//      counters), and the final file is complete and checksum-valid.
//   3. checksum mismatch fails closed: bad content never reaches dest and no
//      partial file is left behind to falsely resume from later.
//   4. resume when the part file is already fully (and correctly) written,
//      e.g. a crash between finishing the transfer and verifying it.
//   5. the bare-path and file:// sources resume by byte offset like HTTP does.
//   6. the cli.mjs sidecar protocol: NDJSON on stdout, exit code, dest file.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import fsp from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

import { downloadFile, DownloadError, parseSumsFile } from "../index.mjs";
import { startRangeServer } from "./fixture-server.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "../../..");
const FIXTURE_DIR = path.join(REPO_ROOT, "contracts", "fixtures", "model_download");
const FIXTURE_FILE = path.join(FIXTURE_DIR, "model.bin");
const SUMS_FILE = path.join(FIXTURE_DIR, "SHA256SUMS");
const CLI_PATH = path.resolve(__dirname, "../cli.mjs");

const fixtureSize = fs.statSync(FIXTURE_FILE).size;
const expectedHash = parseSumsFile(fs.readFileSync(SUMS_FILE, "utf8")).get("model.bin");
assert.ok(expectedHash, "fixture setup: SHA256SUMS must have an entry for model.bin");

/** A fresh temp dir, removed automatically when the test finishes. */
function mkTmp(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "operant-downloader-test-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

/** Runs cli.mjs as a real child process and collects its NDJSON stdout. */
function runCli(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [CLI_PATH, ...args], { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (c) => (stdout += c));
    child.stderr.on("data", (c) => (stderr += c));
    child.on("error", reject);
    child.on("close", (code) => {
      resolve({ code, stdoutLines: stdout.split(/\r?\n/).filter(Boolean), stderr });
    });
  });
}

test("fixture sanity: SHA256SUMS matches the model.bin bytes on disk", async () => {
  const { sha256File } = await import("../index.mjs");
  const actual = await sha256File(FIXTURE_FILE);
  assert.equal(actual, expectedHash);
});

test("fresh download: progress events, byte-identical file, checksum verified", async (t) => {
  const { baseUrl, requests, close } = await startRangeServer(FIXTURE_FILE);
  t.after(() => close());
  const dest = path.join(mkTmp(t), "model.bin");

  const progressEvents = [];
  const result = await downloadFile({
    url: `${baseUrl}/model.bin`,
    dest,
    sumsFile: SUMS_FILE,
    onProgress: (p) => progressEvents.push(p),
  });

  assert.equal(result.alreadyComplete, false);
  assert.equal(result.resumedFrom, 0);
  assert.equal(result.bytesWritten, fixtureSize);
  assert.equal(result.bytesTotal, fixtureSize);
  assert.equal(result.sha256, expectedHash);
  assert.equal(result.path, dest);

  assert.ok(progressEvents.length > 0, "onProgress must fire at least once");
  for (const p of progressEvents) {
    assert.equal(typeof p.bytesReceived, "number");
    assert.ok(p.bytesTotal === null || typeof p.bytesTotal === "number");
    assert.ok(p.percent === null || (p.percent >= 0 && p.percent <= 100));
  }
  const last = progressEvents.at(-1);
  assert.equal(last.bytesReceived, fixtureSize);
  assert.equal(last.percent, 100);

  const [destBytes, fixtureBytes] = await Promise.all([fsp.readFile(dest), fsp.readFile(FIXTURE_FILE)]);
  assert.ok(destBytes.equals(fixtureBytes), "downloaded file must be byte-identical to the fixture");
  assert.equal(fs.existsSync(`${dest}.part`), false, "no leftover .part file after success");

  assert.equal(requests.length, 1);
  assert.equal(requests[0].rangeHeader, null, "a fresh download has no prior bytes to resume from");
  assert.equal(requests[0].bytesServed, fixtureSize);
});

test("resumable interruption: abort partway, restart, restart does not re-fetch bytes already on disk", async (t) => {
  // Throttled so a 256 KB fixture over loopback cannot finish before the
  // abort has a real window to land mid-transfer.
  const { baseUrl, requests, close } = await startRangeServer(FIXTURE_FILE, {
    throttle: { chunkBytes: 8192, delayMs: 10 },
  });
  t.after(() => close());
  const dest = path.join(mkTmp(t), "model.bin");
  const partPath = `${dest}.part`;

  const controller = new AbortController();
  const abortAfterBytes = Math.floor(fixtureSize * 0.3);
  let abortRequested = false;

  const firstAttempt = downloadFile({
    url: `${baseUrl}/model.bin`,
    dest,
    sumsFile: SUMS_FILE,
    signal: controller.signal,
    onProgress: (p) => {
      if (!abortRequested && p.bytesReceived >= abortAfterBytes) {
        abortRequested = true;
        controller.abort();
      }
    },
  });

  await assert.rejects(firstAttempt, (err) => {
    assert.ok(err instanceof DownloadError);
    assert.equal(err.code, "ABORTED");
    return true;
  });

  // Proof (client side) that the abort really landed mid-transfer: a
  // non-empty part file strictly short of the full size.
  const partialSize = fs.statSync(partPath).size;
  assert.ok(partialSize > 0, "expected some bytes to have landed on disk before the abort");
  assert.ok(partialSize < fixtureSize, "expected the abort to land before the file finished");
  assert.equal(fs.existsSync(dest), false, "dest must not exist until a download is verified");

  assert.equal(requests.length, 1, "the first (aborted) attempt is a single request");
  assert.equal(requests[0].rangeHeader, null, "the first attempt had nothing to resume from");
  assert.ok(requests[0].bytesServed < fixtureSize, "server side also saw the first request cut short");

  // Restart: a brand new downloadFile() call against the same dest, exactly
  // what a wizard "Resume" click (or a relaunch after a crash) would do.
  const secondResult = await downloadFile({
    url: `${baseUrl}/model.bin`,
    dest,
    sumsFile: SUMS_FILE,
  });

  assert.equal(secondResult.sha256, expectedHash);
  assert.equal(secondResult.resumedFrom, partialSize);
  assert.equal(secondResult.alreadyComplete, false);
  assert.equal(fs.existsSync(partPath), false, "the part file is renamed away once verified");

  const [destBytes, fixtureBytes] = await Promise.all([fsp.readFile(dest), fsp.readFile(FIXTURE_FILE)]);
  assert.ok(destBytes.equals(fixtureBytes), "the resumed file must be byte-identical to the fixture");

  // The actual guarantee under test: track bytes served on pass 2 and prove
  // it is only the remainder, never bytes pass 1 already delivered.
  assert.equal(requests.length, 2, "restart issues exactly one more request, no retries");
  const secondRequest = requests[1];
  assert.equal(
    secondRequest.rangeHeader,
    `bytes=${partialSize}-`,
    "restart must ask only for the bytes missing after what is already on disk"
  );
  assert.equal(secondRequest.status, 206, "server must honor the Range request as partial content");
  assert.equal(
    secondRequest.bytesServed,
    fixtureSize - partialSize,
    "pass 2 must serve exactly the missing tail, not the whole file again"
  );
});

test("checksum mismatch fails closed: bad content is rejected, dest is never created, part file is cleaned up", async (t) => {
  const tmp = mkTmp(t);
  const corruptSourcePath = path.join(tmp, "corrupt-source.bin");
  const corrupted = Buffer.from(await fsp.readFile(FIXTURE_FILE));
  corrupted[0] ^= 0xff; // same length, different bytes, different sha256
  await fsp.writeFile(corruptSourcePath, corrupted);

  const { baseUrl, close } = await startRangeServer(corruptSourcePath);
  t.after(() => close());

  // basename must be "model.bin" so the lookup in the real SHA256SUMS finds
  // the (correct, non-corrupted) expected digest.
  const dest = path.join(tmp, "model.bin");
  const partPath = `${dest}.part`;

  await assert.rejects(
    downloadFile({ url: `${baseUrl}/model.bin`, dest, sumsFile: SUMS_FILE }),
    (err) => {
      assert.ok(err instanceof DownloadError);
      assert.equal(err.code, "CHECKSUM_MISMATCH");
      assert.equal(err.expected, expectedHash);
      assert.notEqual(err.actual, expectedHash);
      return true;
    }
  );

  assert.equal(fs.existsSync(dest), false, "fail closed: dest must never appear on a checksum mismatch");
  assert.equal(fs.existsSync(partPath), false, "the bad part file must not survive to falsely resume from later");
});

test("resume when the part file already holds the full, correct bytes (crash after transfer, before verification)", async (t) => {
  const { baseUrl, requests, close } = await startRangeServer(FIXTURE_FILE);
  t.after(() => close());
  const dest = path.join(mkTmp(t), "model.bin");
  const partPath = `${dest}.part`;

  // Simulate a process that finished writing the part file but was killed
  // before the checksum + rename step, without going through downloadFile.
  await fsp.copyFile(FIXTURE_FILE, partPath);

  const result = await downloadFile({ url: `${baseUrl}/model.bin`, dest, sumsFile: SUMS_FILE });

  assert.equal(result.sha256, expectedHash);
  assert.equal(result.resumedFrom, fixtureSize);
  assert.equal(fs.existsSync(dest), true);
  assert.equal(fs.existsSync(partPath), false);

  const totalServed = requests.reduce((sum, r) => sum + r.bytesServed, 0);
  assert.equal(totalServed, 0, "no bytes should be re-fetched when the part file is already complete");
});

test("bare-path and file:// sources: resume by byte offset, no HTTP involved", async (t) => {
  const tmp = mkTmp(t);
  const dest = path.join(tmp, "model.bin");

  // A plain filesystem path as the source.
  const result = await downloadFile({ url: FIXTURE_FILE, dest, sumsFile: SUMS_FILE });
  assert.equal(result.sha256, expectedHash);
  assert.equal(result.resumedFrom, 0);

  // Re-downloading a file:// URL of the same source hits the idempotent
  // short circuit: already verified, nothing re-copied.
  const fileUrl = `file:///${FIXTURE_FILE.split(path.sep).join("/")}`;
  const already = await downloadFile({ url: fileUrl, dest, sumsFile: SUMS_FILE });
  assert.equal(already.alreadyComplete, true);

  // A genuine resume: pre-seed a partial .part file and confirm the copy
  // continues from that byte offset instead of restarting.
  fs.rmSync(dest, { force: true });
  const full = await fsp.readFile(FIXTURE_FILE);
  const partial = full.subarray(0, Math.floor(full.length / 3));
  await fsp.writeFile(`${dest}.part`, partial);

  const resumed = await downloadFile({ url: FIXTURE_FILE, dest, sumsFile: SUMS_FILE });
  assert.equal(resumed.resumedFrom, partial.length);
  assert.equal(resumed.sha256, expectedHash);

  const destBytes = await fsp.readFile(dest);
  assert.ok(destBytes.equals(full));
});

test("cli.mjs sidecar protocol: NDJSON progress on stdout, exit 0, verified file", async (t) => {
  const { baseUrl, close } = await startRangeServer(FIXTURE_FILE);
  t.after(() => close());
  const dest = path.join(mkTmp(t), "model.bin");

  const { code, stdoutLines, stderr } = await runCli(["--url", `${baseUrl}/model.bin`, "--dest", dest, "--sums", SUMS_FILE]);

  assert.equal(code, 0, `expected exit 0, stderr was: ${stderr}`);
  assert.ok(stdoutLines.length > 0, "expected at least one NDJSON line on stdout");

  const events = stdoutLines.map((line) => JSON.parse(line));
  for (const e of events) {
    assert.equal(e.v, 1);
    assert.equal(typeof e.seq, "number");
    assert.equal(typeof e.ts, "string");
    assert.ok(e.topic.startsWith("download."), `unexpected topic: ${e.topic}`);
  }
  const seqs = events.map((e) => e.seq);
  assert.deepEqual(seqs, [...seqs].sort((a, b) => a - b), "seq must be non-decreasing");

  assert.equal(events[0].topic, "download.started");
  const last = events.at(-1);
  assert.equal(last.topic, "download.completed");
  assert.equal(last.payload.sha256, expectedHash);
  assert.equal(last.payload.path, dest);

  const [destBytes, fixtureBytes] = await Promise.all([fsp.readFile(dest), fsp.readFile(FIXTURE_FILE)]);
  assert.ok(destBytes.equals(fixtureBytes));
});

test("cli.mjs sidecar protocol: checksum mismatch exits non-zero with a download.failed event", async (t) => {
  const tmp = mkTmp(t);
  const corruptSourcePath = path.join(tmp, "corrupt-source.bin");
  const corrupted = Buffer.from(await fsp.readFile(FIXTURE_FILE));
  corrupted[0] ^= 0xff;
  await fsp.writeFile(corruptSourcePath, corrupted);

  const { baseUrl, close } = await startRangeServer(corruptSourcePath);
  t.after(() => close());
  const dest = path.join(tmp, "model.bin");

  const { code, stdoutLines } = await runCli(["--url", `${baseUrl}/model.bin`, "--dest", dest, "--sums", SUMS_FILE]);

  assert.notEqual(code, 0, "a checksum mismatch must not exit 0");
  const events = stdoutLines.map((line) => JSON.parse(line));
  const last = events.at(-1);
  assert.equal(last.topic, "download.failed");
  assert.equal(last.payload.code, "CHECKSUM_MISMATCH");
  assert.equal(fs.existsSync(dest), false);
});
