// A typed mirror of contracts/model_backend.md's BackendProfile (the probe
// result cached after configuring a model), the same "typed mirror of the
// wire shape" convention ui/src/bus/types.ts uses for contracts/bus_events.md.
// describeBackendProfile turns it into the plain-language explanation
// contracts/model_backend.md itself calls for ("role assignment ... explains
// mismatches in plain language") and docs/specs/backends.md shows an example
// of: "This model cannot see images, so it cannot find things on screen."

export interface BackendProfile {
  backend_id: string;
  vision: boolean;
  tool_use: boolean;
  context_length: number;
  streaming: boolean;
  probed_at: string;
}

// Rough plain-English size, not a real tokenizer: this is a Settings-screen
// approximation so a number like context_length reads as "about how much it
// can read at once" instead of a raw count of an internal unit.
const WORDS_PER_UNIT = 0.75;

function approxWords(contextLength: number): string {
  const rounded = Math.max(100, Math.round((contextLength * WORDS_PER_UNIT) / 100) * 100);
  return rounded.toLocaleString("en-US");
}

/**
 * D6 (docs/specs/design.md section 3.3's Settings > Thinking engines: "with
 * probe badges"). The same probe result describeBackendProfile explains in
 * full sentences, condensed to short at-a-glance labels for the Settings
 * sidebar's Thinking engines section (ui/src/settings/view.ts renders one
 * `.op-badge` per entry). Empty, not a placeholder badge, when nothing is
 * connected yet: describeBackendProfile's "No model connected yet." one-
 * liner already covers that case in prose immediately below the badges.
 */
export function backendProfileBadges(profile: BackendProfile | null | undefined): string[] {
  if (!profile) return [];
  const badges: string[] = [];
  if (profile.vision) badges.push("Sees the screen");
  if (profile.tool_use) badges.push("Takes actions");
  badges.push(`About ${approxWords(profile.context_length)} words`);
  badges.push(profile.streaming ? "Answers as it goes" : "Answers all at once");
  return badges;
}

/** Plain-language lines describing what a probed model can do. Never mentions the internal profile field names. */
export function describeBackendProfile(profile: BackendProfile | null | undefined): string[] {
  if (!profile) {
    return ["No model connected yet."];
  }
  return [
    profile.vision
      ? "It can see images, so it can find things on screen."
      : "It cannot see images, so it cannot find things on screen.",
    profile.tool_use ? "It can take actions on its own." : "It cannot take actions on its own; use it for planning only.",
    `It can read about ${approxWords(profile.context_length)} words at once.`,
    profile.streaming ? "It shows its answer as it goes." : "It shows its answer all at once, not as it goes.",
  ];
}
