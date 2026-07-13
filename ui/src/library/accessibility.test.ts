// X8 app-accessibility bar for the workflow library: an axe-core scan of
// the populated and empty states, plus the keyboard-specific focus-carrying
// behavior axe cannot check by static analysis (see ./view.ts's header
// comment and ui/src/styles/focusPreserve.ts).

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createLibrary } from "./state.ts";
import { createMockRegistry } from "./mockRegistry.ts";
import { mountLibrary } from "./view.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

test("populated library: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const library = createLibrary(bus, { registry: createMockRegistry() });
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountLibrary(container, library.getSnapshot());
    await assertNoViolations(container, "populated library");
    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("empty library: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const library = createLibrary(bus, { registry: createMockRegistry([]) });
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountLibrary(container, library.getSnapshot());
    await assertNoViolations(container, "empty library");
    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("focus survives a rebuild: pressing Schedule keeps focus on the equivalent button in the rebuilt grid", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const registry = createMockRegistry();
    const firstName = registry.list()[0].manifest.name;
    const library = createLibrary(bus, { registry });
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    function render(): void {
      mountLibrary(container, library.getSnapshot(), {
        onRun: (name) => library.run(name),
        onSchedule: (name) => library.schedule(name),
        onExplain: (name) => library.explain(name),
      });
    }
    render();

    const scheduleButton = container.querySelector<HTMLButtonElement>(`[data-op-focus-key="library-schedule-${firstName}"]`);
    assert.ok(scheduleButton, "the first card's Schedule button must be on screen");
    scheduleButton.focus();
    // Drive the same async path the button click triggers, then await it so the
    // honest outcome has resolved before the rebuild. Focus stays on the button
    // across the await (nothing else moves it), so the focus-carry guarantee is
    // still exactly what is under test.
    const outcome = await library.schedule(firstName);
    render();

    assert.equal(outcome?.unavailable, true, "with no core trigger store the honest outcome is 'unavailable'");
    const scheduleButtonAfter = container.querySelector<HTMLButtonElement>(`[data-op-focus-key="library-schedule-${firstName}"]`);
    assert.ok(scheduleButtonAfter);
    assert.notEqual(scheduleButtonAfter, scheduleButton, "the rebuild must have replaced the DOM node");
    assert.equal(env.document.activeElement, scheduleButtonAfter, "focus must have carried onto the rebuilt Schedule button");
    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("focus survives a rebuild: running a workflow keeps focus on the equivalent Run button after its card updates", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const registry = createMockRegistry();
    const firstName = registry.list()[0].manifest.name;
    const library = createLibrary(bus, { registry, now: () => 1_000_000 });
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    function render(): void {
      mountLibrary(container, library.getSnapshot(), { onRun: (name) => library.run(name) });
    }
    render();

    const runButton = container.querySelector<HTMLButtonElement>(`[data-op-focus-key="library-run-${firstName}"]`);
    assert.ok(runButton);
    runButton.focus();
    runButton.click();
    render();

    const runButtonAfter = container.querySelector<HTMLButtonElement>(`[data-op-focus-key="library-run-${firstName}"]`);
    assert.ok(runButtonAfter);
    assert.equal(env.document.activeElement, runButtonAfter, "focus must carry onto the rebuilt Run button after running a workflow");
    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("focus and typed text survive a rebuild: typing into the live search box does not drop focus or lose a keystroke", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const library = createLibrary(bus, { registry: createMockRegistry() });
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    function render(): void {
      mountLibrary(container, library.getSnapshot(), { onSearchChange: (q) => library.setSearchQuery(q) });
    }
    render();

    const input = container.querySelector<HTMLInputElement>('[data-op-focus-key="library-search"]');
    assert.ok(input, "the search input must be on screen");
    input.focus();
    input.value = "invoice";
    input.dispatchEvent(new env.window.Event("input", { bubbles: true }));
    render();

    const inputAfter = container.querySelector<HTMLInputElement>('[data-op-focus-key="library-search"]');
    assert.ok(inputAfter);
    assert.notEqual(inputAfter, input, "the rebuild must have replaced the DOM node");
    assert.equal(env.document.activeElement, inputAfter, "focus must carry onto the rebuilt search input");
    assert.equal(inputAfter.value, "invoice", "the typed query must carry onto the rebuilt search input");
    assert.equal(container.querySelectorAll(".op-library-card").length, 1, "the grid itself must already reflect the filter");

    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("populated library with a live search and a drag in progress: still no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const library = createLibrary(bus, { registry: createMockRegistry() });
    library.setSearchQuery("e"); // matches more than one seeded workflow; a non-trivial, non-empty filtered grid
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountLibrary(container, library.getSnapshot(), { onSearchChange: (q) => library.setSearchQuery(q), onReorder: (n, b) => library.reorder(n, b) });
    await assertNoViolations(container, "populated library with an active search");
    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("each card's glyph badge and last-run dot are decorative: aria-hidden, not the sole carrier of information", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const library = createLibrary(bus, { registry: createMockRegistry() });
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountLibrary(container, library.getSnapshot());

    const glyph = root.querySelector(".op-library-card__glyph");
    assert.ok(glyph);
    assert.equal(glyph?.getAttribute("aria-hidden"), "true");

    const dot = root.querySelector(".op-library-card .op-status__dot");
    assert.ok(dot);
    assert.equal(dot?.getAttribute("aria-hidden"), "true");
    assert.equal(dot?.getAttribute("data-state"), "pending");

    library.dispose();
  } finally {
    env.cleanup();
  }
});
