import { test } from "node:test";
import assert from "node:assert/strict";
import { createModeStore } from "./mode.ts";

test("defaults to default mode", () => {
  const store = createModeStore("default");
  assert.equal(store.get(), "default");
});

test("toggle flips between default and advanced", () => {
  const store = createModeStore("default");
  store.toggle();
  assert.equal(store.get(), "advanced");
  store.toggle();
  assert.equal(store.get(), "default");
});

test("set is a no-op when setting the current mode (no notification)", () => {
  const store = createModeStore("default");
  let calls = 0;
  store.subscribe(() => {
    calls++;
  });
  store.set("default");
  assert.equal(calls, 0);
});

test("subscribers see the new mode and can unsubscribe", () => {
  const store = createModeStore("default");
  const seen: string[] = [];
  const unsubscribe = store.subscribe((mode) => seen.push(mode));

  store.set("advanced");
  unsubscribe();
  store.set("default");

  assert.deepEqual(seen, ["advanced"]);
});

test("independent stores do not share state", () => {
  const a = createModeStore("default");
  const b = createModeStore("default");
  a.set("advanced");
  assert.equal(a.get(), "advanced");
  assert.equal(b.get(), "default");
});
