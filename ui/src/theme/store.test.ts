import { test } from "node:test";
import assert from "node:assert/strict";
import { createThemeStore } from "./store.ts";
import { createDomEnv } from "../styles/testDomEnv.ts";

// Every test passes an explicit initial mode (mirrors ui/src/state/mode.test.ts):
// createThemeStore()'s no-argument default reads localStorage, and these
// tests must not depend on (or leak into) whatever a jsdom instance's
// storage happens to hold.

test("defaults to the mode it is constructed with", () => {
  const store = createThemeStore("dark");
  assert.equal(store.get(), "dark");
});

test("getResolved for an explicit dark/light mode needs no DOM or matchMedia", () => {
  assert.equal(createThemeStore("dark").getResolved(), "dark");
  assert.equal(createThemeStore("light").getResolved(), "light");
});

test("system mode resolves to dark when matchMedia cannot be read at all (the safe fallback)", () => {
  const store = createThemeStore("system");
  assert.equal(store.getResolved(), "dark");
});

test("set is a no-op when setting the current mode (no notification, no re-apply)", () => {
  const store = createThemeStore("dark");
  let calls = 0;
  store.subscribe(() => {
    calls++;
  });
  store.set("dark");
  assert.equal(calls, 0);
});

test("subscribers see the new mode and resolved theme together, and can unsubscribe", () => {
  const store = createThemeStore("dark");
  const seen: Array<[string, string]> = [];
  const unsubscribe = store.subscribe((mode, resolved) => seen.push([mode, resolved]));

  store.set("light");
  unsubscribe();
  store.set("dark");

  assert.deepEqual(seen, [["light", "light"]]);
});

test("cycle goes dark -> light -> system -> dark", () => {
  const store = createThemeStore("dark");
  store.cycle();
  assert.equal(store.get(), "light");
  store.cycle();
  assert.equal(store.get(), "system");
  store.cycle();
  assert.equal(store.get(), "dark");
});

test("independent stores do not share state", () => {
  const a = createThemeStore("dark");
  const b = createThemeStore("dark");
  a.set("light");
  assert.equal(a.get(), "light");
  assert.equal(b.get(), "dark");
});

test("dispose stops notifying subscribers", () => {
  const store = createThemeStore("dark");
  let calls = 0;
  store.subscribe(() => {
    calls++;
  });
  store.dispose();
  store.set("light");
  assert.equal(calls, 0);
});

test("set persists the mode so a fresh store reads it back (survives a simulated app restart)", () => {
  const env = createDomEnv();
  try {
    const first = createThemeStore("dark");
    first.set("light");

    const second = createThemeStore();
    assert.equal(second.get(), "light");
  } finally {
    env.cleanup();
  }
});

test("init applies the resolved theme onto <html data-theme>, with no unthemed default left behind", () => {
  const env = createDomEnv();
  try {
    const store = createThemeStore("light");
    store.init();
    assert.equal(env.document.documentElement.getAttribute("data-theme"), "light");

    store.set("dark");
    assert.equal(env.document.documentElement.getAttribute("data-theme"), "dark");
  } finally {
    env.cleanup();
  }
});
