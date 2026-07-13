#!/usr/bin/env node
// Claims gate: parse CLAIMS.md and enforce the "marketing is a test suite"
// contract (CLAIMS.md's own header). Two failures are red:
//
//   1. The `## Unbacked` section still lists a claim. Shipping copy we cannot
//      back is a red gate: cut the claim or land the test that backs it.
//   2. A citation in the `## Backed claims` section is dangling: the cited file
//      does not exist, or a `path::name` citation names a string that does not
//      actually appear in that file.
//
// Run it directly or via `just claims`:
//
//   node release/scripts/check-claims.mjs
//
// Exits 0 and prints "check-claims: OK" when every backed claim cites real,
// present evidence and the Unbacked section is empty. Exits 1 with a list of
// reasons otherwise. No external deps: Node built-ins only.
//
// This gate is intentionally NOT wired into `just ci` / `just verify`. A
// legitimately-unbacked claim must not turn the whole repo red; it is a signal
// to cut copy, surfaced by running `just claims` on its own.

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
// release/scripts/check-claims.mjs -> repo root is two levels up.
const repoRoot = path.resolve(scriptDir, "..", "..");
const claimsPath = path.join(repoRoot, "CLAIMS.md");

// --------------------------------------------------------------------------
// Markdown parsing (just enough: sections by `## ` header, and table rows).
// --------------------------------------------------------------------------
function splitSections(text) {
  // Returns [{ title, lines }]. Content before the first `## ` header is
  // attached to a synthetic "" preamble section and never validated.
  const lines = text.split(/\r?\n/);
  const sections = [];
  let current = { title: "", lines: [] };
  for (const line of lines) {
    const m = /^##\s+(.*\S)\s*$/.exec(line);
    if (m) {
      sections.push(current);
      current = { title: m[1], lines: [] };
    } else {
      current.lines.push(line);
    }
  }
  sections.push(current);
  return sections;
}

function findSection(sections, prefix) {
  return sections.find((s) => s.title.toLowerCase().startsWith(prefix));
}

function isTableRow(line) {
  return /^\s*\|/.test(line);
}

function isSeparatorRow(line) {
  // A markdown table separator: only |, -, :, and spaces.
  return /^\s*\|[\s|:-]*\|\s*$/.test(line) && line.includes("-");
}

function tableCells(line) {
  // Split a `| a | b | c |` row into ["a","b","c"] (trimmed).
  const trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  return trimmed.split("|").map((c) => c.trim());
}

// Data rows of the first table in a section: table rows minus the header row
// and the separator row.
function dataRows(section) {
  const rows = section.lines.filter(isTableRow);
  const body = [];
  let seenSeparator = false;
  for (const row of rows) {
    if (isSeparatorRow(row)) {
      seenSeparator = true;
      continue;
    }
    if (!seenSeparator) continue; // still in the header row(s)
    body.push(row);
  }
  return body;
}

function backtickSpans(cell) {
  const spans = [];
  const re = /`([^`]+)`/g;
  let m;
  while ((m = re.exec(cell)) !== null) spans.push(m[1].trim());
  return spans;
}

// --------------------------------------------------------------------------
// Citation validation
// --------------------------------------------------------------------------
function validateCitation(cite, justfileText) {
  // Returns null when valid, or a string reason when dangling.
  if (cite.startsWith("just ")) {
    const recipe = cite.slice(5).trim();
    const re = new RegExp("^" + escapeRegExp(recipe) + "(:| )", "m");
    if (!re.test(justfileText)) {
      return `recipe \`${recipe}\` not found in justfile`;
    }
    return null;
  }

  if (cite.includes("::")) {
    const idx = cite.indexOf("::");
    const filePart = cite.slice(0, idx).trim();
    const namePart = cite.slice(idx + 2).trim();
    const abs = path.join(repoRoot, filePart);
    if (!fs.existsSync(abs)) {
      return `file not found: ${filePart}`;
    }
    const content = fs.readFileSync(abs, "utf8");
    if (!content.includes(namePart)) {
      return `\`${namePart}\` does not appear in ${filePart}`;
    }
    return null;
  }

  // Bare path citation (evidence doc, fixture, source file). Strip a trailing
  // #anchor if present, then require the file to exist.
  const filePart = cite.replace(/#.*$/, "").trim();
  const abs = path.join(repoRoot, filePart);
  if (!fs.existsSync(abs)) {
    return `file not found: ${filePart}`;
  }
  return null;
}

