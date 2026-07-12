// Fails CI if a default-mode UI string contains a glossary internal term.
// Scans ui/src default-mode string catalogs. Advanced-mode files (marked with a
// `// @advanced` header or under an `advanced/` dir) are exempt. Passes vacuously
// until U1A adds the UI, which is correct: no strings, no violations.
import { readFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const glossary = JSON.parse(readFileSync("contracts/microcopy_glossary.json", "utf8"));
const terms = glossary.terms.map((t) => t.internal);

// Word-boundary, case-insensitive matcher per term.
const matchers = terms.map((t) => ({
  term: t,
  re: new RegExp(`\\b${t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`, "i"),
}));

// Phrases docs/specs/design.md deliberately sanctions in default mode even
// though they contain a glossary term. design.md is binding and wins until
// amended: section 4 fixes the replay chip copy verbatim and explicitly allows
// replay copy to say it uses no AI. Kept as an exact-phrase allowlist (scrubbed
// before matching) so every OTHER jargon leak is still caught.
const SANCTIONED = [
  "no AI, exact replay",
];
const sanctionedRes = SANCTIONED.map(
  (s) => new RegExp(s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"),
);

const SCAN_DIRS = ["ui/src", "ui/src-tauri/locales"];
const LOCALE_DIRS = ["ui/src/locales"];
const STRING_EXT = /\.(ts|tsx|js|jsx|json|svelte|vue|html)$/i;

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const s = statSync(p);
    if (s.isDirectory()) {
      if (name === "node_modules" || name === "advanced") continue;
      walk(p, out);
    } else if (STRING_EXT.test(name)) {
      out.push(p);
    }
  }
  return out;
}

let hits = [];
let scanned = 0;
for (const dir of SCAN_DIRS) {
  for (const f of walk(dir)) {
    const text = readFileSync(f, "utf8");
    if (text.includes("@advanced")) continue;
    scanned++;
    // Only inspect quoted string literals to avoid flagging identifiers/imports.
    const strings = text.match(/(["'`])(?:\\.|(?!\1).)*\1/g) || [];
    for (const lit of strings) {
      // Remove sanctioned phrases before matching, so their embedded term does
      // not trip the lint while any other jargon in the same literal still does.
      let scrubbed = lit;
      for (const re of sanctionedRes) scrubbed = scrubbed.replace(re, "");
      for (const m of matchers) {
        if (m.re.test(scrubbed)) {
          hits.push(`${f}: term "${m.term}" in ${lit.slice(0, 60)}`);
        }
      }
    }
  }
}

if (hits.length) {
  console.error("microcopy-lint: FAILED. Default-mode strings must use the user-facing term.");
  for (const h of hits.slice(0, 50)) console.error("  " + h);
  process.exit(1);
}
console.log(`microcopy-lint: OK (${scanned} default-mode files clean)`);
