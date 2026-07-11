import { test } from "node:test";
import assert from "node:assert/strict";
import { createGrantPrompt } from "./state.ts";
import { renderGrantSentences } from "./sdkGrant.ts";

test("renders the exact grant sentence from capabilities via U4A's renderer", () => {
  // This is the literal example from docs/specs/ui.md's grant prompt spec.
  const sentences = renderGrantSentences({ paths: ["C:\\Users\\demo\\Downloads"], apps: ["chrome.exe"] });
  assert.deepEqual(sentences, ["This workflow can read files in Downloads and control Chrome."]);
});

test("a workflow with no capabilities reads as needing no permission", () => {
  const sentences = renderGrantSentences({ apps: [], paths: [], network: false });
  assert.deepEqual(sentences, ["This workflow does not need any permission."]);
});

test("no capabilities argument at all still renders a plain sentence, never throws", () => {
  const sentences = renderGrantSentences(undefined);
  assert.equal(sentences.length, 1);
  assert.equal(typeof sentences[0], "string");
  assert.ok(sentences[0].length > 0);
});

test("a fresh grant prompt starts pending with the right title and button labels", () => {
  const prompt = createGrantPrompt({ paths: ["Downloads"], apps: ["chrome.exe"] });
  const snap = prompt.getSnapshot();
  assert.equal(snap.status, "pending");
  assert.equal(snap.title, "This workflow needs permission");
  assert.equal(snap.allowLabel, "Allow");
  assert.equal(snap.denyLabel, "Deny");
  assert.deepEqual(snap.sentences, ["This workflow can read files in Downloads and control Chrome."]);
});

test("allow moves to allowed, notifies subscribers, and fires onAllow exactly once", () => {
  let allowCalls = 0;
  const prompt = createGrantPrompt({ apps: ["chrome.exe"] }, { onAllow: () => allowCalls++ });
  const seen: string[] = [];
  prompt.subscribe((snap) => seen.push(snap.status));

  prompt.allow();
  prompt.allow(); // already resolved: must be a no-op, not a second callback
  prompt.deny(); // also a no-op once resolved

  assert.equal(prompt.getSnapshot().status, "allowed");
  assert.equal(allowCalls, 1);
  assert.deepEqual(seen, ["allowed"]);
});

test("deny moves to denied, notifies subscribers, and fires onDeny exactly once", () => {
  let denyCalls = 0;
  const prompt = createGrantPrompt({ apps: ["chrome.exe"] }, { onDeny: () => denyCalls++ });
  const seen: string[] = [];
  prompt.subscribe((snap) => seen.push(snap.status));

  prompt.deny();
  prompt.deny();
  prompt.allow();

  assert.equal(prompt.getSnapshot().status, "denied");
  assert.equal(denyCalls, 1);
  assert.deepEqual(seen, ["denied"]);
});

test("works with no callbacks supplied at all", () => {
  const prompt = createGrantPrompt({ network: true });
  assert.doesNotThrow(() => prompt.allow());
  assert.equal(prompt.getSnapshot().status, "allowed");
});
