// @advanced
// Exempt from scripts/microcopy_lint.mjs (a test file, not shipped UI copy).
//
// GLASS.md section 6 (Q2 budgets stand). This is the perf-budget harness the
// GLASS pack asks for. It has two halves:
//
//   1. The AUTHORITATIVE gate is a STRUCTURAL PROXY, per GLASS.md's own fallback
//      clause ("If jsdom timing is unreliable, assert a structural proxy ... and
//      document why"). jsdom timing IS unreliable here, and specifically so: the
//      Q2 budgets exist to bound the cost of COMPOSITING the backdrop-filter blur
//      (GLASS.md section 0's sixth trap, "animating backdrop-filter forces
//      continuous re-rasterization"). jsdom performs no layout, paint, or
//      compositing at all, so a millisecond measured here reflects DOM
//      construction only and says nothing about the blur budget. What actually
//      protects the budget is a rule that can be checked structurally: never
//      animate backdrop-filter, filter, or box-shadow (GLASS.md section 6);
//      animate opacity/transform on a pre-composited layer instead. This half
//      greps the shipping ui/src/styles/base.css and fails if that rule is ever
//      broken, which is the real, non-flaky guarantee.
//
//   2. A performance.now() smoke measurement around palette open with 100
//      workflows, kept as a coarse "no pathological blowup" sanity check under a
//      deliberately generous ceiling (NOT the 120ms budget, which jsdom cannot
//      meaningfully verify). It exists so the harness literally measures the open
//      path, but it is not what guards the budget; the structural proxy is.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createPaletteController } from "../palette/state.ts";
import { mountPalette } from "../palette/view.ts";
import type { PaletteEntry } from "../palette/catalog.ts";

const baseCss = readFileSync(fileURLToPath(new URL("../styles/base.css", import.meta.url)), "utf8");

// The three properties GLASS.md section 6 forbids animating, plus `all` (a
// blanket transition would sweep them in). Matched as whole words: `\bfilter\b`
// also catches `backdrop-filter` (the hyphen is a word boundary), so both the
// filter and backdrop-filter forms are covered by one token.
const FORBIDDEN_IN_TRANSITION = /\b(all|filter|box-shadow)\b/i;
const FORBIDDEN_KEYFRAME_PROP = /(?:^|[;{\s])(backdrop-filter|filter|box-shadow)\s*:/i;

/** Extract every @keyframes block body, brace-aware (keyframes nest 0%{...} selectors). */
function keyframeBodies(css: string): string[] {
  const bodies: string[] = [];
  const opener = /@keyframes\s+[\w-]+\s*\{/g;
  // exec advances opener.lastIndex; from there walk brace depth to the block's
  // matching close (the capture itself is unused, so it is not bound to a var).
  while (opener.exec(css) !== null) {
    let depth = 1;
    let i = opener.lastIndex;
    const start = i;
    while (i < css.length && depth > 0) {
      if (css[i] === "{") depth++;
      else if (css[i] === "}") depth--;
      i++;
    }
    bodies.push(css.slice(start, i - 1));
    opener.lastIndex = i;
  }
  return bodies;
}

/** Every `transition:` / `transition-property:` declaration value in the sheet. */
function transitionValues(css: string): string[] {
  const values: string[] = [];
  const re = /transition(?:-property)?\s*:\s*([^;}]+)/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(css)) !== null) values.push(m[1]);
  return values;
}

test("STRUCTURAL PROXY (authoritative): no transition animates backdrop-filter, filter, box-shadow, or `all`", () => {
  const offenders = transitionValues(baseCss).filter((v) => FORBIDDEN_IN_TRANSITION.test(v));
  assert.deepEqual(
    offenders,
    [],
    "GLASS.md section 6: animate opacity/transform only; a transition of backdrop-filter/filter/box-shadow (or `all`) re-rasterizes the blur and blows the Q2 budget",
  );
});

test("STRUCTURAL PROXY (authoritative): no @keyframes tweens backdrop-filter, filter, or box-shadow", () => {
  const offenders = keyframeBodies(baseCss).filter((body) => FORBIDDEN_KEYFRAME_PROP.test(body));
  assert.deepEqual(
    offenders.map((b) => b.trim().slice(0, 60)),
    [],
    "GLASS.md section 6: a keyframe that tweens the blur/shadow re-rasterizes every frame; the shimmer and tray pulse animate opacity only",
  );
});

test("sanity: the sheet does carry glass (so the proxy is guarding something real, not vacuously passing)", () => {
  // Guards against the proxy passing simply because glass was removed: the four
  // glass moments and the shimmer keyframe must actually be present.
  assert.match(baseCss, /backdrop-filter:\s*blur\(/, "the glass surfaces exist");
  assert.match(baseCss, /@keyframes\s+op-glass-shimmer/, "the shimmer keyframe exists");
  assert.ok(keyframeBodies(baseCss).length >= 1, "there is at least one keyframe to have checked");
});

test("performance.now() smoke: opening the palette with 100 workflows completes without a pathological blowup", () => {
  // NON-AUTHORITATIVE (see this file's header): jsdom does no compositing, so
  // this bounds DOM-construction cost only, under a generous ceiling, never the
  // 120ms blur budget. The structural proxy above is the real gate.
  const env = createDomEnv();
  try {
    const controller = createPaletteController();
    const entries: PaletteEntry[] = Array.from({ length: 100 }, (_, i) => ({
      id: `wf-${i}`,
      kind: "workflow",
      title: `Workflow number ${i}`,
      keywords: [`workflow-${i}`],
    }));
    controller.setEntries(entries);
    controller.open();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    const start = performance.now();
    mountPalette(container, controller.getSnapshot());
    const elapsed = performance.now() - start;

    assert.ok(container.querySelector(".op-palette-overlay"), "the palette actually mounted");
    assert.ok(Number.isFinite(elapsed), "the open path was measured");
    assert.ok(elapsed < 500, `palette open smoke bound (jsdom construction only): took ${elapsed.toFixed(1)}ms`);
    controller.dispose();
  } finally {
    env.cleanup();
  }
});
