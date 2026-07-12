// WCAG contrast-ratio math for the design tokens in ./tokens.css.
//
// axe-core's color-contrast rule cannot run reliably here: it samples
// rendered pixels via a canvas 2d context, and jsdom (this project's only
// available DOM for `node --test`, see ./testDomEnv.ts) has no canvas
// backend, so the rule always comes back "incomplete" rather than pass or
// fail. A token-level pairwise check does the same job at the level that
// actually matters: ui/src/styles/tokens.css is the single source of truth
// every screen paints from, so if a pair of tokens meets AA here, every
// screen that uses that pair meets AA too.
//
// Relative luminance and contrast ratio formulas: WCAG 2.1 section 1.4.3 /
// Appendix G (https://www.w3.org/TR/WCAG21/#dfn-relative-luminance).

/** Relative luminance of a `#rrggbb` color, 0 (black) to 1 (white). */
export function relativeLuminance(hex: string): number {
  const { r, g, b } = parseHex(hex);
  const [rl, gl, bl] = [r, g, b].map(channelToLinear);
  return 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
}

/** WCAG contrast ratio between two `#rrggbb` colors, 1 (no contrast) to 21 (black on white). */
export function contrastRatio(hexA: string, hexB: string): number {
  const lumA = relativeLuminance(hexA);
  const lumB = relativeLuminance(hexB);
  const lighter = Math.max(lumA, lumB);
  const darker = Math.min(lumA, lumB);
  return (lighter + 0.05) / (darker + 0.05);
}

/** WCAG AA for normal-weight body/label text: SC 1.4.3, 4.5:1. */
export const AA_TEXT_MIN = 4.5;
/** WCAG AA for large-scale text (18pt+/14pt+bold): SC 1.4.3, 3:1. */
export const AA_LARGE_TEXT_MIN = 3;
/** WCAG AA for the visual boundary of an interactive UI component: SC 1.4.11, 3:1. */
export const AA_NON_TEXT_MIN = 3;

function parseHex(hex: string): { r: number; g: number; b: number } {
  const clean = hex.replace("#", "");
  if (clean.length !== 6) throw new Error(`expected a #rrggbb color, got ${hex}`);
  return {
    r: parseInt(clean.slice(0, 2), 16),
    g: parseInt(clean.slice(2, 4), 16),
    b: parseInt(clean.slice(4, 6), 16),
  };
}

function channelToLinear(channel8bit: number): number {
  const c = channel8bit / 255;
  return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

/**
 * The design-token palette for one theme, mirroring ui/src/styles/tokens.css.
 * Kept as plain hex literals (not read from the .css file) so this stays a
 * pure, dependency-free computation; ./contrast.test.ts cross-checks these
 * literals against the actual tokens.css text so the two cannot drift apart
 * unnoticed.
 */
export interface ThemeTokens {
  bg: string;
  bgElevated: string;
  bgSunken: string;
  border: string;
  borderStrong: string;
  text: string;
  textMuted: string;
  accent: string;
  accentHover: string;
  accentText: string;
  statusIdle: string;
  statusRunning: string;
  statusHalted: string;
  statusWarning: string;
  focusRing: string;
}

export const LIGHT_TOKENS: ThemeTokens = {
  bg: "#f7f7f8",
  bgElevated: "#ffffff",
  bgSunken: "#ececee",
  border: "#d8d8dd",
  borderStrong: "#86868f",
  text: "#17171a",
  textMuted: "#55555f",
  accent: "#2f5fed",
  accentHover: "#2249c4",
  accentText: "#ffffff",
  statusIdle: "#6b6b76",
  statusRunning: "#1f8f5f",
  statusHalted: "#d1293d",
  statusWarning: "#b7791f",
  focusRing: "#2f5fed",
};

export const DARK_TOKENS: ThemeTokens = {
  bg: "#17171a",
  bgElevated: "#201f24",
  bgSunken: "#101012",
  border: "#33333a",
  borderStrong: "#6b6b73",
  text: "#f1f1f3",
  textMuted: "#a7a7b3",
  accent: "#7fa1ff",
  accentHover: "#9db6ff",
  accentText: "#0d1330",
  statusIdle: "#9494a1",
  statusRunning: "#3fcf8e",
  statusHalted: "#ff6b7a",
  statusWarning: "#e0ac47",
  focusRing: "#7fa1ff",
};
