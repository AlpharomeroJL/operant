// GLASS.md GL2 (G1, the command palette as the first glass moment): the palette
// overlay must render as glass, "blurPanel over the app content, edgeStill,
// scrimStrong behind rows" (GLASS.md section 4, G1). Proven with the REAL
// ui/src/styles/tokens.css + base.css injected into jsdom, then read back with
// getComputedStyle: the overlay carries op-glass and computes a backdrop blur.
// jsdom returns backdrop-filter verbatim (var() and all) even though it will not
// substitute var() into color longhands, which is the signal this hangs on; GL1
// material.test.ts already proved what the glass tokens resolve to.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createPaletteController } from "./state.ts";
import { mountPalette } from "./view.ts";
import type { PaletteEntry } from "./catalog.ts";

const tokensCss = readFileSync(fileURLToPath(new URL("../styles/tokens.css", import.meta.url)), "utf8");
const baseCss = readFileSync(fileURLToPath(new URL("../styles/base.css", import.meta.url)), "utf8").replace(/@import[^;]+;/g, "");

const ENTRIES: PaletteEntry[] = [
  { id: "wf-copy-invoice", kind: "workflow", title: "Copy the invoice total", keywords: ["copy-invoice"] },
];

function mountOverlay(env: ReturnType<typeof createDomEnv>): HTMLElement {
  env.document.head.insertAdjacentHTML("beforeend", `<style>${tokensCss}</style><style>${baseCss}</style>`);
  const controller = createPaletteController();
  controller.setEntries(ENTRIES);
  controller.open();
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  mountPalette(container, controller.getSnapshot());
  const overlay = container.querySelector<HTMLElement>(".op-palette-overlay");
  assert.ok(overlay, "the palette must mount an overlay");
  controller.dispose();
  return overlay;
}

test("the palette overlay is glass: it carries op-glass and computes a backdrop blur", () => {
  const env = createDomEnv();
  try {
    const overlay = mountOverlay(env);
    assert.ok(overlay.matches(".op-glass"), "the floating palette is a glass surface (GLASS.md G1)");
    const blur = env.window.getComputedStyle(overlay).getPropertyValue("backdrop-filter");
    assert.match(blur, /blur\(/, "blurPanel is applied over the app content behind the palette");
  } finally {
    env.cleanup();
  }
});

test("the palette wears the STILL material only: it is chrome, never model-live", () => {
  const env = createDomEnv();
  try {
    const overlay = mountOverlay(env);
    assert.ok(!overlay.matches(".op-glass--live"), "the palette never carries the live/amber material");
    // The still edge token is in force (the neutral hairline, not the amber edge).
    const style = env.window.getComputedStyle(overlay);
    const edgeStill = style.getPropertyValue("--op-material-edge-still").trim();
    const edgeLive = style.getPropertyValue("--op-material-edge-live").trim();
    assert.ok(edgeStill.length > 0, "the still edge value resolves");
    assert.notEqual(edgeStill, edgeLive, "still and live are distinct materials");
  } finally {
    env.cleanup();
  }
});
