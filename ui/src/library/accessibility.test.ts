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

test("focus survives a rebuild: pressing Schedule keeps focus on the equivalent button in the rebuilt grid", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const registry = createMockRegistry();
    const firstName = registry.list()[0].manifest.name;
    let notice: string | null = null;
    const library = createLibrary(bus, {
      registry,
      onScheduleRequested: (_name, title) => {
        notice = title;
      },
    });
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
    scheduleButton.click();
    render();

    assert.ok(notice, "clicking Schedule must report the request");
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
