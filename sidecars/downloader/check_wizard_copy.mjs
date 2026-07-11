// Local microcopy and style gate for wizard_copy.json.
//
// scripts/microcopy_lint.mjs (the repo-wide `just check-microcopy` gate) only
// scans ui/src and ui/src-tauri/locales today, so it does not see this
// lane's copy file. This script is the interim, self-contained gate for
// sidecars/downloader until a UI-wiring lane either imports this file's
// strings into a scanned catalog or scripts/microcopy_lint.mjs grows a
// sidecars/ scan root (see README.md, Follow-ups).
//
// Checks every string value in wizard_copy.json against two rules:
//   1. No contracts/microcopy_glossary.json internal term (word-boundary,
//      case-insensitive), mirroring scripts/microcopy_lint.mjs exactly.
//   2. No em dash or horizontal bar, mirroring scripts/check_emdash.mjs.
//      (That repo-wide script already covers every tracked file including
//      this one; this is a fast, dependency-free local echo of the same
//      rule so this directory is self-checking without a git checkout.)
//
// Usage: node check_wizard_copy.mjs
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const GLOSSARY_PATH = path.join(REPO_ROOT, "contracts", "microcopy_glossary.json");
const COPY_PATH = path.join(__dirname, "wizard_copy.json");

// Defined by code point, not a literal, so this checker cannot itself trip
// scripts/check_emdash.mjs.
const EM_DASH = String.fromCharCode(0x2014);
const HORIZONTAL_BAR = String.fromCharCode(0x2015);

function escapeRegExp(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function loadGlossaryMatchers() {
  const glossary = JSON.parse(readFileSync(GLOSSARY_PATH, "utf8"));
  return glossary.terms.map((t) => ({
    term: t.internal,
    re: new RegExp(`\\b${escapeRegExp(t.internal)}\\b`, "i"),
  }));
}

/** Yields [jsonPath, stringValue] for every leaf string under `node`. */
function* walkStrings(node, jsonPath = "$") {
  if (typeof node === "string") {
    yield [jsonPath, node];
  } else if (Array.isArray(node)) {
    for (let i = 0; i < node.length; i++) yield* walkStrings(node[i], `${jsonPath}[${i}]`);
  } else if (node && typeof node === "object") {
    for (const [key, value] of Object.entries(node)) yield* walkStrings(value, `${jsonPath}.${key}`);
  }
}

function main() {
  const matchers = loadGlossaryMatchers();
  const copy = JSON.parse(readFileSync(COPY_PATH, "utf8"));

  const hits = [];
  let scanned = 0;
  for (const [jsonPath, value] of walkStrings(copy)) {
    scanned++;
    for (const m of matchers) {
      if (m.re.test(value)) {
        hits.push(`${jsonPath}: internal term "${m.term}" in ${JSON.stringify(value)}`);
      }
    }
    if (value.includes(EM_DASH) || value.includes(HORIZONTAL_BAR)) {
      hits.push(`${jsonPath}: em dash in ${JSON.stringify(value)}`);
    }
  }

  if (hits.length) {
    console.error(`check-wizard-copy: FAILED (${hits.length} problem${hits.length === 1 ? "" : "s"})`);
    for (const h of hits) console.error(`  ${h}`);
    process.exitCode = 1;
    return;
  }
  console.log(`check-wizard-copy: OK (${scanned} strings clean: 0 glossary terms, 0 em dashes)`);
}

main();
