// axe-core scan of the toast, the same X8 accessibility bar every other
// screen's accessibility.test.ts holds itself to (see ui/src/undo/
// accessibility.test.ts). Both the actionable and message-only shapes are
// scanned, since they render different markup (an extra button).

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { mountToast } from "./view.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

test("actionable toast: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountToast(container, { toast: { id: "t1", message: "Run complete, 14 steps", action: { label: "Undo this run" }, runId: "r1" } });
    await assertNoViolations(container, "actionable toast");
  } finally {
    env.cleanup();
  }
});

test("message-only toast: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountToast(container, { toast: { id: "t1", message: "Saved as workflow" } });
    await assertNoViolations(container, "message-only toast");
  } finally {
    env.cleanup();
  }
});
