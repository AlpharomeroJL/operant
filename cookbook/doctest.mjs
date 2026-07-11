#!/usr/bin/env node
// doctest.mjs - loads and validates every authored cookbook workflow file.
//
// For each cookbook/<slug>/workflow.ts (or .mjs) this script:
//   1. Dynamically imports the module. Node strips TypeScript types natively
//      on recent versions; these files use no TS-only syntax anyway, so no
//      build step or extra dependency is needed.
//   2. Checks the default export is a workflow object produced by
//      defineWorkflow: name, version, inputs, and steps are all present and
//      typed the way sdk/ts/index.d.ts describes.
//   3. Checks every step has a known kind and every input has a known type.
//   4. Reads the module's `benchmark` named export and cross-checks it
//      against cookbook/bench-workflows.json and the prose file's own
//      "Benchmark: Yes" tag, so the three workflows that feed crates/bench
//      (see docs/specs/bench.md) stay marked consistently everywhere.
//   5. Confirms each prose file's old "workflow file goes here" TODO was
//      replaced with a reference to the authored file.
//   6. Scans every file under cookbook/ for em dashes, forbidden repo-wide.
//
// No dependencies beyond Node built-ins and a relative import of the SDK
// (sdk/ts/index.js), so this needs no install and no network. Run with:
//   node cookbook/doctest.mjs

import { readFileSync, readdirSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const COOKBOOK_DIR = path.dirname(fileURLToPath(import.meta.url));

const KNOWN_STEP_KINDS = new Set(["click", "type", "key", "scroll", "wait", "assert"]);
const KNOWN_INPUT_TYPES = new Set(["date", "currency", "text", "file_path", "email", "url"]);
const EXPECTED_WORKFLOW_COUNT = 10;
const EXPECTED_BENCHMARK_COUNT = 3;

// Defined by code point so this checker contains no literal em dash of its
// own (mirrors scripts/check_emdash.mjs).
const EM = String.fromCharCode(0x2014); // em dash
const BAR = String.fromCharCode(0x2015); // horizontal bar
const BINARY_EXT = /\.(png|pdf|bin|ico|gif|mp4|woff2|jpg|jpeg|zip|exe)$/i;

const failures = [];
const fail = (msg) => failures.push(msg);

function walk(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

// ---------- 1. discover cookbook/<slug>/workflow.{ts,mjs} ----------
const slugDirs = readdirSync(COOKBOOK_DIR, { withFileTypes: true })
  .filter((e) => e.isDirectory())
  .map((e) => e.name)
  .sort();

const workflows = [];
for (const slug of slugDirs) {
  const dir = path.join(COOKBOOK_DIR, slug);
  const tsPath = path.join(dir, "workflow.ts");
  const mjsPath = path.join(dir, "workflow.mjs");
  const file = existsSync(tsPath) ? tsPath : existsSync(mjsPath) ? mjsPath : null;
  if (!file) {
    fail(`cookbook/${slug}/ has no workflow.ts or workflow.mjs`);
    continue;
  }
  workflows.push({ slug, file });
}

console.log(`Discovered ${workflows.length} workflow module(s) under cookbook/.\n`);

if (workflows.length !== EXPECTED_WORKFLOW_COUNT) {
  fail(`expected exactly ${EXPECTED_WORKFLOW_COUNT} authored workflows, found ${workflows.length}`);
}

// ---------- 2. import and validate each workflow module ----------
const benchSlugs = [];

for (const { slug, file } of workflows) {
  let mod;
  try {
    mod = await import(pathToFileURL(file).href);
  } catch (e) {
    fail(`${slug}: failed to import ${path.relative(COOKBOOK_DIR, file)}: ${e.message}`);
    continue;
  }

  const wf = mod.default;
  if (!wf || typeof wf !== "object") {
    fail(`${slug}: default export is not a workflow object`);
    continue;
  }
  if (typeof wf.name !== "string" || wf.name.length === 0) {
    fail(`${slug}: workflow.name must be a non-empty string`);
  }
  if (typeof wf.version !== "string" || wf.version.length === 0) {
    fail(`${slug}: workflow.version must be a non-empty string`);
  }
  if (!wf.inputs || typeof wf.inputs !== "object") {
    fail(`${slug}: workflow.inputs must be an object`);
  } else {
    for (const [inputName, descriptor] of Object.entries(wf.inputs)) {
      if (!descriptor || !KNOWN_INPUT_TYPES.has(descriptor.type)) {
        fail(`${slug}: input "${inputName}" has unknown type ${descriptor && descriptor.type}`);
      }
    }
  }
  let stepsOk = Array.isArray(wf.steps) && wf.steps.length > 0;
  if (!stepsOk) {
    fail(`${slug}: workflow.steps must be a non-empty array`);
  } else {
    wf.steps.forEach((s, i) => {
      if (!s || !KNOWN_STEP_KINDS.has(s.kind)) {
        fail(`${slug}: step[${i}] has unknown kind ${s && s.kind}`);
        stepsOk = false;
      }
      if (!s || typeof s.intent !== "string" || s.intent.length === 0) {
        fail(`${slug}: step[${i}] (${s && s.kind}) is missing a plain-English intent`);
        stepsOk = false;
      }
    });
  }

  const benchmark = mod.benchmark;
  if (typeof benchmark !== "boolean") {
    fail(`${slug}: expected a boolean \`benchmark\` named export, got ${typeof benchmark}`);
  } else if (benchmark) {
    benchSlugs.push(slug);
  }

  // Cross-check against the prose file: the TODO must be replaced, and the
  // Benchmark tag C1A set in prose must agree with this module's own claim.
  const prosePath = path.join(COOKBOOK_DIR, `${slug}.md`);
  if (!existsSync(prosePath)) {
    fail(`${slug}: no matching prose file cookbook/${slug}.md`);
  } else {
    const prose = readFileSync(prosePath, "utf8");
    if (/workflow file goes here/i.test(prose)) {
      fail(`${slug}.md: still contains the "workflow file goes here" TODO placeholder`);
    }
    const referencesWorkflow = prose.includes(`${slug}/workflow.ts`) || prose.includes(`${slug}/workflow.mjs`);
    if (!referencesWorkflow) {
      fail(`${slug}.md: does not reference its authored workflow file`);
    }
    const proseTaggedBench = /\*Benchmark:\s*Yes\*/i.test(prose);
    if (typeof benchmark === "boolean" && proseTaggedBench !== benchmark) {
      fail(`${slug}: benchmark export (${benchmark}) disagrees with the prose "Benchmark: Yes" tag (${proseTaggedBench})`);
    }
  }

  const inputCount = wf.inputs ? Object.keys(wf.inputs).length : 0;
  const stepCount = Array.isArray(wf.steps) ? wf.steps.length : 0;
  const status = stepsOk ? "OK" : "FAIL";
  console.log(`  ${status}  ${slug}  (${stepCount} steps, ${inputCount} inputs, benchmark=${benchmark})`);
}

// ---------- 3. bench-workflows.json must name exactly the benchmark=true set ----------
const benchManifestPath = path.join(COOKBOOK_DIR, "bench-workflows.json");
if (!existsSync(benchManifestPath)) {
  fail("cookbook/bench-workflows.json is missing");
} else {
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(benchManifestPath, "utf8"));
  } catch (e) {
    fail(`cookbook/bench-workflows.json is not valid JSON: ${e.message}`);
    manifest = null;
  }
  if (manifest) {
    const manifestSlugs = (manifest.workflows || []).map((w) => w.slug).sort();
    const actualSlugs = [...benchSlugs].sort();
    if (JSON.stringify(manifestSlugs) !== JSON.stringify(actualSlugs)) {
      fail(
        `bench-workflows.json lists [${manifestSlugs.join(", ")}] but workflow modules mark ` +
          `[${actualSlugs.join(", ")}] as benchmark=true`
      );
    }
  }
}

