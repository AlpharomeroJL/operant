// DOM behavior of the Home dashboard against the real data source (B6): mounts
// ./view.ts from a source-driven snapshot and asserts the actual rendered
// markup, the "verify via DOM assertions" bar. Companion to ./accessibility.test.ts
// (axe scans) and ./state.test.ts (pure snapshot logic).

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { createDashboard } from "./state.ts";
import { mountDashboard } from "./view.ts";
import type { DashboardSource } from "./source.ts";

function fakeSource(over: Partial<DashboardSource> = {}): DashboardSource {
  return {
    getWeeklyMetrics: async () => [],
    getRecentRuns: async () => [],
    getUpcomingRuns: async () => [],
    ...over,
  };
}

/** The `.op-dashboard__section` whose heading carries the given id (Up next / Recent runs). */
function sectionByHeading(root: Element, headingId: string): Element | null {
  return root.querySelector(`#${headingId}`)?.closest(".op-dashboard__section") ?? null;
}

test("DOM, real source: the hero, Up next, and Recent runs all render real values from the source", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const source = fakeSource({
      getWeeklyMetrics: async () => [
        { week: "a", minutesSaved: 30 },
        { week: "b", minutesSaved: 192 },
      ],
      getRecentRuns: async () => [
        { runId: "r1", title: "Copy the invoice total into the spreadsheet", outcome: "ok", steps: 4, completedAtMs: 1_000_000 },
      ],
      getUpcomingRuns: async () => [{ workflowName: "weekly-report-email", whenLabel: "tomorrow at 9 am" }],
    });
    const dashboard = createDashboard(bus, { source, now: () => 1_000_000, registry: createMockRegistry([]) });
    await dashboard.refresh();

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountDashboard(container, dashboard.getSnapshot());

    // Hero: 192 minutes (the newest week) is 3.2 hours.
    assert.equal(root.querySelector(".op-dashboard__hero-line")?.textContent, "Operant saved you 3.2 hours this week");
    // A real polyline was plotted (two points).
    assert.ok((root.querySelector(".op-dashboard__sparkline-line")?.getAttribute("points") ?? "").length > 0);

    const upNext = sectionByHeading(root, "op-dashboard-upnext-heading");
    assert.ok(upNext);
    assert.equal(upNext?.querySelector(".op-dashboard__row-title")?.textContent, "weekly-report-email");
    assert.equal(upNext?.querySelector(".op-dashboard__row-when")?.textContent, "tomorrow at 9 am");

    const recent = sectionByHeading(root, "op-dashboard-recent-heading");
    assert.ok(recent);
    assert.equal(recent?.querySelector(".op-dashboard__row-title")?.textContent, "Copy the invoice total into the spreadsheet");
    assert.equal(recent?.querySelector(".op-dashboard__row-outcome")?.textContent, "Run complete, 4 steps");
    assert.equal(recent?.querySelector(".op-status__dot")?.getAttribute("data-state"), "ok");

    dashboard.dispose();
  } finally {
    env.cleanup();
  }
});

test("DOM, honest empty: a real source with nothing recorded renders the empty hero and the teach-first-workflow invite, not fixture numbers", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const dashboard = createDashboard(bus, { source: fakeSource(), now: () => 1_000_000 });
    await dashboard.refresh();

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountDashboard(container, dashboard.getSnapshot());

    assert.equal(root.querySelector(".op-dashboard__hero-line")?.textContent, "No time saved to show yet");
    // Never the fixture hero.
    assert.notEqual(root.querySelector(".op-dashboard__hero-line")?.textContent, "Operant saved you 3.2 hours this week");
    // The quiet first-run invite, with its one specific action.
    const empty = root.querySelector(".op-empty-state");
    assert.ok(empty, "the empty-state invite must render");
    assert.equal(empty?.querySelector(".op-button--primary")?.textContent, "Teach your first workflow");
    // No Up next / Recent runs sections while in the whole-dashboard empty state.
    assert.equal(sectionByHeading(root, "op-dashboard-upnext-heading"), null);
    assert.equal(sectionByHeading(root, "op-dashboard-recent-heading"), null);

    dashboard.dispose();
  } finally {
    env.cleanup();
  }
});

test("DOM, not_implemented Up next: the scheduler being unwired shows 'Nothing scheduled yet.', with real Recent runs alongside", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const source = fakeSource({
      getWeeklyMetrics: async () => [{ week: "this week", minutesSaved: 60 }],
      getRecentRuns: async () => [{ runId: "r1", title: "Back up photos", outcome: "failed", steps: 1, completedAtMs: 1_000_000 }],
      getUpcomingRuns: async () => [], // list_triggers answered not_implemented -> empty, not fake entries
    });
    const dashboard = createDashboard(bus, { source, now: () => 1_000_000 });
    await dashboard.refresh();

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountDashboard(container, dashboard.getSnapshot());

    const upNext = sectionByHeading(root, "op-dashboard-upnext-heading");
    assert.ok(upNext);
    assert.equal(upNext?.querySelector(".op-empty")?.textContent, "Nothing scheduled yet.");
    assert.equal(upNext?.querySelector(".op-dashboard__row"), null, "no scheduled rows are rendered");

    const recent = sectionByHeading(root, "op-dashboard-recent-heading");
    assert.equal(recent?.querySelector(".op-dashboard__row-title")?.textContent, "Back up photos");
    assert.equal(recent?.querySelector(".op-dashboard__row-outcome")?.textContent, "Run did not finish");

    dashboard.dispose();
  } finally {
    env.cleanup();
  }
});
