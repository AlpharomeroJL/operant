// Thin adapter for `operant explain` (cli/src/commands/explain.rs): reads
// `{ manifest, steps }` as one JSON object from stdin, calls the existing
// @operant/sdk/render `renderWorkflow` (sdk/ts/src/render, owned by U4A --
// this script does not reimplement it), and writes the RenderedWorkflow
// result as JSON to stdout. All plain-English formatting of that result
// happens back in Rust; this script's only job is to call the real
// renderer and hand its output back structured.
//
// Not part of the @operant/sdk package surface (no export from
// sdk/ts/index.js): this is CLI-internal plumbing, invoked as a
// subprocess, never imported.

import { renderWorkflow } from "../../sdk/ts/src/render/index.js";

let raw = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) raw += chunk;

let input;
try {
  input = JSON.parse(raw);
} catch (e) {
  console.error(`explain.mjs: stdin is not valid JSON: ${e.message}`);
  process.exit(2);
}

try {
  const rendered = renderWorkflow(input.manifest, input.steps || [], { values: input.values || {} });
  process.stdout.write(JSON.stringify(rendered));
} catch (e) {
  console.error(`explain.mjs: render failed: ${e.message}`);
  process.exit(1);
}
