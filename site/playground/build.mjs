// Regenerates `pkg/` from `crates/replay`: cargo build for
// wasm32-unknown-unknown with the `wasm` feature, `wasm-bindgen` to produce
// the JS glue, then `wasm-opt -Oz` if it is on PATH (best-effort, mirrors
// `just check-markdown`'s own "skip if the tool is not installed" posture:
// this script's job is reproducibility for whoever has the toolchain
// installed, not a CI gate by itself).
//
// Requires: `rustup target add wasm32-unknown-unknown`, the `wasm-bindgen`
// CLI at the SAME version as the `wasm-bindgen` crate this workspace pins
// (`crates/replay/Cargo.toml`'s `wasm` feature), and, optionally,
// `wasm-opt` (from binaryen) for the size pass.
//
// Run from this directory: `node build.mjs`.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, renameSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const here = import.meta.dirname;
const repoRoot = resolve(here, "..", "..");
const targetDir = process.env.CARGO_TARGET_DIR || join(repoRoot, "target");
const pkgDir = join(here, "pkg");
const maxTotalBytes = 5 * 1024 * 1024;

function run(cmd, args, cwd) {
  console.log(`+ ${cmd} ${args.join(" ")}`);
  execFileSync(cmd, args, { cwd: cwd || repoRoot, stdio: "inherit" });
}

function haveTool(cmd) {
  try {
    execFileSync(cmd, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

run("cargo", [
  "build",
  "-p",
  "operant-replay",
  "--features",
  "wasm",
  "--target",
  "wasm32-unknown-unknown",
  "--release",
]);

const wasmIn = join(targetDir, "wasm32-unknown-unknown", "release", "operant_replay.wasm");
if (!existsSync(wasmIn)) {
  console.error(`build.mjs: expected cargo output at ${wasmIn}, not found`);
  process.exit(1);
}

if (!haveTool("wasm-bindgen")) {
  console.error(
    "build.mjs: `wasm-bindgen` CLI not found on PATH. Install the version matching " +
      "the `wasm-bindgen` crate in crates/replay/Cargo.toml (`cargo install wasm-bindgen-cli " +
      "--version <ver>`) and re-run.",
  );
  process.exit(1);
}

mkdirSync(pkgDir, { recursive: true });
run("wasm-bindgen", [
  "--target",
  "web",
  "--out-dir",
  pkgDir,
  "--out-name",
  "operant_replay",
  wasmIn,
]);

const wasmOut = join(pkgDir, "operant_replay_bg.wasm");

if (haveTool("wasm-opt")) {
  const optOut = join(pkgDir, "operant_replay_bg.opt.wasm");
  run("wasm-opt", ["-Oz", "--all-features", "-o", optOut, wasmOut]);
  renameSync(optOut, wasmOut);
} else {
  console.warn("build.mjs: `wasm-opt` not found on PATH, skipping the size pass (best-effort).");
}

const totalBytes = ["operant_replay.js", "operant_replay_bg.wasm"]
  .map((name) => statSync(join(pkgDir, name)).size)
  .reduce((a, b) => a + b, 0);

console.log(`build.mjs: pkg/ payload is ${(totalBytes / 1024 / 1024).toFixed(2)} MiB`);
if (totalBytes > maxTotalBytes) {
  console.error(`build.mjs: payload exceeds the 5 MiB budget (${totalBytes} bytes)`);
  process.exit(1);
}
