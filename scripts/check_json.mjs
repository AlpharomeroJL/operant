// Validates that every contract schema is a parseable JSON Schema and every
// fixture JSON parses. Deep schema validation of fixtures against schemas is
// covered by the Rust round-trip tests (operant-ir); this is the fast structural
// gate that catches malformed contract edits before code builds against them.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const s = statSync(p);
    if (s.isDirectory()) {
      if (name === "node_modules" || name === "target") continue;
      walk(p, out);
    } else if (name.endsWith(".json") || name.endsWith(".jsonl")) {
      out.push(p);
    }
  }
  return out;
}

let failed = 0;
let count = 0;

// All contract schemas and fixtures parse. A `.jsonl` file (the IPC session
// fixtures, contracts/fixtures/ipc/) is newline-delimited JSON: each non-empty
// line must be a valid JSON value on its own. A `.json` file must parse whole,
// and a `.schema.json` must additionally declare $schema and $id.
for (const f of walk("contracts")) {
  count++;
  if (f.endsWith(".jsonl")) {
    const lines = readFileSync(f, "utf8").split(/\r?\n/);
    let bad = 0;
    lines.forEach((line, i) => {
      if (line.trim() === "") return;
      try {
        JSON.parse(line);
      } catch (e) {
        console.error(`check-json: JSONL PARSE FAIL ${f}:${i + 1}: ${e.message}`);
        bad++;
      }
    });
    if (bad) failed += bad;
    continue;
  }
  let doc;
  try {
    doc = JSON.parse(readFileSync(f, "utf8"));
  } catch (e) {
    console.error(`check-json: PARSE FAIL ${f}: ${e.message}`);
    failed++;
    continue;
  }
  if (f.endsWith(".schema.json")) {
    if (!doc["$schema"] || !doc["$id"]) {
      console.error(`check-json: ${f} missing $schema or $id`);
      failed++;
    }
  }
}

if (failed) {
  console.error(`check-json: FAILED (${failed} problems)`);
  process.exit(1);
}
console.log(`check-json: OK (${count} JSON files valid)`);
