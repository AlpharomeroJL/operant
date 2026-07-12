// X8 app-accessibility bar for the first-run tour (new in H1, alongside its
// re-pointed screen map, ./state.ts's header comment): an axe-core scan of
// each of the four callout steps, plus a contextual hint, same pattern as
// every other screen's accessibility.test.ts under ui/src.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createTourStore, type TourStep } from "./state.ts";
import { hintStrings } from "./strings.ts";
import { mountTourCallout, mountContextualHint } from "./view.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

const STEPS: TourStep[] = ["dashboard", "library", "runs", "settings"];

for (const step of STEPS) {
  test(`tour callout at the "${step}" step: no axe violations`, async () => {
    const env = createDomEnv();
    try {
      const tour = createTourStore(step);
      const container = env.document.createElement("div");
      env.document.body.appendChild(container);
      mountTourCallout(container, tour.getSnapshot());
      await assertNoViolations(container, `tour callout (${step})`);
      tour.dispose();
    } finally {
      env.cleanup();
    }
  });
}

test("a contextual hint: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountContextualHint(container, "palette-hint", hintStrings.paletteHint, false);
    await assertNoViolations(container, "contextual hint");
  } finally {
    env.cleanup();
  }
});
