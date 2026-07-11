// Validates that every contract schema is a parseable JSON Schema and every
// fixture JSON parses. Deep schema validation of fixtures against schemas is
// covered by the Rust round-trip tests (operant-ir); this is the fast structural
// gate that catches malformed contract edits before a lane builds against them.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const s = statSync(p);
    if (s.isDirectory()) {
      if (name === "node_modules" || name === "target") continue;
      walk(p, out);
    } else if (name.endsWith(".json")) {
      out.push(p);
    }
  }
  return out;
}

let failed = 0;
let count = 0;

// 1. All contract schemas parse and declare $schema.
for (const f of walk("contracts")) {
  count++;
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

// 2. Campaign state parses (resume protocol depends on it).
try {
  JSON.parse(readFileSync("campaign/state.json", "utf8"));
  count++;
} catch (e) {
  console.error(`check-json: campaign/state.json invalid: ${e.message}`);
  failed++;
}

if (failed) {
  console.error(`check-json: FAILED (${failed} problems)`);
  process.exit(1);
}
console.log(`check-json: OK (${count} JSON files valid)`);
