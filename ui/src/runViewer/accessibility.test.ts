// X8 app-accessibility bar for ./view.ts (new in this lane; see its header
// comment for why): an axe-core scan of idle, running, paused, and halted
// states, plus the keyboard-specific behavior axe cannot check by static
// analysis (the intervene field carrying focus and its own text across a
// rebuild, the same class of bug this lane fixed on the wizard's access-key
// field, ui/src/styles/focusPreserve.ts).

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { pressActivate, typeText } from "../styles/keyboardSim.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { submitGoal } from "../palette/palette.ts";
import { createRunViewer, type RunViewer } from "./state.ts";
import { mountRunViewer } from "./view.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

function mount(env: ReturnType<typeof createDomEnv>, viewer: RunViewer): { container: HTMLElement; render: () => void } {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  function render(): void {
    mountRunViewer(container, viewer.getSnapshot(), {
      onStop: () => viewer.stop(),
      onTogglePause: () => viewer.togglePause(),
      onIntervene: (text) => viewer.intervene(text),
    });
  }
  return { container, render };
}

test("idle run viewer: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const viewer = createRunViewer(createMockBusClient());
    const { container } = mount(env, viewer);
    await assertNoViolations(container, "idle run viewer");
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("running, with streamed-in steps: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
    await new Promise((resolve) => setTimeout(resolve, 40));
    render();
    assert.ok(viewer.getSnapshot().steps.length > 0, "the run must have streamed at least one step");
    await assertNoViolations(container, "running run viewer with steps");
    stop?.();
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("paused, with the intervene field showing: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
    viewer.togglePause();
    render();
    assert.equal(viewer.getSnapshot().showIntervene, true);
    await assertNoViolations(container, "paused run viewer with intervene field");
    stop?.();
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("halted: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
    viewer.stop();
    render();
    assert.equal(viewer.getSnapshot().runState, "halted");
    await assertNoViolations(container, "halted run viewer");
    stop?.();
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("a failed safety check card in the list: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
    const runId = viewer.getSnapshot().runId;
    assert.ok(runId, "the run must have started");
    // Fail a safety check on the first step so the inline card renders, then
    // scan the filmstrip + gate-card DOM the same way the other states are.
    bus.publish("run.step.gated", { run_id: runId, step_id: "s1", gate_kind: "safety", result: "fail" });
    render();
    assert.ok(container.querySelector(".op-safety-card"), "the failed-check card must be on screen");
    await assertNoViolations(container, "run viewer with a failed safety check card");
    stop?.();
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("focus and typed text survive a rebuild: typing an intervene instruction does not lose focus on every keystroke", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
    viewer.togglePause();
    render();

    const input = container.querySelector<HTMLInputElement>('[data-op-focus-key="run-intervene-input"]');
    assert.ok(input, "the intervene input must be on screen while paused");
    input.focus();
    typeText(env.document, input, "click the other invoice");
    render();

    const inputAfter = container.querySelector<HTMLInputElement>('[data-op-focus-key="run-intervene-input"]');
    assert.notEqual(inputAfter, input, "the rebuild must have replaced the DOM node");
    assert.equal(env.document.activeElement, inputAfter, "focus must carry across the rebuild");
    assert.equal(inputAfter?.value, "click the other invoice");
    stop?.();
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Enter on a focused Stop button ends the run", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    const stop = submitGoal(bus, "Copy the invoice total", { stepDelayMs: 2 });
    render();
    assert.equal(viewer.getSnapshot().runState, "running");

    const stopButton = container.querySelector<HTMLButtonElement>('[data-op-focus-key="run-stop"]');
    assert.ok(stopButton);
    stopButton.focus();
    pressActivate(env.document, stopButton, "Enter");

    assert.equal(viewer.getSnapshot().runState, "halted");
    stop?.();
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});
