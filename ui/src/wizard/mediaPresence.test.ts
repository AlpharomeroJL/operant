// The media-presence check (C19 bar: "every wizard screen must have visible
// content (and audible where applicable)"; regression guard for the
// silent-wizard failure class). Two halves:
//
// 1. Proves the checker itself actually catches blankness, the same way
//    ui/src/__tests__/microcopy-lint.test.ts proves the microcopy lint
//    catches jargon rather than trusting its own regex blind.
// 2. Drives a real wizard through all five screens (welcome, setup path,
//    mic check, guided task, schedule) and runs the real checker against
//    each screen's real content, so this is a regression test against
//    ui/src/wizard/state.ts, not just against hand-written fixtures.

import { test } from "node:test";
import assert from "node:assert/strict";
import { checkMediaPresence, assertMediaPresence, type ScreenContent } from "./mediaPresence.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createWizard, type Wizard } from "./state.ts";

/**
 * Resolves once `predicate` is true. Checks immediately first: subscribing
 * without checking first would deadlock forever if the condition became
 * true before the subscription was set up (a real run finishes and then
 * goes quiet, so waiting on a fresh emission that will never come hangs).
 */
function waitUntil(wizard: Wizard, predicate: () => boolean): Promise<void> {
  if (predicate()) return Promise.resolve();
  return new Promise((resolve) => {
    const unsubscribe = wizard.subscribe(() => {
      if (predicate()) {
        unsubscribe();
        resolve();
      }
    });
  });
}

test("a screen with real visible text passes", () => {
  const result = checkMediaPresence({ screen: "welcome", visible: ["Welcome", "Body text", "Continue"] });
  assert.equal(result.ok, true);
  assert.deepEqual(result.reasons, []);
});

test("a screen with no visible strings at all fails: the literal blank-screen bug", () => {
  const result = checkMediaPresence({ screen: "broken", visible: [] });
  assert.equal(result.ok, false);
  assert.ok(result.reasons.some((r) => r.includes("no visible content")));
});

test("a screen whose strings are all blank/whitespace fails the same way", () => {
  const result = checkMediaPresence({ screen: "broken", visible: ["", "   ", ""] });
  assert.equal(result.ok, false);
});

test("a screen that declares an audible cue but leaves its label blank fails: the literal silent-wizard bug", () => {
  const result = checkMediaPresence({
    screen: "mic_check",
    visible: ["heading text is fine"],
    audible: { cueLabel: "" },
  });
  assert.equal(result.ok, false);
  assert.ok(result.reasons.some((r) => r.includes("audible")));
});

test("a screen with a real audible cue label passes", () => {
  const result = checkMediaPresence({
    screen: "mic_check",
    visible: ["heading text is fine"],
    audible: { cueLabel: "Play a sample" },
  });
  assert.equal(result.ok, true);
});

test("assertMediaPresence throws naming every failing screen, and passes silently when all screens are fine", () => {
  const good: ScreenContent = { screen: "ok-screen", visible: ["fine"] };
  const bad: ScreenContent = { screen: "bad-screen", visible: [] };

  assert.doesNotThrow(() => assertMediaPresence([good]));
  assert.throws(() => assertMediaPresence([good, bad]), /bad-screen/);
});

test("every real wizard screen has visible content, and the mic check has an audible cue", async () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { guidedTaskStepDelayMs: 2 });
  const seen: ScreenContent[] = [];

  function capture(): void {
    seen.push(wizard.getSnapshot().mediaContent);
  }

  // Screen 1: welcome.
  capture();

  // Screen 2: setup path.
  wizard.continueWelcome();
  capture();

  // Screen 3: mic check (reached via the access-key path here so that path
  // gets exercised too; the demo-link path is covered by the "reaches a
  // working demo" test in state.test.ts).
  wizard.setAccessKeyText("sk-ant-test-key");
  wizard.continueWithAccessKey();
  capture();

  // Screen 4: guided task, both immediately (heading/intro must not be
  // blank before the first step arrives) and once steps have streamed in.
  wizard.continueMicCheck();
  capture();
  await new Promise((resolve) => setTimeout(resolve, 40));
  capture();

  // Screen 5: schedule, reached once the guided task finishes (already true
  // by now given the wait above, but check rather than assume).
  await waitUntil(wizard, () => wizard.getSnapshot().guidedTask.done);
  wizard.saveAsWorkflow();
  capture();

  assert.equal(seen.length, 6);
  const screensSeen = new Set(seen.map((c) => c.screen));
  assert.deepEqual(screensSeen, new Set(["welcome", "setup_path", "mic_check", "guided_task", "schedule"]));

  assertMediaPresence(seen);

  const micCheckContent = seen.find((c) => c.screen === "mic_check");
  assert.ok(micCheckContent?.audible, "mic check must declare an audible cue");
  assert.ok(micCheckContent.audible.cueLabel.length > 0);

  wizard.dispose();
});

test("the demo path's guided-task screen also has visible content from the very first frame", () => {
  const bus = createMockBusClient();
  const wizard = createWizard(bus, { guidedTaskStepDelayMs: 5 });

  wizard.continueWelcome();
  wizard.startDemo();

  const content = wizard.getSnapshot().mediaContent;
  assert.equal(content.screen, "guided_task");
  assertMediaPresence([content]);

  wizard.dispose();
});
