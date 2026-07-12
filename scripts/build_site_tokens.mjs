// Token-export helper for the docs site (site/). Reads the same single
// source of truth the app itself reads, ui/src/theme/tokens.ts
// (docs/specs/design.md section 2, BINDING), and writes site/tokens.css: a
// generated CSS custom property mirror that the plain static site's own
// hand-written style.css paints from. This is the same relationship
// ui/scripts/build-tokens.mjs already has with ui/src/styles/tokens.css,
// just for the docs site instead of the app shell.
//
// Read-only: this does not modify ui/src/theme/tokens.ts, and does not add a
// build step to the app itself. Run it standalone with
// `node scripts/build_site_tokens.mjs`, or let `just site` run it for you as
// the first step of staging the deployable site.
//
// No color value is invented here: every hex literal below is read straight
// out of DARK_PALETTE/LIGHT_PALETTE. The one derived value (disabled ink) is
// the same "45 percent of secondary" formula design.md section 2 states and
// tokens.ts's own withAlpha() already implements, just recomputed here so
// this script does not need to import a private helper.
//
// GENERATED FILE note: site/tokens.css itself carries a "do not hand-edit"
// banner; this script is the only thing that writes it.

import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { DARK_PALETTE, LIGHT_PALETTE, FONT, SPACE, RADIUS, MOTION } from "../ui/src/theme/tokens.ts";

const outPath = fileURLToPath(new URL("../site/tokens.css", import.meta.url));

/** `#rrggbb` plus an alpha fraction. Mirrors tokens.ts's own withAlpha(). */
function withAlpha(hex, alpha) {
  const clean = hex.replace("#", "");
  const r = parseInt(clean.slice(0, 2), 16);
  const g = parseInt(clean.slice(2, 4), 16);
  const b = parseInt(clean.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

// [cssVarSuffix, DesignPalette field] pairs, in design.md section 2's own
// grouping and order: Surfaces, Ink, Signal, Semantic.
const PALETTE_VARS = [
  ["bg0", "bg0"],
  ["bg1", "bg1"],
  ["bg2", "bg2"],
  ["hairline", "hairline"],
  ["ink", "inkPrimary"],
  ["ink-secondary", "inkSecondary"],
  ["signal", "signal"],
  ["signal-hover", "signalHover"],
  ["on-signal-ink", "onSignalInk"],
  ["success", "success"],
  ["danger", "danger"],
  ["info", "info"],
];

function paletteLines(palette) {
  const lines = PALETTE_VARS.map(([suffix, field]) => `  --op-${suffix}: ${palette[field]};`);
  lines.push(`  --op-ink-disabled: ${withAlpha(palette.inkSecondary, 0.45)};`);
  return lines.join("\n");
}

function typeLines() {
  const lines = [];
  lines.push(`  --op-font-family: ${FONT.family};`);
  lines.push(`  --op-font-family-mono: ${FONT.familyMono};`);
  for (const [step, value] of Object.entries(FONT.size)) lines.push(`  --op-font-size-${step}: ${value};`);
  for (const [step, value] of Object.entries(FONT.lineHeight)) lines.push(`  --op-line-height-${step}: ${value};`);
  for (const [step, value] of Object.entries(FONT.weight)) lines.push(`  --op-font-weight-${step}: ${value};`);
  return lines.join("\n");
}

function spaceLines() {
  return Object.entries(SPACE)
    .map(([step, value]) => `  --op-space-${step}: ${value};`)
    .join("\n");
}

function shapeLines() {
  return [
    `  --op-radius-control: ${RADIUS.control};`,
    `  --op-radius-card: ${RADIUS.card};`,
    `  --op-radius-pill: ${RADIUS.pill};`,
  ].join("\n");
}

function motionLines() {
  return [
    `  --op-motion-fast: ${MOTION.fast};`,
    `  --op-motion-standard: ${MOTION.standard};`,
    `  --op-motion-easing: ${MOTION.easing};`,
  ].join("\n");
}

const banner = `/*
 * GENERATED FILE. Do not hand-edit.
 *
 * Docs-site design tokens, exported from ui/src/theme/tokens.ts (the single
 * source of truth for every color and size in Operant, docs/specs/design.md
 * section 2, BINDING) by scripts/build_site_tokens.mjs. Re-run
 * \`node scripts/build_site_tokens.mjs\` (or \`just site\`, which does that for
 * you) after editing tokens.ts; nothing here is invented by hand.
 *
 * Dark is the default theme (design.md section 1: instrument calm, one warm
 * signal color, "Dark theme (the default)"): the bare :root block below is
 * dark, with a prefers-color-scheme: light override for a system-light OS.
 * site/style.css paints from these --op-* custom properties and should never
 * need to repeat a literal hex.
 *
 * Neither Instrument Sans nor IBM Plex Mono ships as a vendored file in this
 * repo yet (see tokens.ts's own FONT comment for the open sub-item), and this
 * site is offline-friendly by design: no remote font fetch. So
 * --op-font-family(-mono) below fall back to the closest already-installed
 * system faces, same as the app.
 */`;

const css = `${banner}

:root {
${paletteLines(DARK_PALETTE)}

${typeLines()}

${spaceLines()}

${shapeLines()}

${motionLines()}
}

@media (prefers-color-scheme: light) {
  :root {
${paletteLines(LIGHT_PALETTE)}
  }
}
`;

export function buildSiteTokens() {
  writeFileSync(outPath, css, "utf8");
  console.log(`build_site_tokens: wrote ${outPath}`);
  return outPath;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  buildSiteTokens();
}
