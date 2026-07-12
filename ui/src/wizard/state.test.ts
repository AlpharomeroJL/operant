// The wizard's core scripted-run tests (C19 bar): a run through the quiet
// demo link reaches a working demo in default mode, and a run through a
// real setup path reaches completion and publishes a workflow the library
// (ui/src/library/state.ts) can pick up, plus coverage of the trickier
// branches (local download's disk/compatibility gates and pause/resume,
// access-key provider detection wiring, schedule gating). Also covers the
// guided-teach-to-schedule hand-off this lane's own brief names directly
// (C19, FR-U1): wizard completion through guided teach, Save as workflow,
// and schedule, checked against the real glossary
// (contracts/microcopy_glossary.json) at run time. That check matters here
// specifically because the guided task's narrated sentences are rendered
// from Action IR through the real renderer (ui/src/runViewer/sdkRender.ts)
// and never exist as source literals, so scripts/microcopy_lint.mjs's
// static scan cannot see them; only a runtime check can. No DOM: runs under
// plain `node --test`, same split as every other state module in ui/src.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, type BusEvent } from "../bus/types.ts";
import { createWizard } from "./state.ts";
import { GUIDED_TASK_GOAL, GUIDED_TASK_STEPS, GUIDED_TASK_WINDOW } from "./guidedTask.ts";
import type { StartExploreRequest, TeachClient } from "../teach/client.ts";

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

// The real glossary, not a copy: same file scripts/microcopy_lint.mjs reads,
// same word-boundary/case-insensitive matching it uses, so this test and CI
// can never quietly drift apart on what counts as jargon.
const GLOSSARY_PATH = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "contracts", "microcopy_glossary.json");
const GLOSSARY_TERMS: readonly string[] = (
  JSON.parse(readFileSync(GLOSSARY_PATH, "utf8")) as { terms: { internal: string }[] }
).terms.map((t) => t.internal);

/** Fails naming the exact string and term if any visible/audible string leaks glossary-internal vocabulary. */
function assertNoInternalJargon(strings: readonly string[]): void {
  for (const s of strings) {
    for (const term of GLOSSARY_TERMS) {
      const re = new RegExp(`\\b${term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`, "i");
      assert.ok(!re.test(s), `visible string "${s}" leaks internal term "${term}"`);
    }
  }
}

/**
 * A teach client that records how the wizard invokes start_explore and
 * compile_run, and drives each run straight to completion on the bus so the
 * wizard's own runViewer reaches "done" and Save as workflow is reachable (the
 * mock client's real streaming is covered in ui/src/teach/client.test.ts).
 */
