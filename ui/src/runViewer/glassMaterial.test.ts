// @advanced
// Exempt from scripts/microcopy_lint.mjs (a test file, not shipped UI copy;
// same exemption ./view.test.ts takes): its assertions name wire vocabulary
// ("explore"/"replay") from the bus contract.
//
// GLASS.md GL2 bar (the thesis, made physical): explore and replay must render
// PROVABLY different materials, "verified by computed-style assertions, not by
// eye" (GLASS.md section 9, GL2). This drives the real mountRunViewer into a
// jsdom document that has the REAL ui/src/styles/tokens.css + base.css injected,
// then reads getComputedStyle off the run-viewer panel:
//
//   - explore (modelOn true): the panel wears op-glass--live, so its computed
//     box-shadow carries the amber glowLive; the surface is alive.
//   - replay (modelOn false): the panel wears op-glass only, so its computed
//     box-shadow is empty (no glow); the surface is still.
//
// jsdom does not substitute var() into color longhands (border-color computes to
// black either way), but it DOES return box-shadow and backdrop-filter verbatim
// including their var() references, and resolves custom properties via
// getPropertyValue, which is exactly the pair of signals these assertions hang
// on. The GL1 material.test.ts already proved, by reading tokens.css, that
// edgeLive/glowLive are the amber signal color and edgeStill is the neutral
// hairline, so the class the panel selects here is the whole difference.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY, type RunMode } from "../bus/types.ts";
import { createRunViewer } from "./state.ts";
import { mountRunViewer } from "./view.ts";

const tokensCss = readFileSync(fileURLToPath(new URL("../styles/tokens.css", import.meta.url)), "utf8");
// base.css @imports tokens.css / fonts.css; jsdom cannot resolve those relative
// specifiers, so strip them and inject tokens.css ourselves (fonts are irrelevant
// to computed color/shadow). This mounts the real base.css rules, not a hand-
// authored stand-in, so the cascade under test is the shipping one.
const baseCss = readFileSync(fileURLToPath(new URL("../styles/base.css", import.meta.url)), "utf8").replace(/@import[^;]+;/g, "");

function withGlassStyles(env: ReturnType<typeof createDomEnv>): void {
  env.document.head.insertAdjacentHTML("beforeend", `<style>${tokensCss}</style><style>${baseCss}</style>`);
}

/** Mount the run viewer for a run of `mode` and return its panel element. */
function panelForMode(env: ReturnType<typeof createDomEnv>, mode: RunMode): HTMLElement {
  const bus = createMockBusClient();
  const viewer = createRunViewer(bus);
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  bus.publish("run.started", { run_id: "r1", goal: "g", mode });
  mountRunViewer(container, viewer.getSnapshot());
  const panel = container.querySelector<HTMLElement>(".op-run-viewer");
  assert.ok(panel, "the run viewer must mount a panel");
  viewer.dispose();
  return panel;
}

test("the run-viewer panel is glass in every run mode (one of GLASS.md's four glass moments)", () => {
  const env = createDomEnv();
  try {
    withGlassStyles(env);
    for (const mode of [RUN_MODE_EXPLORE, RUN_MODE_REPLAY]) {
      const panel = panelForMode(env, mode);
      assert.ok(panel.matches(".op-glass"), `${mode}: the flight recorder is a glass surface`);
      const blur = env.window.getComputedStyle(panel).getPropertyValue("backdrop-filter");
      assert.match(blur, /blur\(/, `${mode}: a glass panel carries a backdrop blur`);
    }
  } finally {
    env.cleanup();
  }
});

test("explore renders the LIVE material: op-glass--live, and the amber glow is present in the computed box-shadow", () => {
  const env = createDomEnv();
  try {
    withGlassStyles(env);
    const panel = panelForMode(env, RUN_MODE_EXPLORE);
    assert.ok(panel.matches(".op-glass--live"), "an explore run wears the live material");
    const shadow = env.window.getComputedStyle(panel).boxShadow;
    assert.match(shadow, /glow-live/, "the live glow (glowLive) is present on the explore surface");
  } finally {
    env.cleanup();
  }
});

test("replay renders the STILL material: op-glass without op-glass--live, and no glow in the computed box-shadow", () => {
  const env = createDomEnv();
  try {
    withGlassStyles(env);
    const panel = panelForMode(env, RUN_MODE_REPLAY);
    assert.ok(panel.matches(".op-glass"), "a replay run is still a glass surface");
    assert.ok(!panel.matches(".op-glass--live"), "a replay run must NOT wear the live material");
    const shadow = env.window.getComputedStyle(panel).boxShadow;
    assert.equal(shadow, "", "the still replay surface has no glow at all");
  } finally {
    env.cleanup();
  }
});

test("explore and replay are PROVABLY different materials by getComputedStyle, not by eye (GLASS.md GL2 bar)", () => {
  const env = createDomEnv();
  try {
    withGlassStyles(env);
    const exploreShadow = env.window.getComputedStyle(panelForMode(env, RUN_MODE_EXPLORE)).boxShadow;
    const replayShadow = env.window.getComputedStyle(panelForMode(env, RUN_MODE_REPLAY)).boxShadow;
    assert.notEqual(exploreShadow, replayShadow, "the two run modes must compute to different surfaces");
    assert.match(exploreShadow, /glow-live/, "explore is the alive, amber-lit one");
    assert.equal(replayShadow, "", "replay is the still, glow-free one");
  } finally {
    env.cleanup();
  }
});

test("the live and still edges are distinct materials in force (edgeLive is amber, edgeStill is not)", () => {
  const env = createDomEnv();
  try {
    withGlassStyles(env);
    const style = env.window.getComputedStyle(panelForMode(env, RUN_MODE_EXPLORE));
    const edgeLive = style.getPropertyValue("--op-material-edge-live").trim();
    const edgeStill = style.getPropertyValue("--op-material-edge-still").trim();
    assert.ok(edgeLive.length > 0 && edgeStill.length > 0, "both edge tokens resolve");
    assert.notEqual(edgeLive, edgeStill, "the explore edge and the replay edge are different colors");
  } finally {
    env.cleanup();
  }
});
