// Flight recorder view tests (docs/specs/design.md section 3): the DOM
// behavior mountRunViewer adds on top of the streaming step list -- the mode
// chips, the auto-following filmstrip of redacted thumbnails, two-way scrub
// sync between a frame and its row, and a failed safety check as an inline
// card rather than a modal. Uses a real jsdom document (../styles/testDomEnv)
// so clicks dispatch and the rebuild-on-select actually runs, the same harness
// ./accessibility.test.ts uses for its axe scans.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY } from "../bus/types.ts";
import { createRunViewer, type RunViewer } from "./state.ts";
import { mountRunViewer } from "./view.ts";

function mount(env: ReturnType<typeof createDomEnv>, viewer: RunViewer): { container: HTMLElement; render: () => void } {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  function render(): void {
    mountRunViewer(container, viewer.getSnapshot(), {
      onStop: () => viewer.stop(),
      onTogglePause: () => viewer.togglePause(),
      onIntervene: (text) => viewer.intervene(text),
      onSelectStep: (id) => viewer.select(id),
    });
  }
  return { container, render };
}

function proposeStep(bus: ReturnType<typeof createMockBusClient>, runId: string, id: string): void {
  bus.publish("run.step.proposed", { run_id: runId, step: { v: 1, id, kind: "wait" } });
}

test("a teach run shows the amber REC chip with its AI tooltip", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    bus.publish("run.started", { run_id: "r1", goal: "teach it", mode: RUN_MODE_EXPLORE });
    render();

    const chip = container.querySelector(".op-chip--rec");
    assert.ok(chip, "the REC chip must show while teaching");
    assert.match(chip.textContent ?? "", /REC/);
    assert.equal(chip.getAttribute("title"), "Operant is using your AI engine to learn this");
    assert.equal(container.querySelector(".op-chip--exact"), null, "the saved-workflow chip must not show for a teach run");
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("a saved-workflow run shows the quiet gray no-AI chip with the exact design copy", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    bus.publish("run.started", { run_id: "r1", goal: "run it", mode: RUN_MODE_REPLAY });
    render();

    const chip = container.querySelector(".op-chip--exact");
    assert.ok(chip, "the saved-workflow chip must show while running a saved workflow");
    // The one place the design fixes verbatim (docs/specs/design.md section 3).
    assert.equal(chip.textContent, "no AI, exact replay", "exact design.md section 3 copy");
    assert.equal(container.querySelector(".op-chip--rec"), null, "the REC chip must not show for a saved-workflow run");
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("a live run appends one filmstrip frame per step and auto-follows the latest", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    proposeStep(bus, "r1", "s1");
    proposeStep(bus, "r1", "s2");
    render();

    assert.equal(container.querySelectorAll(".op-filmstrip__frame").length, 2, "one frame per step");
    const selected = container.querySelectorAll('.op-filmstrip__frame[data-selected="true"]');
    assert.equal(selected.length, 1, "exactly one frame is followed");
    assert.equal(selected[0].getAttribute("data-step-id"), "s2", "the newest frame is the followed one");

    proposeStep(bus, "r1", "s3");
    render();
    assert.equal(container.querySelectorAll(".op-filmstrip__frame").length, 3);
    assert.equal(
      container.querySelector('.op-filmstrip__frame[data-selected="true"]')?.getAttribute("data-step-id"),
      "s3",
      "the strip auto-followed the newest frame",
    );
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("filmstrip thumbnails are redacted placeholders: no image element, only generated bars", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    proposeStep(bus, "r1", "s1");
    render();

    const frame = container.querySelector(".op-filmstrip__frame");
    assert.ok(frame);
    assert.equal(frame.querySelector("img"), null, "no captured screenshot image ships");
    assert.equal(frame.querySelector("canvas"), null, "no captured pixels ship");
    const thumb = frame.querySelector(".op-filmstrip__thumb");
    assert.equal(thumb?.getAttribute("aria-hidden"), "true", "the placeholder is decorative");
    assert.ok(frame.querySelectorAll(".op-filmstrip__bar").length > 0, "the placeholder is drawn as redaction bars");
    assert.ok(
      [...frame.querySelectorAll(".op-visually-hidden")].some((n) => n.textContent === "Redacted preview"),
      "a screen-reader note flags the thumbnail as redacted",
    );
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("scrub sync: clicking a filmstrip frame highlights the matching step row (and pins the strip)", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    proposeStep(bus, "r1", "s1");
    proposeStep(bus, "r1", "s2");
    render();

    // Click the FIRST frame, not the auto-followed latest (s2).
    const frame1 = container.querySelector<HTMLButtonElement>('.op-filmstrip__frame[data-step-id="s1"]');
    assert.ok(frame1);
    frame1.click();

    // The matching row is now highlighted, so is the frame, and the previously
    // followed latest no longer is.
    assert.equal(
      container.querySelector('.op-step-row[data-step-id="s1"]')?.getAttribute("data-selected"),
      "true",
      "selecting a frame highlights the matching row",
    );
    assert.equal(container.querySelector('.op-filmstrip__frame[data-step-id="s1"]')?.getAttribute("data-selected"), "true");
    assert.equal(container.querySelector('.op-step-row[data-step-id="s2"]')?.getAttribute("data-selected"), null);
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("scrub sync the other way: clicking a step row highlights the matching filmstrip frame", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    proposeStep(bus, "r1", "s1");
    proposeStep(bus, "r1", "s2");
    render();

    const row1 = container.querySelector<HTMLButtonElement>('.op-step-row[data-step-id="s1"]');
    assert.ok(row1);
    row1.click();

    assert.equal(
      container.querySelector('.op-filmstrip__frame[data-step-id="s1"]')?.getAttribute("data-selected"),
      "true",
      "selecting a row highlights the matching frame",
    );
    assert.equal(container.querySelector('.op-step-row[data-step-id="s1"]')?.getAttribute("data-selected"), "true");
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});

test("a failed safety check renders as an inline card in the step list, not a modal", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const viewer = createRunViewer(bus);
    const { container, render } = mount(env, viewer);
    viewer.subscribe(render);

    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    proposeStep(bus, "r1", "s1");
    // A failed safety check alone draws the card; the run then halts on its own.
    bus.publish("run.step.gated", { run_id: "r1", step_id: "s1", gate_kind: "safety", result: "fail", expr: "x < 1" });
    render();

    const card = container.querySelector(".op-safety-card");
    assert.ok(card, "the failed check must render a card");

    // In the list, attached to its own step's list item ...
    assert.ok(card.closest("li.op-step-item"), "the card sits inside the step's list item");
    assert.ok(container.querySelector("ol.op-step-list")?.contains(card), "the card is inside the step list");

    // ... and never a modal.
    assert.equal(container.querySelector(".op-modal-backdrop"), null, "no modal backdrop is used");
    assert.equal(card.closest(".op-modal-backdrop"), null);
    assert.equal(card.getAttribute("role"), "note");
    assert.match(card.textContent ?? "", /safety check/i);
    viewer.dispose();
  } finally {
    env.cleanup();
  }
});
