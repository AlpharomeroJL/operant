// DOM assertions for ./view.ts. jsdom via ui/src/styles/testDomEnv.ts, the
// same harness ui/src/runViewer/view.test.ts and ui/src/dashboard's own view
// tests use.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createUndoScreen, type UndoScreen } from "./state.ts";
import { mountUndoScreen } from "./view.ts";
import { undoScreenStrings } from "./strings.ts";
import type { UndoJournalEntry } from "./mockJournal.ts";

function mount(env: ReturnType<typeof createDomEnv>, screen: UndoScreen) {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  const calls = { confirm: 0, close: 0 };
  function render(): HTMLElement {
    return mountUndoScreen(container, screen.getSnapshot(), {
      onConfirm: () => {
        calls.confirm++;
        screen.confirm();
      },
      onClose: () => {
        calls.close++;
        screen.close();
      },
    });
  }
  return { container, render, calls };
}

test("closed: mounts nothing", () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient());
    const { container, render } = mount(env, screen);
    render();
    assert.equal(container.children.length, 0);
    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("preview: heading, a checkmark per restorable item, no mark and a grayed class on the irreversible one, Confirm and Cancel both present", () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient());
    const { container, render } = mount(env, screen);
    screen.open("run-1");
    render();

    const heading = container.querySelector<HTMLElement>("#op-undo-heading");
    assert.ok(heading);
    assert.equal(heading!.textContent, undoScreenStrings.title);
    const dialog = container.querySelector('[role="alertdialog"]');
    assert.ok(dialog);
    assert.equal(dialog!.getAttribute("aria-labelledby"), "op-undo-heading");

    const items = Array.from(container.querySelectorAll(".op-undo-item"));
    assert.equal(items.length, 6);

    const irreversibleItems = container.querySelectorAll(".op-undo-item--irreversible");
    assert.equal(irreversibleItems.length, 1);
    const irreversibleCheck = irreversibleItems[0].querySelector(".op-undo-item__check");
    assert.equal(irreversibleCheck?.textContent, "", "an irreversible row must carry no checkmark");
    assert.ok(irreversibleItems[0].textContent?.includes("Cannot be undone"));

    const restorableChecks = Array.from(container.querySelectorAll(".op-undo-item:not(.op-undo-item--irreversible) .op-undo-item__check"));
    assert.equal(restorableChecks.length, 5);
    for (const check of restorableChecks) {
      assert.equal(check.textContent, "✓");
      assert.equal(check.getAttribute("aria-hidden"), "true");
    }

    const buttons = Array.from(container.querySelectorAll("button")).map((b) => b.textContent);
    assert.ok(buttons.includes(undoScreenStrings.confirm));
    assert.ok(buttons.includes(undoScreenStrings.cancel));
    assert.ok(!buttons.includes(undoScreenStrings.close));

    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("preview: Confirm executes the undo and Cancel dismisses without executing", () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient());
    const { container, render, calls } = mount(env, screen);
    screen.open("run-1");
    render();

    const confirmButton = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === undoScreenStrings.confirm);
    assert.ok(confirmButton);
    confirmButton!.click();
    assert.equal(calls.confirm, 1);
    assert.equal(screen.getSnapshot().phase, "done");

    render();
    const closeButton = Array.from(container.querySelectorAll("button")).find((b) => b.textContent === undoScreenStrings.close);
    assert.ok(closeButton, "the done phase must show Close, not Cancel/Confirm");
    closeButton!.click();
    assert.equal(calls.close, 1);
    assert.equal(screen.getSnapshot().phase, "closed");

    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("done: reuses the run viewer's own status-dot tokens, green for restored, idle-gray for the irreversible entry, plus a restored-count summary", () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient());
    const { container, render } = mount(env, screen);
    screen.open("run-1");
    screen.confirm();
    render();

    const summary = container.querySelector(".op-undo-screen__summary");
    assert.equal(summary?.textContent, undoScreenStrings.doneSummary(5));

    const dots = Array.from(container.querySelectorAll(".op-status__dot"));
    assert.equal(dots.length, 6);
    const doneDots = dots.filter((d) => (d as HTMLElement).dataset.state === "done");
    const pendingDots = dots.filter((d) => (d as HTMLElement).dataset.state === "pending");
    assert.equal(doneDots.length, 5, "every restorable entry must show as done");
    assert.equal(pendingDots.length, 1, "the irreversible entry stays a quiet, idle-colored dot, never done");

    // No leftover checkmark glyphs from the preview phase.
    assert.equal(container.querySelectorAll(".op-undo-item__check").length, 0);

    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("an empty journal shows the empty label and only a dismiss action, never Confirm", () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient(), { journalForRun: () => [] });
    const { container, render } = mount(env, screen);
    screen.open("run-readonly");
    render();

    assert.equal(container.querySelector(".op-empty")?.textContent, undoScreenStrings.empty);
    assert.equal(container.querySelectorAll(".op-undo-item").length, 0);
    const buttons = Array.from(container.querySelectorAll("button")).map((b) => b.textContent);
    assert.ok(!buttons.includes(undoScreenStrings.confirm));
    assert.ok(buttons.includes(undoScreenStrings.cancel));

    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("an all-irreversible run: nothing executable, every row grayed, Confirm still lets the user acknowledge (narrate) but restores nothing", () => {
  const env = createDomEnv();
  try {
    const onlyIrreversible: UndoJournalEntry[] = [
      { seq: 2, inverse: { op: "irreversible", description: "posted the message to #general" } },
      { seq: 1, inverse: { op: "irreversible", description: "sent the text message" } },
    ];
    const screen = createUndoScreen(createMockBusClient(), { journalForRun: () => onlyIrreversible });
    const { container, render } = mount(env, screen);
    screen.open("run-comms");
    render();

    assert.equal(container.querySelectorAll(".op-undo-item").length, 2);
    assert.equal(container.querySelectorAll(".op-undo-item--irreversible").length, 2);
    assert.equal(container.querySelectorAll(".op-undo-item__check").length, 2);
    for (const check of container.querySelectorAll(".op-undo-item__check")) {
      assert.equal(check.textContent, "");
    }

    screen.dispose();
  } finally {
    env.cleanup();
  }
});
