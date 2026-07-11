// The wizard's core scripted-run tests (C19 bar): a run through the quiet
// demo link reaches a working demo in default mode, and a run through a
// real setup path reaches completion and publishes a workflow the library
// (ui/src/library/state.ts) can pick up, plus coverage of the trickier
// branches (local download's disk/compatibility gates and pause/resume,
// access-key provider detection wiring, schedule gating). No DOM: runs
// under plain `node --test`, same split as every other state module in
// ui/src.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { createWizard } from "./state.ts";

function waitUntil(subscribe: (fn: () => void) => () => void, predicate: () => boolean): Promise<void> {
  if (predicate()) return Promise.resolve();
  return new Promise((resolve) => {
    const unsubscribe = subscribe(() => {
      if (predicate()) {
        unsubscribe();
        resolve();
      }
    });
  });
}

test("welcome is the first screen, with real visible content, in default mode", () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus);
  const snap = wizard.getSnapshot();

  assert.equal(snap.screen, "welcome");
  assert.equal(snap.complete, false);
  assert.equal(snap.welcome.heading, "Welcome to Operant");
  assert.ok(snap.welcome.body.length > 0);
  assert.ok(snap.welcome.continueButton.length > 0);

  wizard.dispose();
});

test("BAR: a scripted wizard run reaches a working demo in default mode via the quiet demo link, with zero grants", async () => {
  const bus = createMockBusClient();
  const approvalEvents: BusEvent[] = [];
  bus.subscribe("approval", (e) => approvalEvents.push(e));

  const wizard = createWizard(bus, { guidedTaskStepDelayMs: 3 });

  // Screen 1: welcome.
  assert.equal(wizard.getSnapshot().screen, "welcome");
  wizard.continueWelcome();

  // Screen 2: setup path, "Just show me a demo": zero grants, watch it work
  // before configuring anything, per docs/specs/zero-code.md and the U2B
  // lane brief. Skips straight to the guided task, bypassing mic check.
  assert.equal(wizard.getSnapshot().screen, "setup_path");
  wizard.startDemo();

  let snap = wizard.getSnapshot();
  assert.equal(snap.screen, "guided_task");
  assert.equal(snap.guidedTask.demo, true);
  assert.equal(snap.guidedTask.done, false);
  // Never blank even before the first step arrives.
  assert.ok(snap.guidedTask.heading.length > 0);
  assert.ok(snap.guidedTask.intro.length > 0);

  await waitUntil(wizard.subscribe, () => wizard.getSnapshot().guidedTask.done);

  snap = wizard.getSnapshot();
  assert.equal(snap.guidedTask.steps.length, 4);
  assert.deepEqual(
    snap.guidedTask.steps.map((s) => s.sentence),
    ['Type "Acme Co" into "Customer"', 'Type "420.00" into "Amount"', 'Type "2026-01-15" into "Date"', 'Click "Save invoice"'],
  );
  for (const step of snap.guidedTask.steps) {
    assert.equal(step.status, "ok");
    assert.ok(!/[{}]/.test(step.sentence), "a step row must never show a raw template or JSON");
  }

  assert.equal(approvalEvents.length, 0, "a zero-grants demo must never trigger an approval prompt");

  wizard.dispose();
});

test("Sign in with ChatGPT and Sign in with Claude both advance straight to the mic check", () => {
  const bus = createMockBusClient();

  const w1 = createWizard(bus);
  w1.continueWelcome();
  w1.chooseChatGPT();
  assert.equal(w1.getSnapshot().screen, "mic_check");
  w1.dispose();

  const w2 = createWizard(createMockBusClient());
  w2.continueWelcome();
  w2.chooseClaude();
  assert.equal(w2.getSnapshot().screen, "mic_check");
  w2.dispose();
});

test("the access key path detects the provider from key shape and gates Continue on having both text and a provider", () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus);
  wizard.continueWelcome();

  let card = wizard.getSnapshot().setupPath.accessKey;
  assert.equal(card.buttonDisabled, true, "nothing pasted yet");

  wizard.setAccessKeyText("sk-ant-abc123");
  card = wizard.getSnapshot().setupPath.accessKey;
  assert.equal(card.detectedLabel, "We recognized this key. It looks like it is from Claude.");
  assert.equal(card.showManualPicker, false);
  assert.equal(card.buttonDisabled, false);

  wizard.setAccessKeyText("some-unrecognized-shape");
  card = wizard.getSnapshot().setupPath.accessKey;
  assert.equal(card.detectedLabel, "We could not tell where this key is from. Pick it from the list below.");
  assert.equal(card.showManualPicker, true);
  assert.equal(card.buttonDisabled, true, "text present but no provider chosen yet");

  wizard.chooseProviderManually("chatgpt");
  card = wizard.getSnapshot().setupPath.accessKey;
  assert.equal(card.buttonDisabled, false);

  wizard.continueWithAccessKey();
  assert.equal(wizard.getSnapshot().screen, "mic_check");

  wizard.dispose();
});

