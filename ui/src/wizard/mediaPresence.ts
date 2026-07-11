// The media-presence check (C19 bar: "every wizard screen must have visible
// content (and audible where applicable)"): a regression guard for the
// silent-wizard failure class, where a screen mounts but every string
// binding it needed came through empty (a wiring bug, not a copy bug, since
// scripts/microcopy_lint.mjs already guarantees the copy itself is never
// jargon; it says nothing about whether a string actually reached the
// screen).
//
// Pure: operates on the content a screen snapshot declares it is showing,
// not on rendered DOM (this project has no jsdom), so it runs under plain
// `node --test` the same as every state module in ui/src. ui/src/wizard/state.ts
// computes a ScreenContent for whichever screen is active from the exact
// same fields ui/src/wizard/view.ts renders, so a future change that blanks
// out a binding at the state layer is what this actually catches; hand
// duplicating the strings here would only catch a typo in this file.

export interface ScreenContent {
  screen: string;
  /** Every visible string this screen renders right now (headings, body copy, button labels, live status lines). */
  visible: string[];
  /** Set only on a screen where an audible cue is part of the design (the mic check's sample). Absent elsewhere. */
  audible?: {
    /** The label of the affordance that produces the audible cue (e.g. "Play a sample"). Blank means the affordance did not actually wire up: the literal silent-wizard bug. */
    cueLabel: string;
  };
}

export interface MediaPresenceResult {
  screen: string;
  ok: boolean;
  /** Empty when ok. Otherwise, one entry per thing that was missing. */
  reasons: string[];
}

/** True for a real, non-whitespace string. */
function present(s: string | undefined | null): boolean {
  return typeof s === "string" && s.trim().length > 0;
}

/**
 * Checks one screen's declared content. A screen fails if it has no visible
 * text at all, or if it declares an audible cue whose label is blank.
 */
export function checkMediaPresence(content: ScreenContent): MediaPresenceResult {
  const reasons: string[] = [];

  const visibleCount = content.visible.filter(present).length;
  if (visibleCount === 0) {
    reasons.push("no visible content");
  }

  if (content.audible && !present(content.audible.cueLabel)) {
    reasons.push("audible cue missing its label");
  }

  return { screen: content.screen, ok: reasons.length === 0, reasons };
}

export function checkAllScreens(contents: readonly ScreenContent[]): MediaPresenceResult[] {
  return contents.map(checkMediaPresence);
}

/** Convenience for a test: throws with every failing screen named, so a failure is legible without stepping through results by hand. */
export function assertMediaPresence(contents: readonly ScreenContent[]): void {
  const results = checkAllScreens(contents);
  const failed = results.filter((r) => !r.ok);
  if (failed.length > 0) {
    const detail = failed.map((r) => `${r.screen}: ${r.reasons.join(", ")}`).join("; ");
    throw new Error(`media-presence check failed: ${detail}`);
  }
}
