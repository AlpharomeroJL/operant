// Core rendering behavior: the canonical notepad workflow renders to numbered
// plain-English steps with parameter chips; grant prose reproduces the spec
// example; drift offers, key phrases, adapter sentences, and gate conditions
// all render to clean English. Imports the renderer by relative path (no install,
// no network), matching the existing sdk suite.

import test from "node:test";
import assert from "node:assert/strict";

import {
  renderWorkflow,
  renderStep,
  renderGrant,
  renderDriftOffer,
  renderCondition,
} from "../src/render/index.js";
import { notepadManifest, notepadSteps } from "../src/render/examples.js";

test("the notepad workflow renders as numbered plain-English steps", () => {
  const wf = renderWorkflow(notepadManifest, notepadSteps);

  assert.deepEqual(
    wf.steps.map((s) => s.sentence),
    [
      'Click "Text editor"',
      'Type "Invoice 2026-07-11 total $142.50" into "Text editor"',
      "Wait for the screen to update",
      "Save the file",
      "Wait for the screen to update",
      'Check that "Text editor" shows the expected text',
    ],
  );

  // Numbered 1..6, in order.
  assert.deepEqual(wf.steps.map((s) => s.n), [1, 2, 3, 4, 5, 6]);
});

test("parameters render as inline chips carrying the current input value", () => {
  const wf = renderWorkflow(notepadManifest, notepadSteps);
  const typeStep = wf.steps[1];

  const chips = typeStep.parts.filter((p) => p.t === "chip");
  assert.deepEqual(chips.map((c) => c.param), ["invoice_date", "amount"]);
  assert.deepEqual(chips.map((c) => c.value), ["2026-07-11", "142.50"]);
  assert.ok(chips.every((c) => c.editable === true));
});

test("input values can be overridden at render time and flow into the chips", () => {
  const wf = renderWorkflow(notepadManifest, notepadSteps, { values: { amount: "999.00" } });
  assert.equal(wf.steps[1].sentence, 'Type "Invoice 2026-07-11 total $999.00" into "Text editor"');
});

test("grant prose reads as a plain sentence (reproduces the spec example)", () => {
  assert.equal(
    renderGrant({ paths: ["C:\\Users\\Sam\\Downloads"], apps: ["chrome.exe"], network: false }),
    "This workflow can read files in Downloads and control Chrome.",
  );
  assert.equal(renderGrant(notepadManifest.capabilities), "This workflow can control Notepad.");
  assert.equal(
    renderGrant({ apps: ["outlook.exe"], paths: [], network: true }),
    "This workflow can control Outlook and connect to the internet.",
  );
  assert.equal(renderGrant({ apps: [], paths: [], network: false }), "This workflow does not need any permission.");
});

test("drift offers are a plain heads-up plus a yes/no choice, not a diff", () => {
  const offer = renderDriftOffer({ element: "Save button" });
  assert.equal(offer.text, "The Save button moved. Update the workflow?");
  assert.equal(offer.accept, "Update the workflow");
  assert.equal(offer.dismiss, "Not now");
});

test("key combos render as plain phrases", () => {
  assert.equal(renderStep({ kind: "key", params: { combo: "ctrl+c" } }), "Copy the selection");
  assert.equal(renderStep({ kind: "key", params: { combo: "ctrl+s" } }), "Save the file");
  assert.equal(renderStep({ kind: "key", params: { combo: "ctrl+shift+k" } }), "Press Ctrl+Shift+K");
});

test("adapter_call renders per namespace and verb", () => {
  assert.equal(
    renderStep({ kind: "adapter_call", params: { namespace: "fs", verb: "move", args: { src: "a.pdf", dest: "Archive" } } }),
    'Move "a.pdf" to "Archive"',
  );
  assert.equal(
    renderStep({ kind: "adapter_call", params: { namespace: "ocr", verb: "extract", args: { src: "receipt.jpg" } } }),
    'Read the text from "receipt.jpg"',
  );
  assert.equal(
    renderStep({ kind: "adapter_call", params: { namespace: "email", verb: "send", args: { to: "a@example.com" } } }),
    "Send an email to a@example.com",
  );
});

test("scroll and drag render to plain English", () => {
  assert.equal(renderStep({ kind: "scroll", params: { direction: "down" } }), "Scroll down");
  assert.equal(
    renderStep({
      kind: "drag",
      target: { selectors: [{ kind: "name_role_path", path: [{ role: "listitem", name: "Invoice.pdf" }] }] },
      params: { to: { selectors: [{ kind: "name_role_path", path: [{ role: "treeitem", name: "PDFs" }] }] } },
    }),
    'Drag "Invoice.pdf" onto "PDFs"',
  );
});

test("gate conditions render as readable conditions, never raw JSON", () => {
  assert.equal(
    renderCondition({ op: "equals", left: { kind: "snapshot_window_process" }, right: { kind: "literal", value: "notepad.exe" } }),
    "the open app is notepad.exe",
  );
  assert.equal(
    renderCondition({ op: "matches", query: { kind: "snapshot_element_value", name: "Text editor" } }),
    '"Text editor" shows the expected text',
  );
  // Unknown operator still yields a safe, jargon-free phrase.
  assert.equal(renderCondition({ op: "some_future_op" }), "the result is what we expect");
});

test("type with an input reference resolves the chip from context", () => {
  assert.equal(
    renderStep({ kind: "type", params: { input_ref: "amount" } }, { values: { amount: "9.99" } }),
    'Type "9.99" into "the text box"',
  );
});

test("the same step renders identically from the SDK shape and the Action IR shape", () => {
  const sdkShape = {
    kind: "click",
    window: { process: "notepad.exe" },
    selectors: [{ kind: "name_role_path", path: [{ role: "document", name: "Text editor" }] }],
  };
  const irShape = {
    v: 1,
    id: "01X",
    kind: "click",
    target: { window: { process: "notepad.exe" }, selectors: [{ kind: "name_role_path", path: [{ role: "document", name: "Text editor" }] }] },
    risk_class: "read",
    grounding: "uia",
  };
  assert.equal(renderStep(sdkShape), renderStep(irShape));
  assert.equal(renderStep(sdkShape), 'Click "Text editor"');
});
