#!/usr/bin/env node
// Test that every cookbook workflow can be rendered/explained via renderWorkflow
// without glossary violations.

import { readFileSync, readdirSync, existsSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const COOKBOOK_DIR = join(
  import.meta.dirname,
  "..",
  "cookbook"
);

// Glossary internal terms (must not appear in rendered output)
const GLOSSARY_INTERNAL = [
  "trajectory",
  "compile",
  "grounding",
  "DSL",
  "manifest",
  "MCP",
  "invariant",
  "gate",
  "precondition",
  "postcondition",
  "selector",
  "anchor",
  "replay",
  "explore",
  "drift",
  "re-ground",
  "sidecar",
  "backend",
  "inference",
  "token",
  "VRAM",
  "API key",
  "OAuth",
  "PKCE",
  "capability grant",
  "risk class",
  "dry-run",
  "audit chain",
  "hash",
  "CDP",
  "UIA",
  "OCR",
  "daemon",
  "regex",
  "cron",
  "stdout",
  "stderr",
];

// Import renderWorkflow from SDK
let renderWorkflow;
try {
  const render = await import("../sdk/ts/src/render/index.js");
  renderWorkflow = render.renderWorkflow;
} catch (e) {
  console.error(`Failed to import renderWorkflow: ${e.message}`);
  process.exit(1);
}

// Find all cookbook workflow files
function findWorkflows() {
  const workflows = [];
  const cookbookEntries = readdirSync(COOKBOOK_DIR, { withFileTypes: true });

  for (const entry of cookbookEntries) {
    if (!entry.isDirectory()) continue;
    const workflowDir = join(COOKBOOK_DIR, entry.name);
    const tsPath = join(workflowDir, "workflow.ts");
    const mjsPath = join(workflowDir, "workflow.mjs");

    if (existsSync(tsPath)) {
      workflows.push({ slug: entry.name, path: tsPath });
    } else if (existsSync(mjsPath)) {
      workflows.push({ slug: entry.name, path: mjsPath });
    }
  }

  return workflows.sort((a, b) => a.slug.localeCompare(b.slug));
}

// Check text for glossary violations
function findGlossaryViolations(text) {
  const violations = [];

  for (const term of GLOSSARY_INTERNAL) {
    // Case-insensitive word-boundary match
    const regex = new RegExp(`\\b${term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`, "i");
    if (regex.test(text)) {
      violations.push(term);
    }
  }

  return violations;
}

// Test a single workflow
async function testWorkflow(workflow) {
  try {
    const mod = await import(pathToFileURL(workflow.path).href);
    const wf = mod.default;

    if (!wf || typeof wf !== "object") {
      return {
        status: "error",
        message: "default export is not a workflow object",
      };
    }

    // Render the workflow with empty steps (just manifest)
    try {
      const rendered = renderWorkflow(
        {
          v: 1,
          name: wf.name || "unknown",
          version: wf.version || "1.0.0",
          description: wf.description || "",
          step_summary: [],
          inputs_schema: wf.inputs_schema || { type: "object", properties: {} },
          capabilities: wf.capabilities || { apps: [], paths: [], network: false, risk_ceiling: "read" },
          gates: wf.gates || [],
          min_operant_version: "1.0.0",
          source_run_id: "test-cookbook",
          dsl: { path: "workflow.ts", hash: "test" },
          signature: null,
        },
        wf.steps || []
      );

      // Check rendered output for glossary violations
      const rendered_str = JSON.stringify(rendered);
      const violations = findGlossaryViolations(rendered_str);

      if (violations.length > 0) {
        return {
          status: "glossary",
          message: `Found glossary terms: ${violations.join(", ")}`,
        };
      }

      return { status: "ok", message: "Rendered successfully" };
    } catch (e) {
      return {
        status: "render_error",
        message: `renderWorkflow failed: ${e.message}`,
      };
    }
  } catch (e) {
    return {
      status: "import_error",
      message: `Failed to import: ${e.message}`,
    };
  }
}

// Main test
async function main() {
  const workflows = findWorkflows();
  console.log(`Found ${workflows.length} cookbook workflow(s).\n`);

  let passed = 0;
  let failed = 0;
  const failures = [];

  for (const workflow of workflows) {
    process.stdout.write(`  Testing ${workflow.slug}... `);
    const result = await testWorkflow(workflow);

    if (result.status === "ok") {
      console.log("OK");
      passed++;
    } else {
      console.log(`FAILED (${result.status})`);
      failed++;
      failures.push({ workflow: workflow.slug, ...result });
    }
  }

  console.log(`\nResults: ${passed} passed, ${failed} failed`);

  if (failures.length > 0) {
    console.error("\nFailures:");
    for (const f of failures) {
      console.error(`  ${f.workflow}: ${f.message}`);
    }
    process.exit(1);
  }

  process.exit(0);
}

main();
