// WCAG AA contrast pass over the design tokens in ./tokens.css (X8
// app-accessibility). Two things are checked:
//
// 1. ./contrast.ts's LIGHT_TOKENS/DARK_TOKENS literals match what
//    tokens.css actually declares, in every block that sets them (the
//    `:root` default, the prefers-color-scheme: dark block, and both
//    explicit data-theme overrides): a hand-copied literal that quietly
//    drifted from the stylesheet would make every ratio below meaningless.
// 2. Every token pair a screen actually paints (text on a surface, a
//    button's own text on its own fill, a status dot or focus ring or
//    interactive-component border against the surface it sits on) meets
//    WCAG AA: 4.5:1 for text (SC 1.4.3), 3:1 for a non-text UI component
//    boundary (SC 1.4.11).
//
// ui/src/styles/border, the plain divider token (`.op-panel`, card, and
// header borders, never an interactive component's own boundary), is
// intentionally not included below: those dividers sit between two already
// visually distinct fills (an elevated panel on the page background), so
// the divider itself is decorative per WCAG 1.4.11's exemption for graphics
// "essential" only when no other visual indicator of the boundary exists.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  contrastRatio,
  LIGHT_TOKENS,
  DARK_TOKENS,
  AA_TEXT_MIN,
  AA_NON_TEXT_MIN,
  type ThemeTokens,
} from "./contrast.ts";

const tokensCssPath = fileURLToPath(new URL("./tokens.css", import.meta.url));
const tokensCss = readFileSync(tokensCssPath, "utf8");

// tokens.css declares each color four times: the light default, the
// prefers-color-scheme: dark block, [data-theme="dark"], and
// [data-theme="light"]. Split on blank-ish boundaries is fragile; instead
// walk brace-delimited blocks in order, which is exactly how the four
// theme declarations are laid out in the file.
function blocks(css: string): string[] {
  const found: string[] = [];
  let depth = 0;
  let start = -1;
  for (let i = 0; i < css.length; i++) {
    if (css[i] === "{") {
      if (depth === 0) start = i;
      depth++;
    } else if (css[i] === "}") {
      depth--;
      if (depth === 0 && start !== -1) {
        found.push(css.slice(start + 1, i));
        start = -1;
      }
    }
  }
  return found;
}

function readVar(block: string, name: string): string {
  const re = new RegExp(`--op-color-${name}:\\s*(#[0-9a-fA-F]{6})`);
  const m = block.match(re);
  if (!m) throw new Error(`tokens.css block missing --op-color-${name}`);
  return m[1].toLowerCase();
}

function tokensFromBlock(block: string): ThemeTokens {
  return {
    bg: readVar(block, "bg"),
    bgElevated: readVar(block, "bg-elevated"),
    bgSunken: readVar(block, "bg-sunken"),
    border: readVar(block, "border"),
    borderStrong: readVar(block, "border-strong"),
    text: readVar(block, "text"),
    textMuted: readVar(block, "text-muted"),
    accent: readVar(block, "accent"),
    accentHover: readVar(block, "accent-hover"),
    accentText: readVar(block, "accent-text"),
    statusIdle: readVar(block, "status-idle"),
    statusRunning: readVar(block, "status-running"),
    statusHalted: readVar(block, "status-halted"),
    statusWarning: readVar(block, "status-warning"),
    focusRing: readVar(block, "focus-ring"),
  };
}

const cssBlocks = blocks(tokensCss);
// Block order in tokens.css: [0] :root (light default), [1] the
// prefers-color-scheme: dark block's inner :root, [2] [data-theme="dark"],
// [3] [data-theme="light"].
const lightFromCss = tokensFromBlock(cssBlocks[0]);
const darkFromCss = tokensFromBlock(cssBlocks[1]);
const darkOverrideFromCss = tokensFromBlock(cssBlocks[2]);
const lightOverrideFromCss = tokensFromBlock(cssBlocks[3]);

