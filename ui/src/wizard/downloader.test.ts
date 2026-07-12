// Unit tests for the mocked local-model downloader (./downloader.ts):
// envelope shape, monotonic progress, pause/resume-from-offset, cancel, a
// failure path, and the compatibility/disk pure checks. No DOM: runs under
// plain `node --test`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { startDownload, probeCompatibility, checkDiskSpace, formatBytes, type DownloadEnvelope } from "./downloader.ts";

function collect(): { events: DownloadEnvelope[]; onEvent: (e: DownloadEnvelope) => void } {
  const events: DownloadEnvelope[] = [];
  return { events, onEvent: (e) => events.push(e) };
}

// Wait for a condition instead of a fixed wall-clock duration. The simulated
// download is timer-driven, so a fixed `setTimeout(60)` guess is flaky under
// load (the ticks slip past the deadline); polling for the actual condition
// makes these tests deterministic on any machine speed.
async function waitFor(predicate: () => boolean, timeoutMs = 2000, stepMs = 2): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) throw new Error("waitFor: condition not met within timeout");
    await new Promise((resolve) => setTimeout(resolve, stepMs));
  }
}

const hasTopic = (events: DownloadEnvelope[], topic: string) => events.some((e) => e.topic === topic);

test("a fresh download emits started, monotonic progress, then completed with a well-formed envelope", async () => {
  const { events, onEvent } = collect();
  startDownload({ totalBytes: 100, ticks: 5, tickMs: 3, onEvent });

  await waitFor(() => hasTopic(events, "download.completed"));

  assert.equal(events[0].topic, "download.started");
  const last = events[events.length - 1];
  assert.equal(last.topic, "download.completed");
  assert.equal(last.payload.bytesWritten, 100);
  assert.equal(last.payload.bytesTotal, 100);

  const percents = events.filter((e) => e.topic === "download.progress").map((e) => e.payload.percent as number);
  assert.equal(percents.length, 5, `expected exactly 5 progress ticks (100 bytes / 5 ticks), got ${percents.length}`);
  for (let i = 1; i < percents.length; i++) {
    assert.ok(percents[i] >= percents[i - 1], "percent must never go backwards");
  }
  assert.equal(percents[percents.length - 1], 100);

  for (const e of events) {
    assert.equal(e.v, 1);
    assert.ok(e.seq >= 1);
    assert.ok(typeof e.ts === "string" && e.ts.length > 0);
  }
  // seq strictly increasing, one per envelope.
  for (let i = 1; i < events.length; i++) {
    assert.ok(events[i].seq > events[i - 1].seq);
  }
});

test("pause keeps whatever arrived and resume continues from that offset, not from zero", async () => {
  const { events, onEvent } = collect();
  // tickMs is generous so there is ample room to pause mid-flight (after the
  // first progress tick, well before completion) without racing the timer.
  const handle = startDownload({ totalBytes: 120, ticks: 6, tickMs: 20, onEvent });

  // Pause as soon as the first progress has arrived: deterministically
  // mid-flight (one of six ticks done), never before any byte and never
  // already complete.
  await waitFor(() => hasTopic(events, "download.progress"));
  handle.pause();

  const paused = events.find((e) => e.topic === "download.paused");
  assert.ok(paused, "expected a download.paused envelope");
  const resumedFrom = paused!.payload.resumedFrom as number;
  assert.ok(resumedFrom > 0, "some bytes must have arrived before pause");
  assert.ok(resumedFrom < 120, "must not already be complete");

  // No further events fire while paused. Waiting several tick intervals makes
  // this absence assertion robust: if a tick were still scheduled it would have
  // fired well within this window.
  const countBeforeResume = events.length;
  await new Promise((resolve) => setTimeout(resolve, 120));
  assert.equal(events.length, countBeforeResume, "no further events while paused");

  handle.resume();
  const resumeStarted = events[events.length - 1];
  assert.equal(resumeStarted.topic, "download.started");
  assert.equal(resumeStarted.payload.resumedFrom, resumedFrom);

  await waitFor(() => hasTopic(events, "download.completed"));
  const completed = events.find((e) => e.topic === "download.completed");
  assert.ok(completed, "expected the download to finish after resuming");
  assert.equal(completed!.payload.bytesWritten, 120);
});

test("cancel stops all further events even if a tick was already scheduled", async () => {
  const { events, onEvent } = collect();
  const handle = startDownload({ totalBytes: 100, ticks: 4, tickMs: 20, onEvent });
  // Cancel mid-flight, with a next tick already scheduled by the progress emit.
  await waitFor(() => hasTopic(events, "download.progress"));
  handle.cancel();
  const countAtCancel = events.length;
  await new Promise((resolve) => setTimeout(resolve, 120));
  assert.equal(events.length, countAtCancel, "cancel must stop every future emit, not just clear the timer");
  assert.ok(!hasTopic(events, "download.completed"));
});

test("a failing transfer emits download.failed with the given code, and never completes", async () => {
  const { events, onEvent } = collect();
  startDownload({ totalBytes: 100, ticks: 5, tickMs: 5, failAt: 2, failCode: "CHECKSUM_MISMATCH", onEvent });

  await waitFor(() => hasTopic(events, "download.failed"));

  const failed = events.find((e) => e.topic === "download.failed");
  assert.ok(failed, "expected a download.failed envelope");
  assert.equal(failed!.payload.code, "CHECKSUM_MISMATCH");
  assert.ok(!hasTopic(events, "download.completed"));
});

test("probeCompatibility: below the minimum fails, below the slow threshold warns, otherwise ok", () => {
  assert.equal(probeCompatibility(2000, 4000, 6000).level, "fail");
  assert.equal(probeCompatibility(5000, 4000, 6000).level, "slow");
  assert.equal(probeCompatibility(8000, 4000, 6000).level, "ok");
  // Boundaries: exactly at a threshold counts as clearing it.
  assert.equal(probeCompatibility(4000, 4000, 6000).level, "slow");
  assert.equal(probeCompatibility(6000, 4000, 6000).level, "ok");
});

test("checkDiskSpace: ok exactly at the boundary, not ok below it, shortfall is the gap not the total", () => {
  assert.equal(checkDiskSpace(1000, 1000).ok, true);
  assert.equal(checkDiskSpace(1000, 1000).shortfallBytes, 0);
  assert.equal(checkDiskSpace(999, 1000).ok, false);
  assert.equal(checkDiskSpace(999, 1000).shortfallBytes, 1);
  assert.equal(checkDiskSpace(1_000_000_000, 4_000_000_000).shortfallBytes, 3_000_000_000);
});

test("formatBytes: readable GB/MB, never raw bytes", () => {
  assert.equal(formatBytes(1_500_000_000), "1.5 GB");
  assert.equal(formatBytes(250_000_000), "250 MB");
});
