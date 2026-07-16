// X8 app-accessibility bar for the GL3 kill-switch overlay (GLASS.md section 4,
// G3), the matching *accessibility.test.ts every new surface in this app ships
// (see ui/src/runViewer/accessibility.test.ts and ui/src/palette/
// accessibility.test.ts): an axe-core scan of the revealed panic overlay. The
// overlay is an alertdialog with a programmatic name (aria-labelledby) and a
// description (aria-describedby), so a screen-reader user is told the surface is
// severed, not left guessing why the app stopped responding.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { mountKillSwitchOverlay } from "./killOverlay.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

function setup(env: ReturnType<typeof createDomEnv>): { backdrop: HTMLElement; mount: HTMLElement } {
  const backdrop = env.document.createElement("div");
  backdrop.className = "op-modal-backdrop op-kill-backdrop";
  const mount = env.document.createElement("div");
  backdrop.appendChild(mount);
  env.document.body.appendChild(backdrop);
  return { backdrop, mount };
}

test("the revealed kill-switch overlay has no axe violations", async () => {
  const env = createDomEnv();
  try {
    const { backdrop, mount } = setup(env);
    const overlay = mountKillSwitchOverlay(backdrop, mount);
    overlay.reveal();
    await assertNoViolations(backdrop, "revealed kill-switch overlay");
  } finally {
    env.cleanup();
  }
});

test("the overlay carries a programmatic accessible name and description", () => {
  const env = createDomEnv();
  try {
    const { backdrop, mount } = setup(env);
    const overlay = mountKillSwitchOverlay(backdrop, mount);
    overlay.reveal();

    const labelId = overlay.panel.getAttribute("aria-labelledby");
    const descId = overlay.panel.getAttribute("aria-describedby");
    assert.ok(labelId && env.document.getElementById(labelId)?.textContent, "the alertdialog is named by a real, non-empty element");
    assert.ok(descId && env.document.getElementById(descId)?.textContent, "the alertdialog is described by a real, non-empty element");
  } finally {
    env.cleanup();
  }
});
