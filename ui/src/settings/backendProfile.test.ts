import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describeBackendProfile, backendProfileBadges, type BackendProfile } from "./backendProfile.ts";

// Read the real glossary rather than typing internal terms as literals here
// (the same reason ui/src/__tests__/microcopy-lint.test.ts reads its bait
// term from the glossary file at run time instead of hardcoding one): it
// stays correct if the glossary's term list changes, and it keeps this file
// itself free of raw internal terms that scripts/microcopy_lint.mjs would
// otherwise flag.
const here = dirname(fileURLToPath(import.meta.url));
const glossaryPath = join(here, "..", "..", "..", "contracts", "microcopy_glossary.json");
const glossary = JSON.parse(readFileSync(glossaryPath, "utf8")) as { terms: Array<{ internal: string; user_facing: string }> };
const GLOSSARY_TERMS = glossary.terms.map((t) => t.internal);

function assertJargonFree(lines: string[]): void {
  for (const line of lines) {
    for (const term of GLOSSARY_TERMS) {
      assert.ok(!new RegExp(`\\b${term}\\b`, "i").test(line), `"${line}" must not use the word "${term}"`);
    }
  }
}

test("no profile yet reads as a plain one-liner", () => {
  const lines = describeBackendProfile(null);
  assert.deepEqual(lines, ["No model connected yet."]);
});

test("a vision-capable, tool-using, streaming model describes itself plainly", () => {
  const profile: BackendProfile = {
    backend_id: "anthropic",
    vision: true,
    tool_use: true,
    context_length: 32768,
    streaming: true,
    probed_at: "2026-07-11T00:00:00Z",
  };
  const lines = describeBackendProfile(profile);
  assert.equal(lines.length, 4);
  assert.match(lines[0], /can see images/);
  assert.match(lines[1], /can take actions on its own/);
  assert.match(lines[2], /words at once/);
  assert.match(lines[3], /as it goes/);
  assertJargonFree(lines);
});

test("a text-only, non-tool, non-streaming model explains its limits in plain language", () => {
  const profile: BackendProfile = {
    backend_id: "local-text",
    vision: false,
    tool_use: false,
    context_length: 8192,
    streaming: false,
    probed_at: "2026-07-11T00:00:00Z",
  };
  const lines = describeBackendProfile(profile);
  assert.match(lines[0], /cannot see images/);
  assert.match(lines[1], /cannot take actions on its own/);
  assert.match(lines[3], /all at once/);
  assertJargonFree(lines);
});

test("context length renders as an approximate word count, never the raw number's own unit", () => {
  const profile: BackendProfile = {
    backend_id: "x",
    vision: false,
    tool_use: false,
    context_length: 128000,
    streaming: false,
    probed_at: "2026-07-11T00:00:00Z",
  };
  const [, , sizeLine] = describeBackendProfile(profile);
  assert.match(sizeLine, /^It can read about [\d,]+ words at once\.$/);
});

// D6 (docs/specs/design.md section 3.3): "probe badges", the Settings
// screen's short at-a-glance labels for the same probe result the four
// describeBackendProfile lines above already explain in full sentences.

test("no profile yet: no badges (the plain one-liner above already covers this case)", () => {
  assert.deepEqual(backendProfileBadges(null), []);
});

test("a vision-capable, tool-using, streaming model gets all four badges, jargon-free", () => {
  const profile: BackendProfile = {
    backend_id: "anthropic",
    vision: true,
    tool_use: true,
    context_length: 32768,
    streaming: true,
    probed_at: "2026-07-11T00:00:00Z",
  };
  const badges = backendProfileBadges(profile);
  assert.equal(badges.length, 4);
  assert.match(badges[0], /Sees the screen/);
  assert.match(badges[1], /Takes actions/);
  assert.match(badges[2], /words/);
  assert.match(badges[3], /Answers as it goes/);
  assertJargonFree(badges);
});

test("a text-only, non-tool, non-streaming model gets only the two badges that always apply", () => {
  const profile: BackendProfile = {
    backend_id: "local-text",
    vision: false,
    tool_use: false,
    context_length: 8192,
    streaming: false,
    probed_at: "2026-07-11T00:00:00Z",
  };
  const badges = backendProfileBadges(profile);
  assert.deepEqual(badges, ["About 6,100 words", "Answers all at once"]);
  assertJargonFree(badges);
});
