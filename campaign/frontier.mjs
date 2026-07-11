// The resume tool. Reads campaign/state.json, reconciles against the durable
// merge markers in campaign/merged/*.ok, and prints the dispatchable frontier:
// packets whose deps are ALL merged and which are not themselves merged.
//
// Usage (orchestrator, on every resume and between dispatch rounds):
//   node campaign/frontier.mjs
//
// This is pure reconciliation: git markers are the source of truth, state.json is
// the registry. A packet is "done" iff campaign/merged/<id>.ok exists.
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const state = JSON.parse(readFileSync(join(HERE, "state.json"), "utf8"));

const mergedDir = join(HERE, "merged");
const merged = new Set(
  existsSync(mergedDir)
    ? readdirSync(mergedDir).filter((f) => f.endsWith(".ok")).map((f) => f.replace(/\.ok$/, ""))
    : []
);

const byId = Object.fromEntries(state.packets.map((p) => [p.id, p]));
const isDone = (id) => merged.has(id);

const frontier = [];
const blocked = [];
for (const p of state.packets) {
  if (isDone(p.id)) continue;
  const unmet = p.deps.filter((d) => !isDone(d));
  if (unmet.length === 0) frontier.push(p);
  else blocked.push({ id: p.id, waiting_on: unmet });
}

// Priority: lower wave first; within backlog (wave 9) the trust packets lead.
const backlogPriority = ["X1", "X2", "X4", "X16"];
frontier.sort((a, b) => {
  if (a.wave !== b.wave) return a.wave - b.wave;
  const ai = backlogPriority.indexOf(a.id);
  const bi = backlogPriority.indexOf(b.id);
  if (ai !== -1 || bi !== -1) return (ai === -1 ? 99 : ai) - (bi === -1 ? 99 : bi);
  return a.id.localeCompare(b.id);
});

const doneCount = state.packets.filter((p) => isDone(p.id)).length;
console.log(`=== OPERANT CAMPAIGN FRONTIER ===`);
console.log(`phase: ${state.phase}  merged: ${doneCount}/${state.packets.length}`);
console.log(`\nDISPATCHABLE NOW (${frontier.length}):`);
for (const p of frontier) {
  console.log(`  ${p.id.padEnd(5)} [${p.tier.padEnd(6)}] w${p.wave} ${p.name}  -> ${p.owned_paths}`);
}
console.log(`\nBLOCKED (${blocked.length}):`);
for (const b of blocked.slice(0, 40)) {
  console.log(`  ${b.id.padEnd(5)} waiting on: ${b.waiting_on.join(", ")}`);
}
console.log(`\nnext_action: dispatch the DISPATCHABLE set (respect concurrency cap), gate, merge, drop <id>.ok, push, re-run this.`);