function escapeRegExp(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// --------------------------------------------------------------------------
// Main
// --------------------------------------------------------------------------
function main() {
  if (!fs.existsSync(claimsPath)) {
    console.error(`check-claims: FAILED. CLAIMS.md not found at ${claimsPath}`);
    process.exit(1);
  }
  const text = fs.readFileSync(claimsPath, "utf8");
  const justfilePath = path.join(repoRoot, "justfile");
  const justfileText = fs.existsSync(justfilePath)
    ? fs.readFileSync(justfilePath, "utf8")
    : "";

  const sections = splitSections(text);
  const backed = findSection(sections, "backed");
  const unbacked = findSection(sections, "unbacked");

  const failures = [];

  if (!backed) {
    failures.push("no `## Backed claims` section found in CLAIMS.md");
  }
  if (!unbacked) {
    failures.push("no `## Unbacked` section found in CLAIMS.md");
  }

  // 1. Every backed claim must carry at least one citation, and every citation
  //    must resolve.
  let backedClaimCount = 0;
  let citationCount = 0;
  if (backed) {
    const rows = dataRows(backed);
    for (const row of rows) {
      backedClaimCount++;
      const cells = tableCells(row);
      const citeCell = cells[cells.length - 1] || "";
      const cites = backtickSpans(citeCell);
      const rowLabel = (cells[0] || `row ${backedClaimCount}`).trim();
      if (cites.length === 0) {
        failures.push(`backed claim ${rowLabel}: no citation in the final column`);
        continue;
      }
      for (const cite of cites) {
        citationCount++;
        const reason = validateCitation(cite, justfileText);
        if (reason) {
          failures.push(`backed claim ${rowLabel}: dangling citation \`${cite}\` (${reason})`);
        }
      }
    }
  }

  // 2. The Unbacked section must be empty of claim entries (table rows or list
  //    items). Any entry is a red gate.
  const unbackedEntries = [];
  if (unbacked) {
    for (const row of dataRows(unbacked)) {
      const cells = tableCells(row);
      unbackedEntries.push(cells.slice(0, 2).join(" | "));
    }
    for (const line of unbacked.lines) {
      if (/^\s*[-*]\s+\S/.test(line)) {
        unbackedEntries.push(line.trim());
      }
    }
  }
  if (unbackedEntries.length > 0) {
    const detail = unbackedEntries.map((e) => `      - ${e}`).join("\n");
    failures.push(
      `the \`## Unbacked\` section still lists ${unbackedEntries.length} claim(s); ` +
        `cut the copy or back it before shipping:\n${detail}`,
    );
  }

  // --------------------------------------------------------------------------
  // Report
  // --------------------------------------------------------------------------
  if (failures.length > 0) {
    console.error(`check-claims: FAILED (${failures.length} problem${failures.length === 1 ? "" : "s"})`);
    console.error(`  source: ${path.relative(repoRoot, claimsPath) || "CLAIMS.md"}`);
    console.error(`  backed claims: ${backedClaimCount}, citations checked: ${citationCount}`);
    for (const f of failures) console.error(`  - ${f}`);
    console.error("");
    console.error("  An unbacked claim is DELETED, never softened. Never fabricate a citation.");
    process.exit(1);
  }

  console.log("check-claims: OK");
  console.log(`  ${backedClaimCount} backed claims, ${citationCount} citations, all resolve.`);
  console.log("  Unbacked section is empty.");
}

main();
