// Every fixture and cookbook workflow renders with ZERO glossary internal terms
// and no raw JSON. The glossary matcher is the same word-boundary, case-
// insensitive one scripts/microcopy_lint.mjs uses, read from the real contract
// so it stays correct as the glossary grows. Scans everything the renderer emits
// for a workflow: grant prose, title/summary, every step sentence, and every
// input field label and value.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { renderWorkflow, renderDriftOffer } from "../src/render/index.js";
import { allWorkflows } from "../src/render/examples.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..", "..");
const glossary = JSON.parse(readFileSync(join(repoRoot, "contracts", "microcopy_glossary.json"), "utf8"));

const matchers = glossary.terms.map((t) => ({
  term: t.internal,
  re: new RegExp(`\\b${t.internal.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`, "i"),
}));

function glossaryHits(str) {
  return matchers.filter((m) => m.re.test(str)).map((m) => m.term);
}

/** Collect every human-facing string the renderer produces for a workflow. */
function renderedStrings(rendered) {
  const out = [rendered.title, rendered.summary, rendered.grant];
  for (const f of rendered.inputs) {
    out.push(f.label, f.value);
  }
  for (const s of rendered.steps) {
    out.push(s.sentence);
    for (const p of s.parts) out.push(p.t === "text" ? p.text : p.value);
  }
  return out.filter((s) => typeof s === "string" && s.length);
}

for (const wf of allWorkflows) {
  test(`"${wf.slug}" renders with no jargon and no raw JSON`, () => {
    const rendered = renderWorkflow(wf.manifest, wf.steps);

    // Numbered steps, each a non-empty sentence.
    assert.ok(rendered.steps.length > 0, "workflow should have steps");
    rendered.steps.forEach((s, i) => {
      assert.equal(s.n, i + 1, "steps are numbered in order");
      assert.ok(s.sentence.length > 0, `step ${s.n} should render a sentence`);
    });

    for (const str of renderedStrings(rendered)) {
      assert.ok(!/[{}]/.test(str), `raw JSON braces in "${wf.slug}": ${str}`);
      assert.ok(!str.includes("[object Object]"), `stringified object in "${wf.slug}": ${str}`);
      const hits = glossaryHits(str);
      assert.equal(hits.length, 0, `internal term(s) [${hits.join(", ")}] in "${wf.slug}": ${str}`);
    }
  });
}

test("drift offers across common controls carry no jargon", () => {
  for (const element of ["Save button", "Submit button", "Compose window", "search box"]) {
    const offer = renderDriftOffer({ element });
    for (const str of [offer.headline, offer.question, offer.text, offer.accept, offer.dismiss]) {
      assert.ok(!/[{}]/.test(str), `raw JSON braces: ${str}`);
      assert.equal(glossaryHits(str).length, 0, `internal term in drift offer: ${str}`);
    }
  }
});

test("the whole cookbook is covered (ten workflows plus the notepad fixture)", () => {
  assert.equal(allWorkflows.length, 11);
});