if (benchSlugs.length !== EXPECTED_BENCHMARK_COUNT) {
  fail(
    `expected exactly ${EXPECTED_BENCHMARK_COUNT} workflows marked benchmark=true, found ` +
      `${benchSlugs.length}: ${benchSlugs.join(", ") || "(none)"}`
  );
}

// ---------- 4. no em dashes anywhere under cookbook/ ----------
const emDashHits = [];
for (const file of walk(COOKBOOK_DIR)) {
  if (BINARY_EXT.test(file)) continue;
  let text;
  try {
    text = readFileSync(file, "utf8");
  } catch {
    continue;
  }
  text.split(/\r?\n/).forEach((line, i) => {
    if (line.includes(EM) || line.includes(BAR)) {
      emDashHits.push(`${path.relative(COOKBOOK_DIR, file)}:${i + 1}: ${line.trim().slice(0, 80)}`);
    }
  });
}
if (emDashHits.length > 0) {
  fail(`em dash found in ${emDashHits.length} line(s) under cookbook/:\n  ` + emDashHits.join("\n  "));
}

// ---------- summary ----------
console.log();
console.log(`========== BENCH-FEED (${EXPECTED_BENCHMARK_COUNT} required by docs/specs/bench.md) ==========`);
for (const slug of benchSlugs) console.log(`  * ${slug}`);

console.log();
console.log("========== DOCTEST SUMMARY ==========");
console.log(`Workflow modules found:  ${workflows.length}`);
console.log(`Benchmark-tagged:        ${benchSlugs.length}`);
console.log(`Failures:                ${failures.length}`);

if (failures.length > 0) {
  console.log("\n========== FAILURES ==========");
  for (const f of failures) console.log("  FAIL: " + f);
  console.log(`\nRESULT: ${failures.length} failure(s). Run failed.\n`);
  process.exit(1);
}

console.log("\nRESULT: all ten cookbook workflows validated, three benchmark workflows marked, no em dashes.\n");
process.exit(0);
