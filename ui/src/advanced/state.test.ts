import { test } from "node:test";
import assert from "node:assert/strict";
import { advancedSurfaceVisibility } from "./state.ts";
import { dslPreview } from "./dslPreview.ts";
import { createConnectedToolsStore } from "./connectedTools.ts";
import type { MockWorkflowRecord } from "../library/mockRegistry.ts";

test("default mode hides every Advanced surface", () => {
  assert.deepEqual(advancedSurfaceVisibility("default"), {
    dslEditor: false,
    rawWorkflowDetails: false,
    auditBrowser: false,
    connectedTools: false,
  });
});

test("advanced mode shows every Advanced surface", () => {
  assert.deepEqual(advancedSurfaceVisibility("advanced"), {
    dslEditor: true,
    rawWorkflowDetails: true,
    auditBrowser: true,
    connectedTools: true,
  });
});

function recordWith(stepSummary: string[]): MockWorkflowRecord {
  return {
    manifest: {
      v: 1,
      name: "sample",
      version: "1.0.0",
      description: "Sample workflow",
      step_summary: stepSummary,
      inputs_schema: { type: "object", properties: {} },
      capabilities: { apps: [], paths: [], network: false, risk_ceiling: "read" },
      dsl: { path: "workflows/sample.ts", hash: "0".repeat(64) },
    },
    steps: [],
    signed: false,
    dryRunOnly: true,
  };
}

test("dslPreview is empty for no record and for a record with no step summary", () => {
  assert.equal(dslPreview(undefined), "");
  assert.equal(dslPreview(recordWith([])), "");
});

test("dslPreview emits one commented statement per step, plus a name@version header", () => {
  const text = dslPreview(recordWith(['Click "Downloads"', "Copy the selection"]));
  assert.match(text, /^\/\/ sample@1\.0\.0/);
  assert.match(text, /\/\/ Click "Downloads"/);
  assert.match(text, /await step1\(\);/);
  assert.match(text, /\/\/ Copy the selection/);
  assert.match(text, /await step2\(\);/);
});

test("connected tools store lists the seeded tools and toggles enabled state", () => {
  const store = createConnectedToolsStore([
    { name: "filesystem", namespace: "mcp:filesystem", riskClass: "write", enabled: true },
    { name: "browser", namespace: "mcp:browser", riskClass: "write", enabled: false },
  ]);

  assert.equal(store.list().length, 2);
  assert.equal(store.list()[1].enabled, false);

  store.setEnabled("browser", true);
  assert.equal(store.list()[1].enabled, true);
  // The other entry is untouched.
  assert.equal(store.list()[0].enabled, true);
});

test("connected tools store notifies subscribers on toggle, ignores an unknown tool name", () => {
  const store = createConnectedToolsStore([{ name: "filesystem", namespace: "mcp:filesystem", riskClass: "write", enabled: true }]);
  let calls = 0;
  store.subscribe(() => calls++);

  store.setEnabled("does-not-exist", true);
  assert.equal(calls, 0);

  store.setEnabled("filesystem", false);
  assert.equal(calls, 1);
  assert.equal(store.list()[0].enabled, false);
});

test("independent connected-tools stores do not share state", () => {
  const a = createConnectedToolsStore([{ name: "filesystem", namespace: "mcp:filesystem", riskClass: "write", enabled: true }]);
  const b = createConnectedToolsStore([{ name: "filesystem", namespace: "mcp:filesystem", riskClass: "write", enabled: true }]);

  a.setEnabled("filesystem", false);

  assert.equal(a.list()[0].enabled, false);
  assert.equal(b.list()[0].enabled, true);
});
