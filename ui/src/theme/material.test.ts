// @advanced
// Test vocabulary (token, invariant, material scale) asserts internals, not
// user-facing copy, so this file is exempt from the microcopy glossary lint.
// GLASS.md GL1 (material-tokens): proves the semantic-glass material scale is
// actually emitted into ui/src/styles/tokens.css and that its keystone value,
// scrimStrong, resolves to a computed, opaque-enough text surface. Mirrors the
// approach ./contrast.test.ts uses for the color tokens: read the generated
// stylesheet, walk its four theme blocks, and cross-check each declared value
// against ui/src/theme/tokens.ts (the single source of truth) so a build that
// quietly drifted would fail here.
//
// No raw hex literal appears in this file (scripts/check_rawhex.mjs scans it):
// every palette color is imported from tokens.ts, and the one #rrggbb this file
// produces (compositeOver's return) is built from digits at run time, never
// written as a literal.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  DARK_MATERIAL,
  LIGHT_MATERIAL,
  MATERIAL_ROLES,
  MATERIAL,
  MOTION_GLASS,
  PALETTES,
} from "./tokens.ts";
import { contrastRatio, relativeLuminance, AA_TEXT_MIN } from "../styles/contrast.ts";

const tokensCssPath = fileURLToPath(new URL("../styles/tokens.css", import.meta.url));
const tokensCss = readFileSync(tokensCssPath, "utf8");

// GLASS.md section 2's per-theme material colors, one row driving every check
// below: [cssVarSuffix, MaterialRoles field, source palette token, alpha]. Each
// value must be exactly that palette token at that alpha (an rgba-of-token
// derivation), which is what keeps amber the only hue and introduces no new
// identity color.
const MATERIAL_COLOR_SPEC = [
  ["scrim-weak", "scrimWeak", "bg1", 0.55],
  ["scrim-strong", "scrimStrong", "bg1", 0.82],
  ["edge-still", "edgeStill", "hairline", 0.14],
  ["edge-live", "edgeLive", "signal", 0.32],
  ["glow-live", "glowLive", "signal", 0.1],
] as const;

// Same top-level brace-block walk ./contrast.test.ts uses; see its comment.
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

function readMaterialVar(block: string, suffix: string): string {
  const re = new RegExp(`--op-material-${suffix}:\\s*(rgba\\([^)]*\\))`);
  const m = block.match(re);
  if (!m) throw new Error(`tokens.css block missing --op-material-${suffix}`);
  return m[1];
}

function parseRgba(rgba: string): { r: number; g: number; b: number; a: number } {
  const m = rgba.match(/^rgba\((\d+),\s*(\d+),\s*(\d+),\s*([0-9.]+)\)$/);
  if (!m) throw new Error(`expected an rgba(...) string, got ${rgba}`);
  return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]), a: Number(m[4]) };
}

function parseHexRgb(hex: string): { r: number; g: number; b: number } {
  const clean = hex.replace("#", "");
  if (clean.length !== 6) throw new Error(`expected a #rrggbb color, got ${hex}`);
  return {
    r: parseInt(clean.slice(0, 2), 16),
    g: parseInt(clean.slice(2, 4), 16),
    b: parseInt(clean.slice(4, 6), 16),
  };
}

// Composite a translucent rgba over an opaque #rrggbb backdrop, returning the
// resulting opaque #rrggbb (standard source-over alpha compositing). Lets us
// ask what a glass surface actually resolves to over the app's own surfaces.
function compositeOver(rgba: string, backdropHex: string): string {
  const fg = parseRgba(rgba);
  const bg = parseHexRgb(backdropHex);
  const mix = (f: number, b: number) => Math.round(f * fg.a + b * (1 - fg.a));
  const pair = (n: number) => n.toString(16).padStart(2, "0");
  return "#" + pair(mix(fg.r, bg.r)) + pair(mix(fg.g, bg.g)) + pair(mix(fg.b, bg.b));
}

