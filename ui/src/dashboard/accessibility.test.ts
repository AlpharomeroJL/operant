// X8 app-accessibility bar for the Home dashboard: an axe-core scan of the
// populated and empty states, same pattern as ui/src/library/accessibility.test.ts.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY } from "../bus/types.ts";
import { createDashboard } from "./state.ts";
import { mountDashboard } from "./view.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

test("populated dashboard (hero, sparkline, Up next, Recent runs): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const dashboard = createDashboard(bus, { now: () => 1_000_000 });
    bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
    bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 4, wall_ms: 400 });

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountDashboard(container, dashboard.getSnapshot());
    await assertNoViolations(container, "populated dashboard");
    dashboard.dispose();
  } finally {
    env.cleanup();
  }
});

test("empty dashboard (the quiet first-run invite): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const dashboard = createDashboard(bus, { upNext: [] });
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountDashboard(container, dashboard.getSnapshot());
    await assertNoViolations(container, "empty dashboard");
    dashboard.dispose();
  } finally {
    env.cleanup();
  }
});

test("the sparkline is hidden from assistive tech and carries a visually-hidden text equivalent instead", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const dashboard = createDashboard(bus);
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountDashboard(container, dashboard.getSnapshot());

    const svg = root.querySelector("svg.op-dashboard__sparkline");
    assert.ok(svg, "the sparkline svg must be present");
    assert.equal(svg?.getAttribute("aria-hidden"), "true");

    const hidden = root.querySelector(".op-visually-hidden");
    assert.ok(hidden?.textContent?.includes("Minutes saved by week"));

    dashboard.dispose();
  } finally {
    env.cleanup();
  }
});

test("each recent-run status dot is decorative (aria-hidden) with its own visually-hidden status text", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const dashboard = createDashboard(bus, { now: () => 1_000_000 });
    bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
    bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 4, wall_ms: 400 });

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountDashboard(container, dashboard.getSnapshot());

    const dot = root.querySelector(".op-status__dot");
    assert.ok(dot);
    assert.equal(dot?.getAttribute("aria-hidden"), "true");
    assert.equal(dot?.getAttribute("data-state"), "ok");

    const row = dot?.parentElement;
    const hiddenLabel = row?.querySelector(".op-visually-hidden");
    assert.equal(hiddenLabel?.textContent, "Completed");

    dashboard.dispose();
  } finally {
    env.cleanup();
  }
});