test("contrast.ts's LIGHT_TOKENS/DARK_TOKENS match tokens.css, so the ratios below are not checking stale literals", () => {
  assert.deepEqual(lightFromCss, LIGHT_TOKENS, ":root's light defaults");
  assert.deepEqual(darkFromCss, DARK_TOKENS, "the prefers-color-scheme: dark block");
  assert.deepEqual(darkOverrideFromCss, DARK_TOKENS, '[data-theme="dark"]');
  assert.deepEqual(lightOverrideFromCss, LIGHT_TOKENS, '[data-theme="light"]');
});

function textPairs(t: ThemeTokens): Array<[string, string, string]> {
  return [
    ["text on bg", t.text, t.bg],
    ["text on bgElevated", t.text, t.bgElevated],
    ["text on bgSunken", t.text, t.bgSunken],
    ["textMuted on bg", t.textMuted, t.bg],
    ["textMuted on bgElevated", t.textMuted, t.bgElevated],
    ["textMuted on bgSunken", t.textMuted, t.bgSunken],
    ["accentText on accent (primary button fill)", t.accentText, t.accent],
    ["accentText on accentHover (primary button, hover fill)", t.accentText, t.accentHover],
  ];
}

function nonTextPairs(t: ThemeTokens): Array<[string, string, string]> {
  return [
    ["borderStrong on bg (palette/field input boundary)", t.borderStrong, t.bg],
    ["borderStrong on bgElevated (button/toggle/select boundary)", t.borderStrong, t.bgElevated],
    ["borderStrong on bgSunken (Advanced code editor boundary)", t.borderStrong, t.bgSunken],
    ["focusRing on bg", t.focusRing, t.bg],
    ["focusRing on bgElevated", t.focusRing, t.bgElevated],
    ["statusIdle dot on bgElevated", t.statusIdle, t.bgElevated],
    ["statusIdle dot on bgSunken", t.statusIdle, t.bgSunken],
    ["statusRunning dot on bgElevated", t.statusRunning, t.bgElevated],
    ["statusRunning dot on bgSunken", t.statusRunning, t.bgSunken],
    ["statusHalted dot on bgElevated", t.statusHalted, t.bgElevated],
    ["statusHalted dot on bgSunken", t.statusHalted, t.bgSunken],
    ["statusWarning dot on bgElevated", t.statusWarning, t.bgElevated],
    ["statusWarning dot on bgSunken", t.statusWarning, t.bgSunken],
  ];
}

for (const [themeName, tokens] of [
  ["light", LIGHT_TOKENS],
  ["dark", DARK_TOKENS],
] as const) {
  test(`${themeName} theme: every text pair meets WCAG AA (${AA_TEXT_MIN}:1)`, () => {
    for (const [label, fg, bg] of textPairs(tokens)) {
      const ratio = contrastRatio(fg, bg);
      assert.ok(ratio >= AA_TEXT_MIN, `${label}: ${ratio.toFixed(2)}:1, needs ${AA_TEXT_MIN}:1 (fg ${fg} bg ${bg})`);
    }
  });

  test(`${themeName} theme: every interactive-component boundary meets WCAG AA non-text contrast (${AA_NON_TEXT_MIN}:1)`, () => {
    for (const [label, fg, bg] of nonTextPairs(tokens)) {
      const ratio = contrastRatio(fg, bg);
      assert.ok(ratio >= AA_NON_TEXT_MIN, `${label}: ${ratio.toFixed(2)}:1, needs ${AA_NON_TEXT_MIN}:1 (fg ${fg} bg ${bg})`);
    }
  });
}

test("contrastRatio is symmetric and maxes out at 21:1 for true black on true white", () => {
  assert.equal(contrastRatio("#000000", "#ffffff"), contrastRatio("#ffffff", "#000000"));
  assert.ok(Math.abs(contrastRatio("#000000", "#ffffff") - 21) < 0.01);
  assert.equal(contrastRatio("#2f5fed", "#2f5fed"), 1);
});
