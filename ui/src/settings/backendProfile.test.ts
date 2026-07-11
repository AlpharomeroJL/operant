import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describeBackendProfile, type BackendProfile } from "./backendProfile.ts";

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