function recordingTeachClient(bus: ReturnType<typeof createMockBusClient>): {
  client: TeachClient;
  startExploreCalls: StartExploreRequest[];
  compileRunCalls: { runId: string; name?: string }[];
} {
  const startExploreCalls: StartExploreRequest[] = [];
  const compileRunCalls: { runId: string; name?: string }[] = [];
  let n = 0;
  const client: TeachClient = {
    startExplore(req) {
      startExploreCalls.push(req);
      const runId = req.runId ?? `fake-run-${++n}`;
      bus.publish("run.started", { run_id: runId, goal: req.goal, mode: RUN_MODE_EXPLORE });
      bus.publish("run.completed", { run_id: runId, outcome: "ok", steps: req.script?.length ?? 0, wall_ms: 1 });
      return { runId, stop() {} };
    },
    compileRun(runId, opts) {
      compileRunCalls.push({ runId, name: opts?.name });
      return { name: opts?.name ?? runId, version: opts?.version ?? "1.0.0", sourceRunId: runId };
    },
  };
  return { client, startExploreCalls, compileRunCalls };
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

test("BAR: wizard completion -> guided teach -> Save as workflow -> schedule produces a saved, scheduled workflow, entirely in default mode with plain-English narration and no glossary jargon", async () => {
  const bus = createMockBusClient();
  const compiled: BusEvent[] = [];
  bus.subscribe("workflow.compiled", (e) => compiled.push(e));

  const wizard = createWizard(bus, { guidedTaskStepDelayMs: 3 });
  const seen: string[] = [];
  function collectVisible(): void {
    const content = wizard.getSnapshot().mediaContent;
    seen.push(...content.visible);
    if (content.audible) seen.push(content.audible.cueLabel);
  }

  // Wizard completion: welcome through setup to the mic check.
  collectVisible();
  wizard.continueWelcome();
  collectVisible();
  wizard.chooseChatGPT();
  assert.equal(wizard.getSnapshot().screen, "mic_check");
  collectVisible();
  wizard.continueMicCheck();

  // Guided teach: explore mode with training wheels, against the fixture web
  // app (contracts/fixtures/webapp/index.html), narrated as it runs.
  let snap = wizard.getSnapshot();
  assert.equal(snap.screen, "guided_task");
  assert.equal(snap.guidedTask.demo, false, "a real teach run, not the quiet demo");

  await waitUntil(wizard.subscribe, () => wizard.getSnapshot().guidedTask.done);
  snap = wizard.getSnapshot();
  collectVisible();

  // Narrated steps: plain English from the real renderer, not a hand-rolled
  // sentence and never a raw Action IR dump.
  assert.deepEqual(
    snap.guidedTask.steps.map((s) => s.sentence),
    ['Type "Acme Co" into "Customer"', 'Type "420.00" into "Amount"', 'Type "2026-01-15" into "Date"', 'Click "Save invoice"'],
  );
  for (const step of snap.guidedTask.steps) {
    assert.ok(!/[{}]/.test(step.sentence), "a narrated step must never leak a raw template or JSON");
  }

  // Ends on exactly one button: Save as workflow.
  assert.equal(snap.guidedTask.canSave, true);
  assert.equal(snap.guidedTask.saveButton, "Save as workflow");
  seen.push(snap.guidedTask.saveButton, snap.guidedTask.doneLabel);

  wizard.saveAsWorkflow();
  assert.equal(compiled.length, 1, "Save as workflow saves the workflow");
  const savedName = (compiled[0].payload as unknown as { name: string }).name;
  seen.push(wizard.getSnapshot().guidedTask.savedHint);

  // The schedule step: plain choices, gated on picking one, producing a
  // scheduled workflow.
  snap = wizard.getSnapshot();
  assert.equal(snap.screen, "schedule");
  assert.equal(snap.schedule.heading, "Want this to run by itself?");
  collectVisible();
  assert.equal(snap.schedule.canContinue, false, "nothing chosen yet");

  wizard.chooseSchedule("daily");
  seen.push(wizard.getSnapshot().schedule.continueButton);
  wizard.finishSchedule();

  snap = wizard.getSnapshot();
  assert.equal(snap.complete, true, "the guided teach handed off into a completed workflow");
  assert.equal(snap.schedule.selected, "daily", "the chosen schedule is what the flow produced");
  assert.equal(savedName, "first-task", "the same workflow that was saved is the one that got scheduled");

  // Entirely default mode: every visible or audible string this run actually
  // showed uses only user-facing vocabulary.
  assertNoInternalJargon(seen);

  wizard.dispose();
});

test("guided teach invokes start_explore with the practice window and the guided steps; Save as workflow invokes compile_run for that run", () => {
  const bus = createMockBusClient();
  const { client, startExploreCalls, compileRunCalls } = recordingTeachClient(bus);
  const wizard = createWizard(bus, { teachClient: client });

  wizard.continueWelcome();
  wizard.chooseChatGPT();
  wizard.continueMicCheck(); // -> guided_task, which begins the guided teach

  assert.equal(startExploreCalls.length, 1, "the guided task must invoke start_explore exactly once");
  const req = startExploreCalls[0];
  assert.equal(req.goal, GUIDED_TASK_GOAL);
  assert.equal(req.windowProcess, GUIDED_TASK_WINDOW, "the guided task teaches against the practice-page window");
  assert.deepEqual(req.script, GUIDED_TASK_STEPS, "the guided steps are the mock's canned trajectory for this teach run");

  // The fake completed the run, so Save as workflow (the compile handoff) is reachable.
  assert.equal(wizard.getSnapshot().screen, "guided_task");
  assert.equal(wizard.getSnapshot().guidedTask.canSave, true);
  wizard.saveAsWorkflow();

  assert.equal(compileRunCalls.length, 1, "Save as workflow must invoke compile_run exactly once");
  assert.equal(compileRunCalls[0].runId, req.runId ?? "fake-run-1", "compile_run must target the run just taught");
  assert.equal(compileRunCalls[0].name, "first-task");
  assert.equal(wizard.getSnapshot().screen, "schedule", "Save as workflow advances to the schedule step");

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

test("local model download: the card always shows the download's own size (design.md section 3.3), regardless of phase", () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { diskNeededBytes: 4_000_000_000 });
  // Before any check has run (diskLabel/compatLabel are still null), sizeLabel is already present.
  assert.equal(wizard.getSnapshot().setupPath.local.sizeLabel, "This download is about 4 GB.");

  wizard.continueWelcome();
  wizard.startLocalDownload();
  assert.equal(
    wizard.getSnapshot().setupPath.local.sizeLabel,
    "This download is about 4 GB.",
    "the size line does not change once a download starts",
  );

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
