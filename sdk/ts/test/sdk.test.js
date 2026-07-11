// Runnable proof (node --test) that the SDK builders produce the object shape a
// compiled workflow.ts is written against. We rebuild the canonical notepad
// workflow (contracts/fixtures/workflow_notepad/workflow.ts) through the SDK and
// assert the assembled object matches, step for step.
//
// The SDK is imported by relative path so the suite needs no install and no
// network. A stricter static proof (that the fixture .ts itself type-checks
// against index.d.ts) is available via `npm run typecheck` where a local
// typescript is present.

import test from "node:test";
import assert from "node:assert/strict";

import { defineWorkflow, step, input } from "../index.js";

const WINDOW = { process: "notepad.exe", titlePattern: ".* - Notepad" };
const EDITOR_SELECTORS = [
  { kind: "automation_id", value: "RichEditD2DPT" },
  { kind: "name_role_path", path: [{ role: "window", name: "Untitled - Notepad" }, { role: "document", name: "Text editor" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
];

test("input builders produce typed descriptors", () => {
  assert.deepEqual(input.date({ default: "2026-07-11", label: "Invoice date" }), {
    type: "date",
    default: "2026-07-11",
    label: "Invoice date",
  });
  assert.deepEqual(input.currency({ default: "142.50", label: "Amount" }), {
    type: "currency",
    default: "142.50",
    label: "Amount",
  });
  assert.equal(input.filePath().type, "file_path");
  assert.equal(input.email().type, "email");
  assert.equal(input.url().type, "url");
});

test("step builders tag each step with its kind", () => {
  assert.equal(step.click({ intent: "c" }).kind, "click");
  assert.equal(step.type({ intent: "t", text: "x" }).kind, "type");
  assert.equal(step.key({ intent: "k", combo: "ctrl+s" }).kind, "key");
  assert.equal(step.wait({ intent: "w", timeoutMs: 5000 }).kind, "wait");
  assert.equal(step.assert({ intent: "a", expr: { op: "matches" } }).kind, "assert");
});

test("defineWorkflow assembles the canonical notepad workflow", () => {
  const wf = defineWorkflow({
    name: "notepad-invoice-note",
    version: "1.0.0",
    description: "Writes a dated invoice note into Notepad and saves it.",
    inputs: {
      invoice_date: input.date({ default: "2026-07-11", label: "Invoice date" }),
      amount: input.currency({ default: "142.50", label: "Amount" }),
    },
    steps: [
      step.click({
        intent: "Click the text editor",
        window: WINDOW,
        selectors: EDITOR_SELECTORS,
        risk: "read",
      }),
      step.type({
        intent: "Type the invoice note",
        window: WINDOW,
        selectors: EDITOR_SELECTORS,
        text: "Invoice {invoice_date} total ${amount}",
        risk: "write",
      }),
      step.wait({
        intent: "Wait for the screen to update",
        scope: { window: WINDOW },
        timeoutMs: 5000,
      }),
      step.key({
        intent: "Save the file",
        window: WINDOW,
        combo: "ctrl+s",
        risk: "write",
      }),
      step.wait({
        intent: "Wait for the screen to update",
        scope: { window: WINDOW },
        timeoutMs: 5000,
      }),
      step.assert({
        intent: "Check that the note was written",
        window: WINDOW,
        expr: {
          op: "matches",
          query: { kind: "snapshot_element_value", role: "document", name: "Text editor" },
          regex: "^Invoice \\d{4}-\\d{2}-\\d{2} total \\$\\d+\\.\\d{2}$",
        },
      }),
    ],
  });

  assert.equal(wf.name, "notepad-invoice-note");
  assert.equal(wf.version, "1.0.0");
  assert.equal(wf.steps.length, 6);
  assert.deepEqual(
    wf.steps.map((s) => s.kind),
    ["click", "type", "wait", "key", "wait", "assert"]
  );

  // Inputs are typed descriptors.
  assert.equal(wf.inputs.invoice_date.type, "date");
  assert.equal(wf.inputs.amount.type, "currency");
  assert.equal(wf.inputs.amount.default, "142.50");

  // The type step carries the template the compiler emitted, dollar sign intact.
  assert.equal(wf.steps[1].text, "Invoice {invoice_date} total ${amount}");

  // The assert step carries a data predicate, not a string of code.
  assert.equal(wf.steps[5].expr.op, "matches");
  assert.equal(wf.steps[5].expr.query.name, "Text editor");
});
