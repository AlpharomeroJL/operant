// Bidirectional for PARAMETERS ONLY: a form edit of an input value round-trips
// back into the workflow details, and the details still validate in shape. Step
// logic is never touched. Editing to a value that breaks an input's shape, or a
// key that is not an input, is refused so the details cannot be corrupted.

import test from "node:test";
import assert from "node:assert/strict";

import { renderInputs, applyInputEdits, validateManifestShape } from "../src/render/index.js";
import { notepadManifest } from "../src/render/examples.js";

test("the notepad details validate in shape to begin with", () => {
  const res = validateManifestShape(notepadManifest);
  assert.deepEqual(res, { ok: true, errors: [] });
});

test("inputs render as editable form fields with current values", () => {
  const fields = renderInputs(notepadManifest);
  assert.deepEqual(
    fields.map((f) => ({ name: f.name, label: f.label, value: f.value })),
    [
      { name: "invoice_date", label: "Invoice date", value: "2026-07-11" },
      { name: "amount", label: "Amount", value: "142.50" },
    ],
  );
});

test("a form edit round-trips into the details and the details still validate", () => {
  const edited = applyInputEdits(notepadManifest, { amount: "200.00", invoice_date: "2026-08-01" });

  // The edit landed on the input defaults...
  assert.equal(edited.inputs_schema.properties.amount.default, "200.00");
  assert.equal(edited.inputs_schema.properties.invoice_date.default, "2026-08-01");

  // ...the details still validate in shape...
  assert.deepEqual(validateManifestShape(edited), { ok: true, errors: [] });

  // ...and rendering the edited details shows the new values.
  assert.deepEqual(
    renderInputs(edited).map((f) => f.value),
    ["2026-08-01", "200.00"],
  );
});

test("editing does not mutate the original details (pure round-trip)", () => {
  const before = JSON.stringify(notepadManifest);
  applyInputEdits(notepadManifest, { amount: "1.00" });
  assert.equal(JSON.stringify(notepadManifest), before);
});

test("feeding the rendered values straight back is an identity round-trip", () => {
  const values = Object.fromEntries(renderInputs(notepadManifest).map((f) => [f.name, f.value]));
  const same = applyInputEdits(notepadManifest, values);
  assert.deepEqual(same, notepadManifest);
});

test("step logic is out of reach: unknown keys are refused", () => {
  assert.throws(() => applyInputEdits(notepadManifest, { steps: "tampered" }), /no detail called/i);
  assert.throws(() => applyInputEdits(notepadManifest, { not_an_input: "x" }), /no detail called/i);
});

test("a value that breaks an input's shape is refused", () => {
  // amount must look like 0.00; a bare word is refused.
  assert.throws(() => applyInputEdits(notepadManifest, { amount: "free" }), /does not look right/i);
  // invoice_date must be a date.
  assert.throws(() => applyInputEdits(notepadManifest, { invoice_date: "next tuesday" }), /does not look right/i);
});

test("validateManifestShape catches a corrupted detail default", () => {
  const broken = structuredClone(notepadManifest);
  broken.inputs_schema.properties.amount.default = "not-a-currency";
  const res = validateManifestShape(broken);
  assert.equal(res.ok, false);
  assert.ok(res.errors.some((e) => /amount/.test(e)));
});
