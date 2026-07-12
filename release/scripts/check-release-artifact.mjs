#!/usr/bin/env node
// Release gate: inspect a built core binary's REPORTED capabilities and FAIL if
// it is a mock artifact (real_uia/real_input false). This is the structural
// enforcement of "no mock ships as product" (release/BUILD-MATRIX.md): the same
// four automation booleans the shell gates real-work UI on (contracts/ipc.md
// section 3) also gate whether an artifact is allowed to be a release.
//
// The capability blob is read from the artifact itself, three ways:
//
//   node release/scripts/check-release-artifact.mjs                 # default: run the release binary
//   node release/scripts/check-release-artifact.mjs --bin <path>    # run a specific binary's `capabilities` verb
//   node release/scripts/check-release-artifact.mjs --caps <file>   # read a capability JSON blob from a file
//   node release/scripts/check-release-artifact.mjs --stdin         # read a capability JSON blob from stdin
//
// The default and --bin forms spawn `<binary> capabilities` (cli/src/commands/
// capabilities.rs) and parse its stdout, so the gate reads what the compiled
// artifact actually reports, not what a doc claims. The --caps/--stdin forms let
// a test feed a representative blob without a build (see cli/tests/release_gate.rs).
//
// Exits 0 and prints "check-release-artifact: OK" when the artifact is a real,
// shippable core. Exits 1 with a list of reasons otherwise. No external deps.

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

// --------------------------------------------------------------------------
// The gate decision (pure): real_uia AND real_input MUST be true, or this is a
// mock artifact and must not ship. transport_kind is a sanity assertion.
// --------------------------------------------------------------------------
function checkCapabilities(caps) {
  const failures = [];
  if (caps === null || typeof caps !== "object" || Array.isArray(caps)) {
    return { ok: false, failures: ["capability payload is not a JSON object"] };
  }
  // The load-bearing check: a real run needs BOTH features (contracts/ipc.md
  // section 3, cli/src/commands/run.rs's E4 rule). Either one false is a mock
  // core that can show a UI but cannot act on the machine.
  for (const field of ["real_uia", "real_input"]) {
    const feature = field.replace("_", "-");
    if (caps[field] !== true) {
      failures.push(
        `${field} is ${JSON.stringify(caps[field])}, expected true. The release ` +
          `core must be built with the ${feature} feature so the installed app can ` +
          `actually automate; this looks like a mock artifact.`,
      );
    }
  }
  // The core must speak the contracted sidecar transport.
  if (caps.transport_kind !== "stdio") {
    failures.push(
      `transport_kind is ${JSON.stringify(caps.transport_kind)}, expected "stdio"`,
    );
  }
  return { ok: failures.length === 0, failures };
}

// --------------------------------------------------------------------------
// Capability sources
// --------------------------------------------------------------------------
function defaultBinary() {
  // The release binary lands under the shared cargo target dir (justfile exports
  // CARGO_TARGET_DIR); honor it so this finds the artifact `build-release-core`
  // just produced, in whatever lane target dir is in effect.
  const targetDir = process.env.CARGO_TARGET_DIR || "D:/dev/operant-target";
  const exe = process.platform === "win32" ? "operant.exe" : "operant";
  return path.join(targetDir, "release", exe);
}

function capsFromBinary(bin) {
  if (!fs.existsSync(bin)) {
    console.error(`check-release-artifact: FAILED. Core binary not found: ${bin}`);
    console.error("  Build it first: just build-release-core");
    process.exit(1);
  }
  let stdout;
  try {
    stdout = execFileSync(bin, ["capabilities"], { encoding: "utf8" });
  } catch (e) {
    console.error(`check-release-artifact: FAILED. Could not run \`${bin} capabilities\`: ${e.message}`);
    process.exit(1);
  }
  const caps = parseJson(stdout, `${bin} capabilities`);
  // Extra artifact-level guard: a release build must NOT carry the dev-only
  // verbs. `record-ipc` only exists under the dev-ipc-record feature
  // (cli/src/main.rs is #[cfg]-gated); if the binary accepts it, a dev feature
  // leaked into what claims to be a release core. `--help` has no side effects.
  let devVerbPresent = false;
  try {
    execFileSync(bin, ["record-ipc", "--help"], { stdio: "ignore" });
    devVerbPresent = true;
  } catch {
    devVerbPresent = false;
  }
  return { caps, devVerbPresent };
}

