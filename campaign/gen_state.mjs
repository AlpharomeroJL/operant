// Generates campaign/state.json: the machine-readable ledger that is the durable
// backbone of the resume protocol. Rerunnable and idempotent: it preserves any
// existing per-packet status by merging over a prior state.json if present.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, "state.json");

// [id, name, tier, wave, deps[], owned_paths_hint]
const P = [
  // Wave 1
  ["L1A", "core-bus", "sonnet", 1, [], "crates/core"],
  ["L2A", "perception-uia", "sonnet", 1, ["L1A"], "crates/perception-uia"],
  ["L3A", "action-core", "sonnet", 1, ["L1A"], "crates/action"],
  ["L4A", "model-backends", "sonnet", 1, ["L1A"], "crates/orchestrator/src/backends,sidecars/vision"],
  ["L5A", "storage-recorder", "sonnet", 1, ["L1A"], "crates/recorder"],
  ["L6A", "safety-gates", "opus", 1, ["L1A"], "crates/safety,crates/gates"],
  ["U1A", "ux-foundation", "sonnet", 1, [], "ui"],
  ["U2A", "wizard-downloader", "sonnet", 1, [], "sidecars/downloader"],
  ["E1A", "e2e-harness", "sonnet", 1, [], "e2e"],
  ["D1A", "docs-scaffold", "haiku", 1, [], "site"],
  ["C1A", "cookbook-prose", "haiku", 1, [], "cookbook"],
  ["R1A", "registry-index", "haiku", 1, [], "../operant-registry"],
  ["B1A", "bench-scaffold", "haiku", 1, [], "crates/bench"],
  ["S1A", "soak-scaffold", "haiku", 1, [], "e2e/soak"],
  ["M1A", "launch-drafts", "sonnet", 1, [], "LAUNCH.md"],
  // Wave 2
  ["L7A", "orchestrator-loop", "sonnet", 2, ["L1A", "L3A", "L4A", "L5A", "L6A"], "crates/orchestrator"],
  ["L8A", "compiler", "opus", 2, ["L3A", "L5A", "L6A"], "crates/compiler,crates/replay,sdk/ts"],
  ["L9A", "browser-adapter", "sonnet", 2, ["L3A"], "crates/action/src/adapters/browser"],
  ["L10A", "scheduler", "haiku", 2, ["L8A"], "crates/scheduler"],
  ["L11A", "voice-sidecar", "sonnet", 2, ["L1A"], "sidecars/voice"],
  ["L2B", "adapters-native", "sonnet", 2, ["L3A"], "crates/action/src/adapters"],
  ["L4B", "backend-integration", "sonnet", 2, ["L4A"], "crates/orchestrator/src/backends,docs/models.md"],
  ["L6B", "audit-export", "opus", 2, ["L6A"], "crates/safety/src/audit"],
  ["U1B", "shell-live", "sonnet", 2, ["U1A", "L1A", "L7A"], "ui/src"],
  ["U2B", "wizard-build", "sonnet", 2, ["U2A", "U1A"], "ui/src/wizard"],
  ["U3A", "errors-doctor", "sonnet", 2, ["L1A"], "crates/doctor"],
  ["E1B", "golden-path-v0", "sonnet", 2, ["L7A", "L8A"], "e2e"],
  ["D1B", "docs-guides", "haiku", 2, ["D1A"], "site/guides"],
  ["R1B", "registry-core", "sonnet", 2, ["L6A"], "crates/registry"],
  // Wave 3
  ["L12A", "shell-complete", "sonnet", 3, ["U1B"], "ui/src"],
  ["L13A", "cli-sdk-mcp", "sonnet", 3, ["L8A"], "cli,sdk/ts,crates/orchestrator/src/mcp"],
  ["L14A", "release", "sonnet", 3, ["U1B", "L8A"], "release,ui/src-tauri"],
  ["L8B", "drift-repair", "opus", 3, ["L8A", "L2A"], "crates/compiler/src/drift"],
  ["L7B", "registry-surface", "sonnet", 3, ["R1B", "U1B"], "cli,ui/src/gallery"],
  ["L9B", "bench-suite", "sonnet", 3, ["L8A", "B1A", "C1B"], "crates/bench"],
  ["L10B", "soak-run", "haiku", 3, ["S1A", "L10A"], "e2e/soak"],
  ["U2C", "wizard-teach", "sonnet", 3, ["U2B", "L7A"], "ui/src/wizard"],
  ["U4A", "plain-english-renderer", "opus", 3, ["L8A"], "sdk/ts/src/render,ui/src/render"],
  ["U3B", "doctor-ui", "haiku", 3, ["U3A"], "ui/src/doctor"],
  ["U5A", "tour-hints", "haiku", 3, ["U1B"], "ui/src/tour"],
  ["E1C", "first-timer-path", "sonnet", 3, ["U2B", "U2C", "L12A"], "e2e/first-timer"],
  ["C1B", "cookbook-compile", "sonnet", 3, ["L8A", "C1A"], "cookbook"],
  ["M1C", "readme-draft", "sonnet", 3, ["M1A"], "README.md"],
  // Wave 4
  ["V1", "capture", "sonnet", 4, ["L12A", "E1C"], "assets"],
  ["V2", "readme-final", "sonnet", 4, ["M1C", "V1", "L9B"], "README.md"],
  ["V3", "site-deploy", "haiku", 4, ["D1B"], "site"],
  ["V4", "launch-final", "sonnet", 4, ["M1A", "V1"], "LAUNCH.md"],
  ["V5", "first-timer-release", "sonnet", 4, ["E1C", "L14A"], "e2e/first-timer"],
  // Backlog
  ["X1", "kill-switch", "sonnet", 9, ["L3A", "U1A"], "crates/action/src/killswitch"],
  ["X2", "undo-journal", "opus", 9, ["L3A", "L5A"], "crates/recorder/src/undo"],
  ["X4", "anchor-redaction", "sonnet", 9, ["L4A", "L5A"], "crates/recorder/src/redact"],
  ["X16", "oauth-broker", "sonnet", 9, ["L4A", "U2B"], "crates/orchestrator/src/oauth"],
  ["X3", "time-saved", "haiku", 9, ["L5A", "U1B"], "crates/recorder/src/metrics"],
  ["X5", "watch-and-suggest", "opus", 9, ["L5A", "L7A", "X4"], "crates/orchestrator/src/watch"],
  ["X6", "composition", "sonnet", 9, ["L8A", "L6A"], "crates/replay/src/compose"],
  ["X7", "backup-portable", "haiku", 9, ["L5A"], "crates/recorder/src/backup"],
  ["X8", "app-accessibility", "sonnet", 9, ["U1B"], "ui/src"],
  ["X9", "playwright-importer", "sonnet", 9, ["L8A", "L9A"], "cli/src/import"],
  ["X10", "community-kit", "haiku", 9, [], ".github,CONTRIBUTING.md,SECURITY.md"],
  ["X11", "narrated-demo", "sonnet", 9, ["V1", "L11A"], "assets/video"],
  ["X12", "explain-cli", "haiku", 9, ["U4A"], "cli/src/explain"],
  ["X13", "diagnostics-bundle", "haiku", 9, ["U3A"], "crates/doctor/src/bundle"],
  ["X14", "i18n-scaffold", "haiku", 9, ["U1A"], "ui/src/locales"],
  ["X15", "wasm-playground", "sonnet", 9, ["L8A", "L9A", "D1A"], "site/playground"],
];

