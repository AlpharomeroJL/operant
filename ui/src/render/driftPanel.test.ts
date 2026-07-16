// @advanced
// Exempt from scripts/microcopy_lint.mjs (a test file, not shipped UI copy;
// same exemption ui/src/runViewer/view.test.ts takes): its descriptions name
// the wire concept "drift" (WorkflowDriftDetectedPayload, contracts/
// bus_events.md), which is correct vocabulary for a test and never rendered as
// UI text. The drift card's own visible copy stays plain, glossary-clean
// English (ui/src/render/strings.ts and the GL4 version labels).
//
// GLASS.md GL4 bar (drift patch panel): a real drift patch renders in the
// two-material grammar (GLASS.md section 4, G4), the SAME op-glass /
// op-glass--live vocabulary the run viewer uses for explore vs replay (G2). The
// proposed patch (what the model produced) is live-edged glass; the
// current/broken version (what is being replaced) is still, faded glass. Proven
// two ways: the class contract, and a getComputedStyle read off the REAL
// base.css so the live glow is present on the patch and absent on the broken
// half, not merely asserted by class name. Also axe-clean, since a drift offer
// is an alertdialog a screen-reader user must be able to act on.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { mountDriftCard, type DriftOfferView } from "./workflowView.ts";

const tokensCss = readFileSync(fileURLToPath(new URL("../styles/tokens.css", import.meta.url)), "utf8");
const baseCss = readFileSync(fileURLToPath(new URL("../styles/base.css", import.meta.url)), "utf8").replace(/@import[^;]+;/g, "");

const OFFER: DriftOfferView = {
  headline: "Something on screen moved",
  question: "Update the workflow?",
  text: "The Save button used to be at the top right.",
  preview: "Use the Save button now in the toolbar.",
  accept: "Update",
  dismiss: "Not now",
};

function mount(env: ReturnType<typeof createDomEnv>, withStyles = false): HTMLElement {
  if (withStyles) env.document.head.insertAdjacentHTML("beforeend", `<style>${tokensCss}</style><style>${baseCss}</style>`);
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  mountDriftCard(container, OFFER);
  return container;
}

test("the proposed patch is live-edged glass; the broken version is still, faded glass (the two-material grammar)", () => {
  const env = createDomEnv();
  try {
    const container = mount(env);
    const broken = container.querySelector<HTMLElement>(".op-change-card__version--broken");
    const patch = container.querySelector<HTMLElement>(".op-change-card__version--patch");
    assert.ok(broken && patch, "both halves render");

    assert.ok(broken.matches(".op-glass"), "the broken version is a glass surface");
    assert.ok(!broken.matches(".op-glass--live"), "the broken version is inert (still), not live");

    assert.ok(patch.matches(".op-glass"), "the patch is a glass surface");
    assert.ok(patch.matches(".op-glass--live"), "the patch the model produced is alive (live-edged)");
  } finally {
    env.cleanup();
  }
});

test("each half carries the right content: the current version and the proposed patch", () => {
  const env = createDomEnv();
  try {
    const container = mount(env);
    const brokenBody = container.querySelector(".op-change-card__version--broken .op-change-card__version-body");
    const patchBody = container.querySelector(".op-change-card__version--patch .op-change-card__version-body");
    assert.equal(brokenBody?.textContent, OFFER.text);
    assert.equal(patchBody?.textContent, OFFER.preview);
  } finally {
    env.cleanup();
  }
});

test("PROVABLY different materials by getComputedStyle: the patch glows, the broken half does not", () => {
  const env = createDomEnv();
  try {
    const container = mount(env, true);
    const broken = container.querySelector<HTMLElement>(".op-change-card__version--broken")!;
    const patch = container.querySelector<HTMLElement>(".op-change-card__version--patch")!;

    const brokenShadow = env.window.getComputedStyle(broken).boxShadow;
    const patchShadow = env.window.getComputedStyle(patch).boxShadow;
    assert.match(patchShadow, /glow-live/, "the live patch carries the amber glow");
    assert.equal(brokenShadow, "", "the inert broken half has no glow");
    assert.notEqual(patchShadow, brokenShadow, "the two halves compute to different surfaces");
  } finally {
    env.cleanup();
  }
});

test("a drift offer with only a broken version (no patch preview) still renders, with no live half", () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountDriftCard(container, { ...OFFER, preview: undefined });
    assert.ok(container.querySelector(".op-change-card__version--broken"), "the broken half renders");
    assert.equal(container.querySelector(".op-change-card__version--patch"), null, "no patch half without a preview");
    assert.equal(container.querySelector(".op-glass--live"), null, "nothing is live when there is no proposed patch");
  } finally {
    env.cleanup();
  }
});

test("the drift card has no axe violations", async () => {
  const env = createDomEnv();
  try {
    const container = mount(env);
    const results = await axe.run(container, { resultTypes: ["violations"] });
    assert.deepEqual(
      results.violations.map((v) => ({ id: v.id, help: v.help })),
      [],
      "axe-core violations on the drift card",
    );
  } finally {
    env.cleanup();
  }
});
