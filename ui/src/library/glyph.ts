// Deterministic per-workflow glyph identity for the Library card grid
// (docs/specs/design.md section 3: "Each workflow gets an auto-assigned
// duotone glyph and hue (hash of the name into a fixed 12-hue ramp,
// overridable)"). Pure functions, no DOM: ./view.ts turns the { hueIndex,
// letter } this returns into the actual duotone badge markup.
//
// design.md section 2 is binding that every color in the app derives from
// ui/src/theme/tokens.ts, and scripts/check_rawhex.mjs enforces that with a
// literal-hex scan; that pair was scoped to the app's existing surface/ink/
// signal/semantic palette, not a brand new 12-color identity ramp. Rather
// than add 12 independent literals outside that scan's two exempt files,
// ./view.ts rotates the app's one signal color (tokens.ts's `signal`,
// ui/src/styles/tokens.css's --op-color-accent) by hueIndex * 30 degrees
// (12 * 30 = 360) with a CSS `filter: hue-rotate()`. Every glyph color is
// still, literally, derived from tokens.ts; this module only ever hands
// back an index and a rotation in degrees, never a color.

/** design.md: "a fixed 12-hue ramp." */
export const GLYPH_HUE_COUNT = 12;

/**
 * Small, deterministic, dependency-free string hash (FNV-1a), unsigned
 * 32-bit. Deterministic across processes and Node versions (unlike
 * Array.prototype.sort's engine-dependent tie-breaking or object key
 * iteration order), which is the one property this needs: the same
 * workflow name must hash to the same hue index every time, on every
 * machine, forever.
 */
function hashName(name: string): number {
  let hash = 0x811c9dc5; // FNV offset basis
  for (let i = 0; i < name.length; i++) {
    hash ^= name.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193); // FNV prime
  }
  return hash >>> 0;
}

export interface WorkflowGlyph {
  hueIndex: number;
  /** Degrees to rotate the base accent color's hue by (view.ts's CSS filter): hueIndex * (360 / GLYPH_HUE_COUNT). */
  hueRotationDeg: number;
  /** The glyph badge's monogram: first letter or digit of title (falling back to name), uppercased. */
  letter: string;
}

function wrapToRange(n: number, count: number): number {
  return ((Math.trunc(n) % count) + count) % count;
}

/**
 * name hashes to one of GLYPH_HUE_COUNT fixed slots; overrideHueIndex (any
 * integer, wrapped into range so a caller never has to pre-clamp it) takes
 * precedence when given, design.md's "overridable". No workflow manifest
 * field carries an override today (contracts/workflow_manifest.schema.json
 * is not this lane's to extend), so every real card currently takes the
 * hashed path; the parameter exists so a future manifest field is a
 * same-shape call-site change, not a new code path.
 */
export function assignGlyph(name: string, title: string, overrideHueIndex?: number): WorkflowGlyph {
  const hueIndex = overrideHueIndex === undefined ? hashName(name) % GLYPH_HUE_COUNT : wrapToRange(overrideHueIndex, GLYPH_HUE_COUNT);
  const source = title || name;
  // \p{L}/\p{N}, not [a-zA-Z0-9]: a workflow named entirely in another
  // script (or led by a symbol) still gets a real letter instead of always
  // falling back to "?".
  const match = /[\p{L}\p{N}]/u.exec(source);
  return {
    hueIndex,
    hueRotationDeg: Math.round(hueIndex * (360 / GLYPH_HUE_COUNT)),
    letter: (match?.[0] ?? "?").toUpperCase(),
  };
}
