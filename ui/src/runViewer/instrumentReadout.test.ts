// @advanced
// Exempt from scripts/microcopy_lint.mjs (a test file, not shipped UI copy;
// same exemption ./view.test.ts takes): it names wire vocabulary
// ("explore"/"replay") and the run.completed telemetry field.
//
// GLASS.md GL5 bar (the instrument readout, done honestly): the displayed
// MODEL CALLS value must PROVABLY come from a measured counter on the
// run.completed event, not a constant. CLAIMS.md's "replay makes zero model
// calls" is now shown in-app, so it is backed here by two fixture events, one
// carrying model_calls=3 (an explore run) and one carrying model_calls=0 (a
// replay run): the readout shows 3 and 0 respectively, which a hardcoded zero
// could never do. A run with no measured count shows "-", never a fabricated 0.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient, type BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY, type RunMode, type RunCompletedPayload } from "../bus/types.ts";
import { createRunViewer, type RunViewer } from "./state.ts";
import { mountRunViewer } from "./view.ts";

function mount(env: ReturnType<typeof createDomEnv>, viewer: RunViewer): { container: HTMLElement; render: () => void } {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  const render = (): void => void mountRunViewer(container, viewer.getSnapshot());
  return { container, render };
}

/**
 * Complete a run with a measured model-call count. model_calls rides
 * run.completed as an append-only telemetry field the core emits (GLASS.md
 * section 5); the TS wire mirror (ui/src/bus/types.ts) has not caught up to it,
 * so this fixture attaches it through an `as` cast, exactly the shape a real
 * core sends and ui/src/runViewer/state.ts reads back off the payload.
 */
function completeRun(bus: BusClient, runId: string, modelCalls: number): void {
  bus.publish("run.completed", {
    run_id: runId,
    outcome: "ok",
    steps: 1,
    wall_ms: 10,
    model_calls: modelCalls,
  } as RunCompletedPayload);
}

function modelCallsText(container: HTMLElement): string | null | undefined {
  return container.querySelector('[data-metric="model-calls"] .op-run-viewer__readout-value')?.textContent;
}

function runAndComplete(env: ReturnType<typeof createDomEnv>, mode: RunMode, modelCalls: number): HTMLElement {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const { container, render } = mount(env, viewer);
  viewer.subscribe(render);
  bus.publish("run.started", { run_id: "r1", goal: "g", mode });
  completeRun(bus, "r1", modelCalls);
  render();
  viewer.dispose();
  return container;
}

test("an explore run that made 3 model calls displays MODEL CALLS 3 (read from the event, not a constant)", () => {
  const env = createDomEnv();
  try {
    const container = runAndComplete(env, RUN_MODE_EXPLORE, 3);
    assert.equal(modelCallsText(container), "3", "the displayed count is the measured 3");
    assert.match(container.textContent ?? "", /MODEL CALLS/, "the readout is labeled");
  } finally {
    env.cleanup();
  }
});

test("a replay run that made 0 model calls displays MODEL CALLS 0 (the honest zero, proven measured by the 3-vs-0 pair)", () => {
  const env = createDomEnv();
  try {
    const container = runAndComplete(env, RUN_MODE_REPLAY, 0);
    assert.equal(modelCallsText(container), "0", "replay's measured zero shows as 0");
  } finally {
    env.cleanup();
  }
});

test("the readout tracks the event field: the SAME code path shows whatever the counter measured", () => {
  const env = createDomEnv();
  try {
    // A distinctive value no constant in the source would coincidentally equal.
    assert.equal(modelCallsText(runAndComplete(env, RUN_MODE_EXPLORE, 7)), "7");
    assert.equal(modelCallsText(runAndComplete(env, RUN_MODE_EXPLORE, 42)), "42");
  } finally {
    env.cleanup();
  }
});

test("before a run completes, MODEL CALLS shows '-' (unavailable), never a fabricated 0", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);
    // A run is under way but has not completed: no measured count exists yet.
    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    render();
    assert.equal(modelCallsText(container), "-", "an unmeasured count is '-', not 0");
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("NETWORK shows '-' while network_bytes is not on the event (unavailable, never a fake 0 KB)", () => {
  const env = createDomEnv();
  try {
    const container = runAndComplete(env, RUN_MODE_EXPLORE, 3);
    const network = container.querySelector('[data-metric="network"] .op-run-viewer__readout-value');
    assert.equal(network?.textContent, "-", "network is honestly unavailable, not a fabricated 0 KB");
  } finally {
    env.cleanup();
  }
});

test("no readout before any run has started (nothing to instrument yet)", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    render();
    assert.equal(container.querySelector(".op-run-viewer__readout"), null, "idle run viewer shows no instruments");
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});
