// Redacted filmstrip thumbnails for the flight recorder (docs/specs/design.md
// section 3: "A horizontal filmstrip of step thumbnails (redacted
// screenshots)"). The run viewer shows one thumbnail per step.
//
// A real run will one day carry an actual captured-then-redacted screenshot
// per step, but the bus (contracts/bus_events.md) carries no screenshot pixels
// today (the full snapshot "goes to the recorder, not the bus"), and this
// shell has no capture pipeline. So every frame the shell draws is a
// synthesized placeholder: a few "redaction bars" whose widths are a
// deterministic hash of the step id alone. design.md's requirement ("the point
// is no sensitive pixels ship") is met structurally here, not by scrubbing an
// image after the fact: there is no image data anywhere in this path to leak,
// only these generated bar widths, and they are derived from the step id, never
// from any step content. When a real redacted-capture pipeline lands, it
// replaces this module's output with an already-redacted image; the filmstrip
// view stays the same.

/**
 * A small, stable, non-cryptographic hash (FNV-1a, 32-bit). Deterministic so a
 * given step id always draws the same redaction pattern across re-renders,
 * which is what keeps a live run's earlier frames visually stable as new ones
 * stream in.
 */
function hashString(input: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** The narrowest and widest a redaction bar may be, as CSS percentages: never empty, never full-bleed, so every bar always reads as a blacked-out line. */
export const BAR_MIN_PCT = 45;
export const BAR_MAX_PCT = 92;

/**
 * Deterministic redaction-bar widths (as CSS percentages) for one thumbnail.
 * Derived only from `seed` (the step id) and the bar's own index, never from
 * any step content, so a placeholder can never encode anything sensitive. Each
 * width lands in [BAR_MIN_PCT, BAR_MAX_PCT].
 */
export function redactionBars(seed: string, count = 3): number[] {
  const span = BAR_MAX_PCT - BAR_MIN_PCT;
  const bars: number[] = [];
  for (let i = 0; i < count; i++) {
    const slice = hashString(`${seed}:${i}`);
    bars.push(BAR_MIN_PCT + (slice % (span + 1)));
  }
  return bars;
}
