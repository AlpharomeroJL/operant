// Unit tests for provider detection from key shape (./accessKey.ts). No DOM:
// runs under plain `node --test`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { detectProviderFromKey } from "./accessKey.ts";

test("an Anthropic-shaped key detects as claude", () => {
  assert.equal(detectProviderFromKey("sk-ant-api03-abc123XYZ"), "claude");
});

test("an OpenAI-shaped key detects as chatgpt", () => {
  assert.equal(detectProviderFromKey("sk-abc123XYZ"), "chatgpt");
});

test("an OpenAI project-scoped key also detects as chatgpt", () => {
  assert.equal(detectProviderFromKey("sk-proj-abc123XYZ"), "chatgpt");
});

test("leading and trailing whitespace do not defeat detection", () => {
  assert.equal(detectProviderFromKey("   sk-ant-abc123   "), "claude");
});

test("blank or unrecognized shapes fall back to null, for the manual picker", () => {
  assert.equal(detectProviderFromKey(""), null);
  assert.equal(detectProviderFromKey("   "), null);
  assert.equal(detectProviderFromKey("just-some-random-value"), null);
});
