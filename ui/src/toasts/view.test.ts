// DOM assertions for ./view.ts. jsdom via ui/src/styles/testDomEnv.ts, the
// same harness ui/src/undo/view.test.ts and ui/src/tray/view.test.ts use.
//
// The no-action case is exercised with a hand-built fixture snapshot
// (mountToast takes plain data, not a bus) rather than through
// createToasts, since every real trigger today (run.completed) always
// attaches an action: this proves the amber-only-when-actionable half of
// design.md section 3's Toasts that the live app cannot demonstrate yet.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createToasts, type ToastSnapshot } from "./state.ts";
import { mountToast } from "./view.ts";

test("no toast: mounts nothing", () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    mountToast(container, { toast: null });
    assert.equal(container.children.length, 0);
  } finally {
    env.cleanup();
  }
});

test("an actionable toast (run.completed): verb-first message, bottom-right toast role, one amber action button", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const toasts = createToasts(bus);
    bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 14, wall_ms: 100 });

    const container = env.document.createElement("div");
    let actedOnRunId: string | null = null;
    mountToast(container, toasts.getSnapshot(), { onAction: (runId) => (actedOnRunId = runId) });

    const toastEl = container.querySelector(".op-toast");
    assert.ok(toastEl, "the toast element must be present");
    assert.equal(toastEl!.getAttribute("role"), "status");

    const message = container.querySelector(".op-toast__message");
    assert.equal(message?.textContent, "Run complete, 14 steps");
    // Verb-first (design.md section 3): the sentence's very first word is a
    // verb naming what happened, not a noun phrase burying it later.
    assert.match(message!.textContent!, /^(Run|Saved|Restored)\b/);

    const action = container.querySelector<HTMLButtonElement>(".op-toast__action");
    assert.ok(action, "an actionable toast must render its action button");
    assert.equal(action!.textContent, "Undo this run");

    action!.click();
    assert.equal(actedOnRunId, "r1");

    toasts.dispose();
  } finally {
    env.cleanup();
  }
});

test("a message-only toast renders no action element at all, so nothing is left to paint amber", () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    const fixture: ToastSnapshot = { toast: { id: "t1", message: "Saved as workflow" } };
    mountToast(container, fixture);

    const message = container.querySelector(".op-toast__message");
    assert.equal(message?.textContent, "Saved as workflow");

    assert.equal(container.querySelector(".op-toast__action"), null, "amber is only for an invited action (design.md section 3)");
    assert.equal(container.querySelectorAll("button").length, 0);
  } finally {
    env.cleanup();
  }
});

test("a toast with an action label but no runId is treated as non-actionable: defensive, should never happen from state.ts", () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    const fixture: ToastSnapshot = { toast: { id: "t1", message: "Run complete, 1 step", action: { label: "Undo this run" } } };
    mountToast(container, fixture);

    assert.equal(container.querySelector(".op-toast__action"), null);
  } finally {
    env.cleanup();
  }
});

test("re-mounting with no toast clears a previously rendered one", () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    mountToast(container, { toast: { id: "t1", message: "Saved as workflow" } });
    assert.ok(container.querySelector(".op-toast"));

    mountToast(container, { toast: null });
    assert.equal(container.children.length, 0);
  } finally {
    env.cleanup();
  }
});