function parseJson(text, source) {
  // Tolerate a leading UTF-8 BOM: contracts/ipc.md section 1 notes Windows
  // PowerShell producers emit one, and readers must strip it. A piped
  // `operant capabilities | node ... --stdin` hits exactly this.
  const clean = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
  try {
    return JSON.parse(clean);
  } catch (e) {
    console.error(`check-release-artifact: FAILED. ${source} did not emit valid JSON: ${e.message}`);
    process.exit(1);
  }
}

function readStdin() {
  try {
    return fs.readFileSync(0, "utf8");
  } catch (e) {
    console.error(`check-release-artifact: FAILED. Could not read stdin: ${e.message}`);
    process.exit(1);
  }
}

// --------------------------------------------------------------------------
// CLI
// --------------------------------------------------------------------------
function parseArgs(argv) {
  const opts = { mode: "bin", bin: null, caps: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "-h" || a === "--help") {
      opts.mode = "help";
    } else if (a === "--bin") {
      opts.mode = "bin";
      opts.bin = argv[++i];
      if (!opts.bin) fatalUsage("--bin needs a path");
    } else if (a === "--caps") {
      opts.mode = "caps";
      opts.caps = argv[++i];
      if (!opts.caps) fatalUsage("--caps needs a file path");
    } else if (a === "--stdin") {
      opts.mode = "stdin";
    } else {
      fatalUsage(`unexpected argument: ${a}`);
    }
  }
  return opts;
}

function fatalUsage(msg) {
  console.error(`check-release-artifact: ${msg}`);
  console.error("usage: check-release-artifact.mjs [--bin <path> | --caps <file> | --stdin]");
  process.exit(2);
}

function printHelp() {
  console.log("check-release-artifact.mjs [--bin <path> | --caps <file> | --stdin]");
  console.log("");
  console.log("Fail (exit 1) if a built core binary reports mock capabilities");
  console.log("(real_uia/real_input false). Default: run the release binary under");
  console.log("CARGO_TARGET_DIR. See release/BUILD-MATRIX.md.");
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.mode === "help") {
    printHelp();
    return;
  }

  let caps;
  let source;
  let devVerbPresent = false;

  if (opts.mode === "caps") {
    source = opts.caps;
    caps = parseJson(readFile(opts.caps), opts.caps);
  } else if (opts.mode === "stdin") {
    source = "<stdin>";
    caps = parseJson(readStdin(), "<stdin>");
  } else {
    const bin = opts.bin || defaultBinary();
    source = bin;
    ({ caps, devVerbPresent } = capsFromBinary(bin));
  }

  const { ok, failures } = checkCapabilities(caps);
  const allFailures = failures.slice();
  if (devVerbPresent) {
    allFailures.push(
      "binary accepts the dev-only `record-ipc` verb; a release core must be built " +
        "without the dev-ipc-record feature (and without dev-agent-bridge).",
    );
  }

  if (allFailures.length) {
    console.error(
      `check-release-artifact: FAILED (${allFailures.length} problem${allFailures.length === 1 ? "" : "s"})`,
    );
    console.error(`  source: ${source}`);
    console.error(`  reported: ${JSON.stringify(caps)}`);
    for (const f of allFailures) console.error(`  - ${f}`);
    console.error("  This artifact must not ship as a release. See release/BUILD-MATRIX.md.");
    process.exit(1);
  }

  console.log("check-release-artifact: OK (real, shippable core)");
  console.log(`  source: ${source}`);
  console.log(`  real_uia=${caps.real_uia} real_input=${caps.real_input} transport=${caps.transport_kind}`);
}

function readFile(p) {
  try {
    return fs.readFileSync(p, "utf8");
  } catch (e) {
    console.error(`check-release-artifact: FAILED. Could not read ${p}: ${e.message}`);
    process.exit(1);
  }
}

main();
