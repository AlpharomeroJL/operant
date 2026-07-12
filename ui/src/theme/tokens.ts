// The single source of truth for every color and size token in the Operant
// shell (docs/specs/design.md section 2, BINDING: "Every color and size in
// the app derives from ui/src/theme/tokens.ts; a lint forbids raw hex
// anywhere else").
//
// Two things are generated from the values in this file so they can never
// quietly drift from it:
//   - ui/src/styles/tokens.css (ui/scripts/build-tokens.mjs, run via the
//     npm pretest/predev/prebuild hooks and `just ui`) is the CSS custom
//     property mirror every screen's stylesheet paints from.
//   - ui/src/styles/contrast.ts imports the same resolved values for its
//     WCAG contrast-ratio math (ui/src/styles/contrast.test.ts).
// scripts/check_rawhex.mjs (repo root) forbids a raw hex color literal
// anywhere under ui/src except this file and the generated tokens.css.

export type ThemeName = "dark" | "light";

/**
 * design.md section 2's palette, verbatim, one field per bullet. No derived
 * or invented values live here: if a color is not literally in design.md,
 * it does not belong in *_PALETTE (see ColorRoles below for the small set
 * of derived roles this packet had to define on top of the palette).
 */
export interface DesignPalette {
  /** Window on Mica. */
  bg0: string;
  /** Cards. */
  bg1: string;
  /** Raised. */
  bg2: string;
  hairline: string;
  inkPrimary: string;
  inkSecondary: string;
  /** The identity color: recording, active state, the primary call to action. Never a large fill. */
  signal: string;
  signalHover: string;
  onSignalInk: string;
  success: string;
  /** Reserved: kill switch, destructive actions. */
  danger: string;
  info: string;
}

// Hex literals below are lowercase throughout (design.md itself writes them
// uppercase; case is not meaningful in CSS hex colors, and the codebase's
// existing convention, including the CSS-vs-TS cross-check in
// contrast.test.ts, is lowercase, so this is what that check expects).
export const DARK_PALETTE: DesignPalette = {
  bg0: "#101114",
  bg1: "#17181c",
  bg2: "#1e2026",
  hairline: "#2a2d34",
  inkPrimary: "#ecedef",
  inkSecondary: "#9da2ab",
  signal: "#e8a13c",
  signalHover: "#f2b355",
  onSignalInk: "#1a1204",
  success: "#3fb27f",
  danger: "#e5484d",
  info: "#5b8def",
};

export const LIGHT_PALETTE: DesignPalette = {
  bg0: "#f7f7f5",
  bg1: "#ffffff",
  bg2: "#f1f1ee",
  hairline: "#e3e3de",
  inkPrimary: "#1b1c1e",
  inkSecondary: "#5c5f66",
  // design.md section 2 lists Signal and Semantic as a single value each,
  // not a light/dark pair (unlike Surfaces and Ink, which are listed
  // separately per theme): the identity color and success/danger/info read
  // the same regardless of theme.
  signal: DARK_PALETTE.signal,
  signalHover: DARK_PALETTE.signalHover,
  onSignalInk: DARK_PALETTE.onSignalInk,
  success: DARK_PALETTE.success,
  danger: DARK_PALETTE.danger,
  info: DARK_PALETTE.info,
};

export const PALETTES: Record<ThemeName, DesignPalette> = {
  dark: DARK_PALETTE,
  light: LIGHT_PALETTE,
};

/**
 * WCAG contrast-ratio math constants: not design tokens, just the literal
 * true-black/true-white RGB endpoints ui/src/styles/contrast.test.ts uses to
 * verify the contrast-ratio formula itself (a 21:1 ceiling has to be checked
 * against real black-on-white, not a token, since no surface or ink token in
 * the palette above is pure black). Defined once here, the one file
 * scripts/check_rawhex.mjs exempts, so contrast.ts and contrast.test.ts
 * never need a raw hex literal of their own.
 */
export const PURE_BLACK = "#000000";
export const PURE_WHITE = "#ffffff";