test("the full real setup path reaches completion and publishes a workflow the library can pick up", async () => {
  const bus = createMockBusClient();
  const compiled: BusEvent[] = [];
  bus.subscribe("workflow.compiled", (e) => compiled.push(e));

  const wizard = createWizard(bus, { guidedTaskStepDelayMs: 3 });

  wizard.continueWelcome();
  wizard.setAccessKeyText("sk-ant-abc123");
  wizard.continueWithAccessKey();
  assert.equal(wizard.getSnapshot().screen, "mic_check");

  wizard.playMicSample();
  assert.equal(wizard.getSnapshot().micCheck.played, true);
  assert.ok(wizard.getSnapshot().micCheck.level > 0);
  wizard.continueMicCheck();
  assert.equal(wizard.getSnapshot().screen, "guided_task");
  assert.equal(wizard.getSnapshot().guidedTask.demo, false);

  await waitUntil(wizard.subscribe, () => wizard.getSnapshot().guidedTask.done);
  assert.equal(wizard.getSnapshot().guidedTask.canSave, true);

  wizard.saveAsWorkflow();
  assert.equal(wizard.getSnapshot().screen, "schedule");
  assert.equal(wizard.getSnapshot().guidedTask.saved, true);

  assert.equal(compiled.length, 1);
  assert.equal((compiled[0].payload as unknown as { name: string }).name, "first-task");

  let schedule = wizard.getSnapshot().schedule;
  assert.equal(schedule.canContinue, false);
  wizard.finishSchedule();
  assert.equal(wizard.getSnapshot().complete, false, "must not finish without a chosen schedule");

  wizard.chooseSchedule("daily");
  schedule = wizard.getSnapshot().schedule;
  assert.equal(schedule.canContinue, true);

  wizard.finishSchedule();
  assert.equal(wizard.getSnapshot().complete, true);

  wizard.dispose();
});

test("local model download: not enough disk space blocks the flow with the shortfall, not the model's full size", () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { diskFreeBytes: 1_000_000_000, diskNeededBytes: 4_000_000_000 });
  wizard.continueWelcome();
  wizard.startLocalDownload();

  const local = wizard.getSnapshot().setupPath.local;
  assert.equal(local.phase, "disk_low");
  assert.equal(local.diskLabel, "This computer is short on space. Free up 3 GB and try again.");
  assert.equal(local.compatLabel, null, "compatibility is never checked once disk fails");
  assert.equal(local.buttonDisabled, false, "must be retryable once space frees up");

  wizard.dispose();
});

test("local model download: a compatibility failure blocks the flow and disables Download (hardware cannot change)", () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { vramMb: 1000, vramMinMb: 4000, vramSlowMb: 6000 });
  wizard.continueWelcome();
  wizard.startLocalDownload();

  const local = wizard.getSnapshot().setupPath.local;
  assert.equal(local.phase, "compat_fail");
  assert.equal(local.diskLabel, "You have enough room for this.");
  assert.equal(local.compatLabel, "This computer does not have enough graphics memory for this option. Try signing in with ChatGPT or Claude instead.");
  assert.equal(local.buttonDisabled, true);

  wizard.dispose();
});

test("local model download: a slow-but-workable compatibility result still proceeds to download", async () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { vramMb: 5000, vramMinMb: 4000, vramSlowMb: 6000, download: { totalBytes: 10, ticks: 2, tickMs: 3 } });
  wizard.continueWelcome();
  wizard.startLocalDownload();

  const local = wizard.getSnapshot().setupPath.local;
  assert.equal(local.compatLabel, "This computer can run it, but it may think slowly.");
  assert.equal(local.phase, "downloading", "a slow warning must not block the download");

  wizard.dispose();
});

