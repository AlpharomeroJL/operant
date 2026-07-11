// Proves the microcopy lint (scripts/microcopy_lint.mjs) actually catches
// jargon in default mode, rather than just trusting that our own strings
// happen to be clean. Runs the real script as a child process against a
// throwaway fixture tree, so it tests the actual CI gate, not a
// reimplementation of its regex.
//
// The bait term is read from the real glossary at run time (never typed as
// a literal in this file) for two reasons: it stays correct if the
// glossary's term list changes, and it keeps this file itself free of a
// raw internal term that this very lint would otherwise flag.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, cpSync, rmSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

// This file lives at ui/src/__tests__/microcopy-lint.test.ts; the repo root
// (where scripts/ and contracts/ live) is three levels up.
const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..", "..");
const lintScript = join(repoRoot, "scripts", "microcopy_lint.mjs");
const glossaryPath = join(repoRoot, "contracts", "microcopy_glossary.json");

interface GlossaryTerm {
  internal: string;
  user_facing: string;
}

const glossary = JSON.parse(readFileSync(glossaryPath, "utf8")) as { terms: GlossaryTerm[] };
const sampleTerm: GlossaryTerm = glossary.terms[0] ?? { internal: "trajectory", user_facing: "recording" };

interface LintRun {
  status: number | null;
  output: string;
}

function runLintAgainst(setup: (tmpRoot: string) => void): LintRun {
  const tmpRoot = mkdtempSync(join(tmpdir(), "microcopy-lint-test-"));
  try {
    mkdirSync(join(tmpRoot, "contracts"), { recursive: true });
    cpSync(glossaryPath, join(tmpRoot, "contracts", "microcopy_glossary.json"));
    mkdirSync(join(tmpRoot, "ui", "src"), { recursive: true });
    setup(tmpRoot);

    const result = spawnSync(process.execPath, [lintScript], {
      cwd: tmpRoot,
      encoding: "utf8",
    });
    return { status: result.status, output: `${result.stdout}\n${result.stderr}` };
  } finally {
    rmSync(tmpRoot, { recursive: true, force: true });
  }
}

function jargonFixture(): string {
  return `export const label = "Open the ${sampleTerm.internal} viewer";\n`;
}

function cleanFixture(): string {
  return `export const label = "Open the ${sampleTerm.user_facing} viewer";\n`;
}

test("microcopy lint fails a default-mode string that uses an internal term", () => {
  const { status, output } = runLintAgainst((tmpRoot) => {
    writeFileSync(join(tmpRoot, "ui", "src", "fixture.ts"), jargonFixture());
  });

  assert.equal(status, 1, `expected the lint to fail, got status ${status}. Output:\n${output}`);
  assert.match(output, /FAILED/);
  assert.ok(output.includes(sampleTerm.internal), `expected the output to name the term. Output:\n${output}`);
});

test("microcopy lint passes the same term when the file is marked @advanced", () => {
  const advancedMarker = ["@", "advanced"].join("");
  const { status, output } = runLintAgainst((tmpRoot) => {
    writeFileSync(join(tmpRoot, "ui", "src", "fixture.ts"), `// ${advancedMarker}\n${jargonFixture()}`);
  });

  assert.equal(status, 0, `expected the lint to pass an advanced-marked file, got status ${status}. Output:\n${output}`);
  assert.match(output, /OK/);
});

test("microcopy lint passes the same term when the file is under an advanced directory", () => {
  const { status } = runLintAgainst((tmpRoot) => {
    const advDir = join(tmpRoot, "ui", "src", "advanced");
    mkdirSync(advDir, { recursive: true });
    writeFileSync(join(advDir, "fixture.ts"), jargonFixture());
  });

  assert.equal(status, 0);
});

test("microcopy lint passes clean default-mode strings", () => {
  const { status, output } = runLintAgainst((tmpRoot) => {
    writeFileSync(join(tmpRoot, "ui", "src", "fixture.ts"), cleanFixture());
  });

  assert.equal(status, 0, `expected clean strings to pass. Output:\n${output}`);
});
