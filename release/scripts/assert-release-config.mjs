#!/usr/bin/env node
// CI-style assertion: the updater endpoint constant must be present (and
// well-formed) in the release build's Tauri config, and the config must
// describe a single-source NSIS bundle with a real-looking updater pubkey.
//
// This is what docs/specs/release.md means by "endpoint present in release
// profile asserted by CI (a release build is scanned for the endpoint
// constant)". It is meant to be run from the repo root:
//
//   node release/scripts/assert-release-config.mjs
//
// Exits 0 and prints "assert-release-config: OK" on success; exits 1 with a
// list of problems otherwise. No external dependencies.

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

// The stable updater manifest URL this project ships. Kept as a single named
// constant so a future change is a one-line diff instead of a silent drift.
const EXPECTED_UPDATER_ENDPOINT =
  "https://github.com/AlpharomeroJL/operant/releases/latest/download/latest.json";

const CONFIG_PATH = path.resolve("ui/src-tauri/tauri.conf.json");

const problems = [];

function check(condition, message) {
  if (!condition) problems.push(message);
}

let raw;
try {
  raw = fs.readFileSync(CONFIG_PATH, "utf8");
} catch (e) {
  console.error(`assert-release-config: FAILED. Could not read ${CONFIG_PATH}: ${e.message}`);
  process.exit(1);
}

let config;
try {
  config = JSON.parse(raw);
} catch (e) {
  console.error(`assert-release-config: FAILED. ${CONFIG_PATH} is not valid JSON: ${e.message}`);
  process.exit(1);
}

// 1. Updater endpoint constant is present, well-formed, and matches.
const endpoints = config?.plugins?.updater?.endpoints;
check(Array.isArray(endpoints) && endpoints.length > 0, "plugins.updater.endpoints is missing or empty");
if (Array.isArray(endpoints)) {
  const hasExpected = endpoints.includes(EXPECTED_UPDATER_ENDPOINT);
  check(
    hasExpected,
    `plugins.updater.endpoints does not contain the expected stable endpoint\n` +
      `    expected: ${EXPECTED_UPDATER_ENDPOINT}\n` +
      `    found:    ${JSON.stringify(endpoints)}`,
  );
  for (const ep of endpoints) {
    let url;
    try {
      url = new URL(ep);
    } catch {
      problems.push(`endpoint is not a well-formed URL: ${ep}`);
      continue;
    }
    check(url.protocol === "https:", `endpoint is not https: ${ep}`);
  }
}

// 2. Updater pubkey is present and plausibly a base64-encoded minisign public
//    key file (see release/KEYS.md): decodes to >= 2 lines, second line
//    decodes to exactly 42 bytes starting with the "Ed" algorithm id.
const pubkey = config?.plugins?.updater?.pubkey;
check(typeof pubkey === "string" && pubkey.length > 0, "plugins.updater.pubkey is missing or empty");
if (typeof pubkey === "string" && pubkey.length > 0) {
  try {
    const decoded = Buffer.from(pubkey, "base64").toString("utf8");
    const lines = decoded.split(/\r?\n/).filter(Boolean);
    check(lines.length >= 2, "plugins.updater.pubkey does not decode to a 2-line minisign public key file");
    check(
      lines[0]?.startsWith("untrusted comment:"),
      'plugins.updater.pubkey line 1 does not start with "untrusted comment:"',
    );
    const blob = Buffer.from(lines[1] || "", "base64");
    check(blob.length === 42, `plugins.updater.pubkey key blob is ${blob.length} bytes, expected 42`);
    check(
      blob.length >= 2 && blob[0] === 0x45 && blob[1] === 0x64,
      'plugins.updater.pubkey key blob does not start with the "Ed" algorithm id',
    );
  } catch (e) {
    problems.push(`plugins.updater.pubkey is not valid base64: ${e.message}`);
  }
}

// 3. Single-source NSIS bundle target: no other Windows installer format.
const targets = config?.bundle?.targets;
check(Array.isArray(targets), "bundle.targets should be an explicit array (found: " + JSON.stringify(targets) + ")");
if (Array.isArray(targets)) {
  check(targets.includes("nsis"), "bundle.targets does not include nsis");
  check(targets.length === 1, `bundle.targets should list only nsis for a single-source bundle, found: ${JSON.stringify(targets)}`);
}

// 4. Updater artifacts (and their signatures) must actually be produced at
//    build time, or the pubkey/endpoint above are configured but inert.
check(config?.bundle?.createUpdaterArtifacts === true, "bundle.createUpdaterArtifacts is not true");

if (problems.length) {
  console.error(`assert-release-config: FAILED (${problems.length} problem${problems.length === 1 ? "" : "s"})`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}

console.log(`assert-release-config: OK (${CONFIG_PATH})`);
console.log(`  updater endpoint: ${EXPECTED_UPDATER_ENDPOINT}`);