test("local model download: progress, pause keeps the byte count, resume continues, then Continue moves to the mic check", async () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, {
    diskFreeBytes: 50_000_000_000,
    diskNeededBytes: 4_000_000_000,
    vramMb: 8000,
    download: { totalBytes: 100, ticks: 5, tickMs: 6 },
  });
  wizard.continueWelcome();
  wizard.startLocalDownload();

  let local = wizard.getSnapshot().setupPath.local;
  assert.equal(local.diskLabel, "You have enough room for this.");
  assert.equal(local.compatLabel, "This computer can run it well.");
  assert.equal(local.phase, "downloading");
  assert.equal(local.percent, 0);

  await new Promise((resolve) => setTimeout(resolve, 14));
  local = wizard.getSnapshot().setupPath.local;
  assert.ok(local.percent > 0, "some progress must have arrived");
  assert.ok(local.percent < 100, "must not already be done at this point");

  wizard.pauseLocalDownload();
  local = wizard.getSnapshot().setupPath.local;
  assert.equal(local.phase, "paused");
  const pausedPercent = local.percent;

  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(wizard.getSnapshot().setupPath.local.percent, pausedPercent, "must not progress while paused");

  wizard.resumeLocalDownload();
  await waitUntil(wizard.subscribe, () => wizard.getSnapshot().setupPath.local.phase === "complete");

  local = wizard.getSnapshot().setupPath.local;
  assert.equal(local.percent, 100);
  assert.equal(local.showContinue, true);
  assert.equal(local.progressLabel, "Ready to go.");

  wizard.continueAfterLocalDownload();
  assert.equal(wizard.getSnapshot().screen, "mic_check");

  wizard.dispose();
});

test("local model download: a failed transfer shows the matching what/why/action and can be retried", async () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, {
    download: { totalBytes: 20, ticks: 2, tickMs: 3, failAt: 1, failCode: "CHECKSUM_MISMATCH" },
  });
  wizard.continueWelcome();
  wizard.startLocalDownload();

  await waitUntil(wizard.subscribe, () => wizard.getSnapshot().setupPath.local.phase === "failed");

  const local = wizard.getSnapshot().setupPath.local;
  assert.equal(local.errorWhat, "The downloaded file did not match what we expected.");
  assert.equal(local.errorWhy, "This can happen if the download was interrupted or the connection was not trustworthy.");
  assert.equal(local.errorAction, "Try downloading again.");
  assert.equal(local.buttonLabel, "Try again");
  assert.equal(local.buttonDisabled, false);

  // Retryable: starting again leaves the failed phase (re-runs the checks).
  wizard.startLocalDownload();
  assert.notEqual(wizard.getSnapshot().setupPath.local.phase, "failed");

  wizard.dispose();
});

test("mic check: skipping and continuing both lead to the guided task in real (non-demo) mode", () => {
  const busSkip = createMockBusClient();
  const skipWizard = createWizard(busSkip, { guidedTaskStepDelayMs: 5000 });
  skipWizard.continueWelcome();
  skipWizard.chooseChatGPT();
  skipWizard.skipMicCheck();
  assert.equal(skipWizard.getSnapshot().screen, "guided_task");
  assert.equal(skipWizard.getSnapshot().guidedTask.demo, false);
  skipWizard.dispose();

  const busContinue = createMockBusClient();
  const continueWizard = createWizard(busContinue, { guidedTaskStepDelayMs: 5000 });
  continueWizard.continueWelcome();
  continueWizard.chooseClaude();
  continueWizard.playMicSample();
  continueWizard.continueMicCheck();
  assert.equal(continueWizard.getSnapshot().screen, "guided_task");
  continueWizard.dispose();
});

test("the demo path loops back to setup_path so the user can then configure things for real", async () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { guidedTaskStepDelayMs: 3 });

  wizard.continueWelcome();
  wizard.startDemo();
  await waitUntil(wizard.subscribe, () => wizard.getSnapshot().guidedTask.done);

  assert.equal(wizard.getSnapshot().guidedTask.canContinueDemo, true);
  wizard.continueAfterDemo();
  assert.equal(wizard.getSnapshot().screen, "setup_path");

  // And a real path from here still works end to end.
  wizard.chooseChatGPT();
  assert.equal(wizard.getSnapshot().screen, "mic_check");
  wizard.skipMicCheck();
  await waitUntil(wizard.subscribe, () => wizard.getSnapshot().guidedTask.done);
  assert.equal(wizard.getSnapshot().guidedTask.demo, false);

  wizard.dispose();
});

test("dispose cleans up without throwing, mid-download and mid-run", () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { download: { totalBytes: 1000, ticks: 20, tickMs: 100 } });
  wizard.continueWelcome();
  wizard.startLocalDownload();
  assert.doesNotThrow(() => wizard.dispose());
});
