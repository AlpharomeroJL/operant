// Generates ui/src/styles/tokens.css from ui/src/theme/tokens.ts, the single
// source of truth for every color and size in the Operant shell (docs/specs/
// design.md section 2). Run via `npm run build:tokens`, or automatically
// before `npm test` / `npm run dev` / `npm run build` (see package.json's
// pretest/predev/prebuild hooks) and by `just ui`, so tokens.css can never
// quietly drift from tokens.ts: this script is the only thing that writes it.
//
// Do not hand-edit ui/src/styles/tokens.css: it is regenerated from scratch
// every run. Edit ui/src/theme/tokens.ts and re-run this script instead.

import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  DARK_COLORS,
  LIGHT_COLORS,
  SHADOW_ROLES,
  SPACE,
  RADIUS,
  FONT,
  MOTION,
} from "../src/theme/tokens.ts";

const outPath = fileURLToPath(new URL("../src/styles/tokens.css", import.meta.url));

// [cssVarSuffix, ColorRoles field] pairs, in the order they render. Kept as
// one ordered list so the color block and the non-color block below read in
// a stable, reviewable order every regeneration.
const COLOR_VARS = [
  ["bg", "bg"],
  ["bg-elevated", "bgElevated"],
  ["bg-sunken", "bgSunken"],
  ["border", "border"],
  ["border-strong", "borderStrong"],
  ["text", "text"],
  ["text-muted", "textMuted"],
  ["text-disabled", "textDisabled"],
  ["text-inverse", "textInverse"],
  ["accent", "accent"],
  ["accent-hover", "accentHover"],
  ["accent-text", "accentText"],
  ["status-idle", "statusIdle"],
  ["status-running", "statusRunning"],
  ["status-halted", "statusHalted"],
  ["status-warning", "statusWarning"],
  ["status-done", "statusDone"],
  ["focus-ring", "focusRing"],
  ["success", "success"],
  ["danger", "danger"],
  ["info", "info"],
  ["scrim", "scrim"],
];

function colorLines(colors, shadow) {
  const lines = COLOR_VARS.map(([suffix, field]) => `  --op-color-${suffix}: ${colors[field]};`);
  lines.push(`  --op-shadow-popover: ${shadow.popover};`);
  lines.push(`  --op-shadow-modal: ${shadow.modal};`);
  return lines.join("\n");
}

function nonColorLines() {
  const lines = [];
  lines.push("  /* Spacing scale, 4px base (design.md section 2). */");
  for (const [step, value] of Object.entries(SPACE)) lines.push(`  --op-space-${step}: ${value};`);
  lines.push("");
  lines.push("  /* Type (design.md section 2): see ui/src/theme/tokens.ts's FONT for the bundling note. */");
  lines.push(`  --op-font-family: ${FONT.family};`);
  lines.push(`  --op-font-family-mono: ${FONT.familyMono};`);
  for (const [step, value] of Object.entries(FONT.size)) lines.push(`  --op-font-size-${step}: ${value};`);
  for (const [step, value] of Object.entries(FONT.lineHeight)) lines.push(`  --op-line-height-${step}: ${value};`);
  for (const [step, value] of Object.entries(FONT.weight)) lines.push(`  --op-font-weight-${step}: ${value};`);
  lines.push("");
  lines.push("  /* Shape (design.md section 2). */");
  lines.push(`  --op-radius-control: ${RADIUS.control};`);
  lines.push(`  --op-radius-card: ${RADIUS.card};`);
  lines.push(`  --op-radius-pill: ${RADIUS.pill};`);
  // Back-compat aliases: base.css/wizard.css predate the card/control naming
  // and reference --op-radius-md (controls) / --op-radius-lg (cards).
  lines.push(`  --op-radius-md: ${RADIUS.control};`);
  lines.push(`  --op-radius-lg: ${RADIUS.card};`);
  lines.push("");
  lines.push("  /* Motion (design.md section 2). prefers-reduced-motion is handled globally in base.css. */");
  lines.push(`  --op-motion-fast: ${MOTION.fast};`);
  lines.push(`  --op-motion-base: ${MOTION.standard};`);
  lines.push(`  --op-motion-easing: ${MOTION.easing};`);
  return lines.join("\n");
}

const banner = `/*
 * GENERATED FILE. Do not hand-edit.
 *
 * Design tokens: color, spacing, type, radius, and motion scale for the
 * Operant shell (docs/specs/design.md section 2, BINDING). Produced by
 * ui/scripts/build-tokens.mjs from ui/src/theme/tokens.ts, the single source
 * of truth; re-run \`npm run build:tokens\` (or \`npm test\` / \`npm run dev\` /
 * \`npm run build\`, which do it for you) after editing tokens.ts.
 *
 * Dark is the default theme (design.md section 1): the bare :root block
 * below is dark, with a prefers-color-scheme: light override for a
 * system-light OS, and both are also reachable directly via an explicit
 * data-theme attribute (ui/src/theme/store.ts) that wins over the OS
 * preference in either direction.
 */`;

const css = `${banner}

:root {
${colorLines(DARK_COLORS, SHADOW_ROLES.dark)}

${nonColorLines()}
}

@media (prefers-color-scheme: light) {
  :root {
${colorLines(LIGHT_COLORS, SHADOW_ROLES.light)}
  }
}

/* Explicit overrides win over the OS preference in either direction. */
:root[data-theme="dark"] {
${colorLines(DARK_COLORS, SHADOW_ROLES.dark)}
}

:root[data-theme="light"] {
${colorLines(LIGHT_COLORS, SHADOW_ROLES.light)}
}
`;

writeFileSync(outPath, css, "utf8");
console.log(`build-tokens: wrote ${outPath}`);
