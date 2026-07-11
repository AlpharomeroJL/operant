// Totality property: rendering is TOTAL over the Action IR. Every kind in the
// contract's enum renders to a non-empty plain-English sentence with no raw
// JSON; the renderer's kind set matches the contract exactly (so a new kind in
// the contract fails this test until it is handled); and an unknown kind is
// refused rather than silently rendered. This is the "unknown-kind fallback is
// FORBIDDEN in default mode" guarantee from docs/specs/zero-code.md.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { ACTION_IR_KINDS, renderStep, renderStepParts } from "../src/render/index.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..", "..");
const schema = JSON.parse(readFileSync(join(repoRoot, "contracts", "action_ir.schema.json"), "utf8"));
const schemaKinds = schema.properties.kind.enum;

function assertClean(sentence, label) {
  assert.equal(typeof sentence, "string", `${label} must be a string`);
  assert.ok(sentence.length > 0, `${label} must be non-empty`);
  assert.ok(!/[{}]/.test(sentence), `${label} must not contain raw JSON braces: ${sentence}`);
  assert.ok(!sentence.includes("[object Object]"), `${label} must not stringify objects: ${sentence}`);
}

test("the renderer's kind set is exactly the contract's Action IR kinds", () => {
  assert.deepEqual([...ACTION_IR_KINDS].sort(), [...schemaKinds].sort());
});

test("every Action IR kind renders to a clean sentence, even with no params", () => {
  for (const kind of schemaKinds) {
    const sentence = renderStep({ kind });
    assertClean(sentence, `bare ${kind}`);
  }
});

test("every Action IR kind renders to a clean sentence with representative params", () => {
  const samples = {
    click: { kind: "click", target: { selectors: [{ kind: "name_role_path", path: [{ role: "button", name: "Save" }] }] } },
    type: { kind: "type", params: { text: "hello world" } },
    key: { kind: "key", params: { combo: "ctrl+s" } },
    scroll: { kind: "scroll", params: { direction: "down", amount: 3 } },
    drag: {
      kind: "drag",
      target: { selectors: [{ kind: "name_role_path", path: [{ role: "listitem", name: "File" }] }] },
      params: { to: { selectors: [{ kind: "name_role_path", path: [{ role: "treeitem", name: "Folder" }] }] } },
    },
    wait: { kind: "wait", params: { timeout_ms: 5000 } },
    assert: { kind: "assert", params: { expr: { op: "matches", query: { kind: "snapshot_element_value", name: "Box" } } } },
    adapter_call: { kind: "adapter_call", params: { namespace: "fs", verb: "move", args: { src: "a", dest: "b" } } },
  };

  // Every contract kind must have a sample here (guards against a kind being
  // added to the contract without a corresponding rendering proof).
  for (const kind of schemaKinds) {
    assert.ok(kind in samples, `no representative sample for kind "${kind}"`);
    const { sentence, kind: renderedKind } = renderStepParts(samples[kind]);
    assert.equal(renderedKind, kind);
    assertClean(sentence, kind);
  }
});

test("an unknown kind is refused, never rendered as raw JSON", () => {
  assert.throws(() => renderStep({ kind: "teleport" }), /unknown step kind/i);
  assert.throws(() => renderStep({ kind: undefined }), /unknown step kind/i);
});

test("adapter_call is total over unknown namespaces and verbs", () => {
  assertClean(renderStep({ kind: "adapter_call", params: { namespace: "quantum", verb: "entangle" } }), "unknown adapter");
  assertClean(renderStep({ kind: "adapter_call", params: {} }), "empty adapter");
});