/** `#rrggbb` plus an alpha fraction, e.g. for "45 percent of secondary" (design.md's disabled ink). */
function withAlpha(hex: string, alpha: number): string {
  const clean = hex.replace("#", "");
  const r = parseInt(clean.slice(0, 2), 16);
  const g = parseInt(clean.slice(2, 4), 16);
  const b = parseInt(clean.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/**
 * Every color role a screen actually paints with, resolved from the palette
 * above. Most roles are a 1:1 rename of a palette value (bg -> bg0, text ->
 * inkPrimary, ...); a handful are this packet's own documented derivation,
 * since design.md fixes the palette but does not enumerate every CSS role:
 *
 * - `borderStrong` (the WCAG 1.4.11 3:1 boundary for inputs/buttons/toggles,
 *   ui/src/styles/contrast.test.ts's nonTextPairs): the plain hairline is
 *   deliberately too quiet for this (see contrast.test.ts's own comment on
 *   why the plain `border` token is exempt as decorative); `inkSecondary`
 *   already has to read clearly as muted text, so it is reused here rather
 *   than inventing a color outside the palette, and it clears 3:1 against
 *   every surface in both themes (contrast.test.ts).
 * - `focusRing`: kept off `signal` deliberately. design.md section 1's
 *   thesis is one warm color reserved for recording/active/primary-CTA;
 *   painting every keyboard focus outline amber would spend that color on
 *   the single most frequent piece of chrome in the app. `info` is neutral,
 *   unclaimed by any other role, and clears 3:1 in both themes.
 * - `statusRunning` / `statusDone`: pre-redesign, "ok" and "done" steps
 *   reused the same green as a live "running" state. design.md section 3's
 *   Flight recorder text draws that exact distinction ("Replay is the calm
 *   default; explore is the amber exception... determinism looks quiet"), so
 *   this packet splits them: `statusRunning` (actively recording/in
 *   progress) is the signal color, `statusDone` (finished, successfully) is
 *   `success`. Both are still ink-quiet at `statusIdle`.
 * - `statusWarning` (paused / retried): design.md's palette has no separate
 *   warning hue. `info` reads as "needs a moment, not urgent" without
 *   overloading `signal`'s narrower meaning.
 *
 * Status/glyph dot colors (`statusIdle/Running/Halted/Warning/Done`) are
 * deliberately NOT part of contrast.test.ts's strict WCAG 1.4.11 battery;
 * see that file for why (every dot in this app ships with a redundant,
 * programmatically associated text equivalent, so the dot's exact color is
 * never the sole carrier of the state).
 */
export interface ColorRoles {
  bg: string;
  bgElevated: string;
  bgSunken: string;
  border: string;
  borderStrong: string;
  text: string;
  textMuted: string;
  /** design.md: "disabled: 45 percent of secondary". WCAG contrast does not apply to disabled controls. */
  textDisabled: string;
  textInverse: string;
  accent: string;
  accentHover: string;
  accentText: string;
  statusIdle: string;
  statusRunning: string;
  statusHalted: string;
  statusWarning: string;
  statusDone: string;
  focusRing: string;
  success: string;
  danger: string;
  info: string;
  /** Modal backdrop dimming (docs/specs/design.md doesn't specify one, so this packet's own derivation): a fixed near-black scrim reads as "receded" in both themes, the same way a movie theater dims regardless of the house lights' own color. */
  scrim: string;
}

function resolveRoles(theme: ThemeName): ColorRoles {
  const p = PALETTES[theme];
  const other = PALETTES[theme === "dark" ? "light" : "dark"];
  return {
    bg: p.bg0,
    bgElevated: p.bg1,
    bgSunken: p.bg2,
    border: p.hairline,
    borderStrong: p.inkSecondary,
    text: p.inkPrimary,
    textMuted: p.inkSecondary,
    textDisabled: withAlpha(p.inkSecondary, 0.45),
    // The color legible if the surface underneath flipped to the other theme.
    textInverse: other.inkPrimary,
    accent: p.signal,
    accentHover: p.signalHover,
    accentText: p.onSignalInk,
    statusIdle: p.inkSecondary,
    statusRunning: p.signal,
    statusHalted: p.danger,
    statusWarning: p.info,
    statusDone: p.success,
    scrim: "rgba(20, 20, 25, 0.4)",
    focusRing: p.info,
    success: p.success,
    danger: p.danger,
    info: p.info,
  };
}

export const DARK_COLORS: ColorRoles = resolveRoles("dark");
export const LIGHT_COLORS: ColorRoles = resolveRoles("light");

export const COLOR_ROLES: Record<ThemeName, ColorRoles> = {
  dark: DARK_COLORS,
  light: LIGHT_COLORS,
};

/** design.md: "Shadows minimal, one level; the dark theme uses hairlines instead of shadows." */
export interface ShadowRoles {
  popover: string;
  modal: string;
}

export const SHADOW_ROLES: Record<ThemeName, ShadowRoles> = {
  dark: { popover: "none", modal: "none" },
  light: {
    popover: "0 4px 16px rgba(20, 20, 25, 0.16)",
    modal: "0 12px 40px rgba(20, 20, 25, 0.24)",
  },
};

/** design.md: "Spacing on a 4px grid." Theme-invariant. */
export const SPACE = {
  1: "4px",
  2: "8px",
  3: "12px",
  4: "16px",
  5: "24px",
  6: "32px",
  7: "48px",
  8: "64px",
} as const;

/** design.md: "Radius: 8 (cards), 6 (controls), full (pills)." */
export const RADIUS = {
  control: "6px",
  card: "8px",
  pill: "999px",
} as const;

/**
 * design.md: "UI and display face: Instrument Sans (bundled, variable)."
 * "Numeric and step data: IBM Plex Mono, tabular figures..."
 * "No font ships that is not bundled."
 *
 * OPEN SUB-ITEM (flagged, not silently skipped): neither font's files are
 * vendored in this repo yet, and this app is deliberately air-gapped (no
 * network font fetch: see the removed tokens.css comment this replaces, and
 * scripts/check_airgap.mjs), so a remote webfont `<link>`/`@import` is not an
 * option either. Until a later packet vendors the actual font files, both
 * stacks below fall back to the closest already-installed system faces
 * (Segoe UI on Windows for the UI face, Cascadia Code/Consolas for mono) so
 * the app never ships an unbundled *network* font while staying legible.
 */
export const FONT = {
  family: `"Instrument Sans", -apple-system, "Segoe UI", system-ui, Roboto, Helvetica, Arial, sans-serif`,
  familyMono: `"IBM Plex Mono", ui-monospace, "Cascadia Code", Consolas, monospace`,
  // design.md: "Scale: 12 / 13 / 15 body, 17 section, 22 title, 28 dashboard hero."
  size: {
    xs: "0.75rem", // 12px
    sm: "0.8125rem", // 13px
    base: "0.9375rem", // 15px
    md: "1.0625rem", // 17px, section
    lg: "1.375rem", // 22px, title
    xl: "1.75rem", // 28px, dashboard hero
  },
  lineHeight: {
    tight: 1.25,
    base: 1.5,
  },
  weight: {
    regular: 400,
    medium: 500,
    semibold: 600,
  },
} as const;

/** design.md: "Motion: 160ms cubic-bezier(0.2, 0, 0, 1) standard." */
export const MOTION = {
  fast: "100ms",
  standard: "160ms",
  easing: "cubic-bezier(0.2, 0, 0, 1)",
} as const;