const cssBlocks = blocks(tokensCss);
// Block order in tokens.css (documented in ./contrast.test.ts): [0] :root dark
// default, [1] the prefers-color-scheme: light block's inner :root, [2]
// [data-theme="dark"], [3] [data-theme="light"].
const themeBlocks = [
  { idx: 0, material: DARK_MATERIAL, label: ":root dark default" },
  { idx: 1, material: LIGHT_MATERIAL, label: "prefers-color-scheme: light" },
  { idx: 2, material: DARK_MATERIAL, label: '[data-theme="dark"]' },
  { idx: 3, material: LIGHT_MATERIAL, label: '[data-theme="light"]' },
] as const;

test("every per-theme material var is emitted in all four theme blocks and matches tokens.ts", () => {
  for (const { idx, material, label } of themeBlocks) {
    for (const [suffix, field] of MATERIAL_COLOR_SPEC) {
      const fromCss = readMaterialVar(cssBlocks[idx], suffix);
      assert.equal(fromCss, material[field], `${label}: --op-material-${suffix} should match tokens.ts`);
    }
  }
});

test("the theme-invariant material vars (blur / saturate / shimmer) are emitted once on :root and match tokens.ts", () => {
  const root = cssBlocks[0];
  assert.match(root, /--op-material-blur-panel:\s*16px;/);
  assert.match(root, /--op-material-blur-overlay:\s*40px;/);
  assert.match(root, /--op-material-sat-panel:\s*140%;/);
  assert.match(root, /--op-motion-glass-shimmer:\s*2400ms ease-in-out infinite;/);

  assert.equal(MATERIAL.blurPanel, "16px");
  assert.equal(MATERIAL.blurOverlay, "40px");
  assert.equal(MATERIAL.satPanel, "140%");
  // The shimmer drives an ::after overlay's opacity (GLASS.md section 2): a
  // finite duration, an easing, and an infinite loop.
  assert.equal(MOTION_GLASS.edgeShimmer, "2400ms ease-in-out infinite");
});

test("every material color is an alpha derivation of a palette token, introducing no new hue", () => {
  for (const theme of ["dark", "light"] as const) {
    const material = MATERIAL_ROLES[theme];
    const palette = PALETTES[theme];
    for (const [, field, source, alpha] of MATERIAL_COLOR_SPEC) {
      const got = parseRgba(material[field]);
      const src = parseHexRgb(palette[source]);
      assert.deepEqual(
        { r: got.r, g: got.g, b: got.b },
        src,
        `${theme} ${field} should be the ${source} token's rgb (no new hue)`,
      );
      assert.equal(got.a, alpha, `${theme} ${field} alpha`);
    }
  }
});

test("scrimStrong resolves to a computed, opaque-enough surface that keeps text legible (GLASS.md section 2 contrast rule)", () => {
  for (const theme of ["dark", "light"] as const) {
    const material = MATERIAL_ROLES[theme];
    const palette = PALETTES[theme];
    const strong = parseRgba(material.scrimStrong);
    const weak = parseRgba(material.scrimWeak);

    // Opaque-enough: scrimStrong is the default text surface on glass, so its
    // alpha is high (0.82) and strictly greater than the dimming-veil scrimWeak.
    assert.ok(strong.a >= 0.8, `${theme}: scrimStrong alpha ${strong.a} should be >= 0.8`);
    assert.ok(strong.a > weak.a, `${theme}: scrimStrong should be more opaque than scrimWeak`);

    // Computed, known value: composited over every app surface (bg0/bg1/bg2)
    // scrimStrong stays within a hair of opaque bg1's luminance, and body text
    // (inkPrimary) clears WCAG AA on the result. This is what lets the axe scans
    // check text against a computed background instead of a variable one.
    const bg1Luminance = relativeLuminance(palette.bg1);
    for (const backdrop of [palette.bg0, palette.bg1, palette.bg2]) {
      const surface = compositeOver(material.scrimStrong, backdrop);
      assert.ok(
        Math.abs(relativeLuminance(surface) - bg1Luminance) < 0.06,
        `${theme}: scrimStrong over ${backdrop} should stay near opaque bg1's luminance`,
      );
      const ratio = contrastRatio(palette.inkPrimary, surface);
      assert.ok(
        ratio >= AA_TEXT_MIN,
        `${theme}: text on scrimStrong over ${backdrop} is ${ratio.toFixed(2)}:1, needs ${AA_TEXT_MIN}:1`,
      );
    }
  }
});
