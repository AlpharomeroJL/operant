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

import { DARK_COLORS, LIGHT_COLORS, type ColorRoles } from "../theme/tokens.ts";

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
 * A narrow view of ui/src/theme/tokens.ts's fuller ColorRoles: exactly the
 * fields this file's pairwise AA checks (below and in ./contrast.test.ts)
 * exercise, so contrast.test.ts's tokens.css cross-check keeps comparing the
 * same shape it always has. tokens.ts is still the single source of truth
 * for the values themselves; this is just a projection of it.
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
  statusDone: string;
  focusRing: string;
}

function projectThemeTokens(colors: ColorRoles): ThemeTokens {
  const {
    bg,
    bgElevated,
    bgSunken,
    border,
    borderStrong,
    text,
    textMuted,
    accent,
    accentHover,
    accentText,
    statusIdle,
    statusRunning,
    statusHalted,
    statusWarning,
    statusDone,
    focusRing,
  } = colors;
  return {
    bg,
    bgElevated,
    bgSunken,
    border,
    borderStrong,
    text,
    textMuted,
    accent,
    accentHover,
    accentText,
    statusIdle,
    statusRunning,
    statusHalted,
    statusWarning,
    statusDone,
    focusRing,
  };
}

export const LIGHT_TOKENS: ThemeTokens = projectThemeTokens(LIGHT_COLORS);
export const DARK_TOKENS: ThemeTokens = projectThemeTokens(DARK_COLORS);
