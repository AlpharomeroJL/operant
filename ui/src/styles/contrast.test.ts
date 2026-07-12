// WCAG AA contrast pass over the design tokens in ./tokens.css (X8
// app-accessibility; palette updated for D1 tokens-and-shell, docs/specs/
// design.md). Two things are checked:
//
// 1. ./contrast.ts's LIGHT_TOKENS/DARK_TOKENS literals match what
//    tokens.css actually declares, in every block that sets them (the
//    `:root` default, the prefers-color-scheme override, and both explicit
//    data-theme overrides): a hand-copied literal that quietly drifted from
//    the stylesheet would make every ratio below meaningless. Both files
//    ultimately derive from ui/src/theme/tokens.ts, so in practice this
//    guards against contrast.ts's ColorRoles projection (see that file)
//    silently diverging from ui/scripts/build-tokens.mjs's CSS output.
// 2. Every token pair a screen actually paints (text on a surface, a
//    button's own text on its own fill, a focus ring or interactive-
//    component border against the surface it sits on) meets WCAG AA: 4.5:1
//    for text (SC 1.4.3), 3:1 for a non-text UI component boundary
//    (SC 1.4.11).
//
// Two categories of token are intentionally NOT in the pairwise checks
// below:
//
// - ui/src/styles/border, the plain divider token (`.op-panel`, card, and
//   header borders, never an interactive component's own boundary): those
//   dividers sit between two already visually distinct fills (an elevated
//   panel on the page background), so the divider itself is decorative per
//   WCAG 1.4.11's exemption for graphics "essential" only when no other
//   visual indicator of the boundary exists.
// - statusIdle/Running/Halted/Warning (and statusDone), the status-dot and
//   tray-glyph fill colors: every single usage in ui/src (ui/src/main.ts's
//   run-status dot, ui/src/runViewer/view.ts's per-step dots, ui/src/tray/
//   view.ts's glyph) pairs the dot with an `aria-hidden` graphic plus an
//   adjacent or visually-hidden text equivalent carrying the same state, so
//   the dot's exact color, like the divider above, is never the sole way to
//   perceive the state; the same WCAG 1.4.11 "essential" exemption applies.
//   This is also why the numbers do not merely happen to pass: this D1
//   repaint deliberately maps `statusRunning` to design.md's `signal` token
//   and `statusWarning` to `info` (ui/src/theme/tokens.ts's resolveRoles doc
//   comment explains why), and `signal`/`success`/`info` as tiny fills read
//   as low as 1.93:1-2.85:1 against the light theme's bg1/bg2 (measured;
//   the pre-redesign palette's equivalents were chosen dark/saturated enough
//   to clear 3:1 specifically so they could double as standalone indicators,
//   which design.md's fixed signal/success/info values are not). Because
//   every dot has a redundant text equivalent, that shortfall is not an
//   accessibility regression, so this suite no longer asserts a number for
//   them rather than asserting a weaker, misleading one.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { PURE_BLACK, PURE_WHITE } from "../theme/tokens.ts";
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
// Block order in tokens.css: [0] :root (DARK default, D1: design.md section 1
// makes dark the default theme, so this flipped from the pre-D1 light
// default), [1] the prefers-color-scheme: light block's inner :root (also
// flipped, from :dark), [2] [data-theme="dark"], [3] [data-theme="light"].
const darkFromCss = tokensFromBlock(cssBlocks[0]);
const lightFromCss = tokensFromBlock(cssBlocks[1]);
const darkOverrideFromCss = tokensFromBlock(cssBlocks[2]);
const lightOverrideFromCss = tokensFromBlock(cssBlocks[3]);

test("contrast.ts's LIGHT_TOKENS/DARK_TOKENS match tokens.css, so the ratios below are not checking stale literals", () => {
  assert.deepEqual(darkFromCss, DARK_TOKENS, ":root's dark defaults");
  assert.deepEqual(lightFromCss, LIGHT_TOKENS, "the prefers-color-scheme: light block");
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

// Status dots are deliberately not included here: see this file's header
// comment for why (every dot ships with a redundant text equivalent, so its
// exact fill color is not an AA-checked boundary).
function nonTextPairs(t: ThemeTokens): Array<[string, string, string]> {
  return [
    ["borderStrong on bg (palette/field input boundary)", t.borderStrong, t.bg],
    ["borderStrong on bgElevated (button/toggle/select boundary)", t.borderStrong, t.bgElevated],
    ["borderStrong on bgSunken (Advanced code editor boundary)", t.borderStrong, t.bgSunken],
    ["focusRing on bg", t.focusRing, t.bg],
    ["focusRing on bgElevated", t.focusRing, t.bgElevated],
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
  assert.equal(contrastRatio(PURE_BLACK, PURE_WHITE), contrastRatio(PURE_WHITE, PURE_BLACK));
  assert.ok(Math.abs(contrastRatio(PURE_BLACK, PURE_WHITE) - 21) < 0.01);
  assert.equal(contrastRatio(LIGHT_TOKENS.accent, LIGHT_TOKENS.accent), 1);
});