let prior = {};
if (existsSync(OUT)) {
  try {
    const old = JSON.parse(readFileSync(OUT, "utf8"));
    for (const p of old.packets || []) prior[p.id] = p;
  } catch {}
}

const packets = P.map(([id, name, tier, wave, deps, paths]) => ({
  id,
  name,
  tier,
  wave,
  deps,
  owned_paths: paths,
  status: prior[id]?.status || "pending",
  branch: prior[id]?.branch || null,
  merge_commit: prior[id]?.merge_commit || null,
  attempts: prior[id]?.attempts || 0,
}));

const state = {
  v: 1,
  campaign: "operant-v1.0.0",
  repo: "AlpharomeroJL/operant",
  registry_repo: "AlpharomeroJL/operant-registry",
  build_tree: "D:/dev/operant",
  cargo_target: "D:/dev/operant-target",
  identity: { name: "Josef Long", email: "Josefdean@protonmail.com", no_ai_attribution: true },
  phase: prior_phase(),
  current_wave: 1,
  fix_at_gate_log: [],
  packets,
};

function prior_phase() {
  if (existsSync(OUT)) {
    try {
      return JSON.parse(readFileSync(OUT, "utf8")).phase || "phase0-bootstrap";
    } catch {}
  }
  return "phase0-bootstrap";
}

writeFileSync(OUT, JSON.stringify(state, null, 2) + "\n");
console.log(`state.json written: ${packets.length} packets`);
