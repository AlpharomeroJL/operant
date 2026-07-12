// axe-core scan of the Undo screen's preview and done phases, plus a
// keyboard-only pass over Confirm/Cancel, the same X8 accessibility bar
// ui/src/runViewer/accessibility.test.ts and ui/src/dashboard/
// accessibility.test.ts hold every other screen to.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { pressActivate } from "../styles/keyboardSim.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createUndoScreen, type UndoScreen } from "./state.ts";
import { mountUndoScreen } from "./view.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

function mount(env: ReturnType<typeof createDomEnv>, screen: UndoScreen): { container: HTMLElement; render: () => void } {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  function render(): void {
    mountUndoScreen(container, screen.getSnapshot(), {
      onConfirm: () => screen.confirm(),
      onClose: () => screen.close(),
    });
  }
  return { container, render };
}

test("preview phase (mixed restorable and irreversible items): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient());
    const { container, render } = mount(env, screen);
    screen.open("run-1");
    render();
    await assertNoViolations(container, "undo preview");
    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("done phase: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient());
    const { container, render } = mount(env, screen);
    screen.open("run-1");
    screen.confirm();
    render();
    await assertNoViolations(container, "undo done");
    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("empty-journal phase: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient(), { journalForRun: () => [] });
    const { container, render } = mount(env, screen);
    screen.open("run-readonly");
    render();
    await assertNoViolations(container, "undo empty journal");
    screen.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Enter on a focused Confirm button executes the undo", () => {
  const env = createDomEnv();
  try {
    const screen = createUndoScreen(createMockBusClient());
    const { container, render } = mount(env, screen);
    screen.open("run-1");
    render();

    const confirmButton = Array.from(container.querySelectorAll("button")).find((b) => b.className.includes("op-button--primary"));
    assert.ok(confirmButton, "Confirm must be the primary button");
    confirmButton!.focus();
    pressActivate(env.document, confirmButton!, "Enter");

    assert.equal(screen.getSnapshot().phase, "done");
    screen.dispose();
  } finally {
    env.cleanup();
  }
});
