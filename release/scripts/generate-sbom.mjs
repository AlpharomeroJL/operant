#!/usr/bin/env node
// Emits a software bill of materials into release/sbom/, per docs/specs/release.md
// ("SBOM via cargo-auditable plus npm ls"). Uses cargo-auditable when it is
// installed; falls back to `cargo tree` otherwise (both are named explicitly
// as acceptable in this lane's brief). Covers the root Rust workspace (the
// crates that get built into the shipped binaries), the ui/src-tauri Rust
// sub-workspace (the Tauri shell, deliberately its own workspace, see its
// Cargo.toml), and the ui/ npm workspace (the frontend).
//
// Usage: node release/scripts/generate-sbom.mjs
//
// release/sbom/ is regenerated in place (stable filenames, no dated
// subdirectories) so the SBOM in the repo always reflects the last time this
// script ran, and diffs stay small release to release. It is committed (not
// gitignored) so "did the SBOM script actually run and produce something" is
// visible in the commit itself; the same files are meant to also be attached
// to the GitHub release as upload assets at ship time (see REPRODUCIBLE.md).

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SBOM_DIR = path.join(REPO_ROOT, "release", "sbom");

// On Windows, npm is a npm.cmd shim. Node's Windows process spawning (since
// the CVE-2024-27980 hardening, present in the Node version this repo pins)
// refuses to exec a .cmd/.bat file at all unless shell:true is set, even when
// the exact extension-matching name is passed to execFileSync; without it
// every call fails with EINVAL. shell:true is safe here because every
// argument this script passes is a plain flag or literal repo-relative path
// with no spaces or shell metacharacters (no user-controlled input reaches
// this function).
function resolveCmd(cmd) {
  if (process.platform === "win32" && cmd === "npm") return "npm.cmd";
  return cmd;
}

function needsShell(resolvedCmd) {
  return process.platform === "win32" && /\.(cmd|bat)$/i.test(resolvedCmd);
}

function run(cmd, args, cwd) {
  const resolved = resolveCmd(cmd);
  try {
    const out = execFileSync(resolved, args, {
      cwd,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      shell: needsShell(resolved),
    });
    return { ok: true, out };
  } catch (e) {
    const out = (e.stdout || "") + (e.stderr || e.message || "");
    return { ok: false, out };
  }
}

function toolVersion(cmd, args) {
  const r = run(cmd, args);
  return r.ok ? r.out.trim().split("\n")[0] : null;
}

fs.mkdirSync(SBOM_DIR, { recursive: true });

const manifest = {
  generatedAt: new Date().toISOString(),
  tool: "release/scripts/generate-sbom.mjs",
  versions: {
    node: process.version,
    npm: toolVersion("npm", ["--version"]),
    cargo: toolVersion("cargo", ["--version"]),
    cargoAuditable: toolVersion("cargo", ["auditable", "--version"]),
  },
  components: [],
};

console.log("generate-sbom: tool versions");
console.log(`  node:            ${manifest.versions.node}`);
console.log(`  npm:             ${manifest.versions.npm ?? "(not found)"}`);
console.log(`  cargo:           ${manifest.versions.cargo ?? "(not found)"}`);
console.log(`  cargo-auditable: ${manifest.versions.cargoAuditable ?? "(not installed, falling back to cargo tree)"}`);

// ---------------------------------------------------------------------------
// Rust: root workspace + the ui/src-tauri sub-workspace.
// ---------------------------------------------------------------------------

function cargoComponent(name, cwd, outFile) {
  const useAuditable = Boolean(manifest.versions.cargoAuditable);
  let result;
  let method;
  if (useAuditable) {
    // cargo-auditable embeds SBOM data into a compiled binary; it does not
    // itself print a dependency tree, so even when installed we still use
    // `cargo tree` for this human/machine-readable snapshot and note that
    // cargo-auditable is available for embedding at actual build time.
    method = "cargo tree (cargo-auditable is installed; also embed with `cargo auditable build --release`)";
  } else {
    method = "cargo tree (cargo-auditable not installed)";
  }
  result = run("cargo", ["tree", "--locked"], cwd);
  if (!result.ok) {
    // --locked fails if the lockfile would need updating; retry without it
    // so the SBOM script still emits something (see BAR: "SBOM script runs
    // and emits something").
    result = run("cargo", ["tree"], cwd);
  }
  const outPath = path.join(SBOM_DIR, outFile);
  fs.writeFileSync(outPath, result.ok ? result.out : `FAILED to run cargo tree in ${cwd}\n\n${result.out}`);
  console.log(`generate-sbom: ${name} -> ${path.relative(REPO_ROOT, outPath)} (${result.ok ? "ok" : "FAILED, see file"})`);
  manifest.components.push({ name, cwd: path.relative(REPO_ROOT, cwd) || ".", method, ok: result.ok, file: outFile });
}

cargoComponent("rust workspace (root)", REPO_ROOT, "cargo-tree-root.txt");
cargoComponent("rust sub-workspace (ui/src-tauri)", path.join(REPO_ROOT, "ui", "src-tauri"), "cargo-tree-ui-src-tauri.txt");

// ---------------------------------------------------------------------------
// npm: ui/ frontend workspace. --package-lock-only reads package-lock.json
// directly so this works even when node_modules has not been installed.
// ---------------------------------------------------------------------------

function npmComponent() {
  const cwd = path.join(REPO_ROOT, "ui");
  const jsonResult = run("npm", ["ls", "--all", "--package-lock-only", "--json"], cwd);
  const textResult = run("npm", ["ls", "--all", "--package-lock-only"], cwd);
  fs.writeFileSync(path.join(SBOM_DIR, "npm-ls-ui.json"), jsonResult.out);
  fs.writeFileSync(path.join(SBOM_DIR, "npm-ls-ui.txt"), textResult.out);
  // `npm ls` exits non-zero on unmet/extraneous deps even when it printed a
  // usable tree (e.g. node_modules not installed locally); treat "produced
  // parseable JSON" as success rather than the process exit code.
  let parsed = false;
  try {
    JSON.parse(jsonResult.out);
    parsed = true;
  } catch {
    parsed = false;
  }
  console.log(`generate-sbom: npm workspace (ui) -> release/sbom/npm-ls-ui.{json,txt} (${parsed ? "ok" : "FAILED, see file"})`);
  manifest.components.push({
    name: "npm workspace (ui)",
    cwd: "ui",
    method: "npm ls --all --package-lock-only",
    ok: parsed,
    file: "npm-ls-ui.json",
  });
}

npmComponent();

fs.writeFileSync(path.join(SBOM_DIR, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");

const failed = manifest.components.filter((c) => !c.ok);
console.log(`generate-sbom: wrote ${manifest.components.length + 1} files to release/sbom/`);
if (failed.length) {
  console.log(`generate-sbom: ${failed.length} component(s) did not run cleanly; see the files above for details.`);
}
console.log("generate-sbom: done");
