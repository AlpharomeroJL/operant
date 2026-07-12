// BAR: "frecency ordering of recents."

import { test } from "node:test";
import assert from "node:assert/strict";
import { frecencyScore, createFrecencyStore } from "./frecency.ts";

test("frecencyScore: an entry never picked scores 0 regardless of its timestamp", () => {
  assert.equal(frecencyScore({ count: 0, lastUsedAt: 0 }, 1_000_000), 0);
});

test("frecencyScore: more recent beats less recent at equal count", () => {
  const now = 10_000_000;
  const recent = frecencyScore({ count: 1, lastUsedAt: now - 1_000 }, now); // seconds ago
  const stale = frecencyScore({ count: 1, lastUsedAt: now - 30 * 24 * 3_600_000 }, now); // a month ago
  assert.ok(recent > stale, "a pick from moments ago must outscore one from a month ago at the same count");
});

test("frecencyScore: picked more often beats picked once, at equal recency", () => {
  const now = 10_000_000;
  const oftenPicked = frecencyScore({ count: 20, lastUsedAt: now }, now);
  const oncePicked = frecencyScore({ count: 1, lastUsedAt: now }, now);
  assert.ok(oftenPicked > oncePicked, "twenty picks must outscore one pick at the same recency");
});

test("frecencyScore: a much older but far more frequent entry can still outrank a single recent pick", () => {
  const now = 10_000_000;
  const oldFavorite = frecencyScore({ count: 200, lastUsedAt: now - 10 * 24 * 3_600_000 }, now); // 10 days ago, weight 10
  const onceJustNow = frecencyScore({ count: 1, lastUsedAt: now }, now); // weight 100
  assert.ok(oldFavorite > onceJustNow, "200 * 10 must beat 1 * 100");
});

test("createFrecencyStore: countOf/scoreOf are 0 for an id never recorded", () => {
  const store = createFrecencyStore({ now: () => 0, storageKey: "test.frecency.unrecorded" });
  assert.equal(store.countOf("nope"), 0);
  assert.equal(store.scoreOf("nope"), 0);
});

test("createFrecencyStore: record() bumps count and refreshes the timestamp", () => {
  let clock = 1000;
  const store = createFrecencyStore({ now: () => clock, storageKey: "test.frecency.bump" });
  store.record("wf-a");
  assert.equal(store.countOf("wf-a"), 1);
  clock = 2000;
  store.record("wf-a");
  assert.equal(store.countOf("wf-a"), 2);
  assert.equal(store.all()[0].lastUsedAt, 2000);
});

test("createFrecencyStore: all() ranks by frecency score, most-frecent first", () => {
  let clock = 0;
  const store = createFrecencyStore({ now: () => clock, storageKey: "test.frecency.rank" });

  // "rarely" is picked once, long ago.
  store.record("rarely");
  clock = 20 * 24 * 3_600_000; // 20 days later

  // "often" is picked many times, recently.
  for (let i = 0; i < 10; i++) store.record("often");

  const ranked = store.all().map((e) => e.id);
  assert.deepEqual(ranked, ["often", "rarely"], "the frequently and recently picked entry must rank first");
});

test("createFrecencyStore: subscribe fires with the latest ranking after each record()", () => {
  let clock = 0;
  const store = createFrecencyStore({ now: () => clock, storageKey: "test.frecency.subscribe" });
  const seen: string[][] = [];
  const unsubscribe = store.subscribe((entries) => seen.push(entries.map((e) => e.id)));

  store.record("a");
  clock = 10 * 24 * 3_600_000; // 10 days later: far enough that "a" drops into a lower recency bucket than a fresh "b"
  store.record("b");

  assert.deepEqual(seen, [["a"], ["b", "a"]]);
  unsubscribe();
  store.record("c");
  assert.equal(seen.length, 2, "no further notifications after unsubscribe");
});

test("createFrecencyStore: persists across a new store instance sharing the same storage key (in-memory fallback when localStorage is undefined)", () => {
  // This test environment (plain `node --test`, no jsdom) has no global
  // localStorage at all, which is exactly the "storage unavailable" branch
  // ./frecency.ts's readStored/writeStored fall back to: every store starts
  // fresh in memory rather than throwing. Confirms that fallback is inert,
  // not that persistence itself works (ui/src/palette/state.test.ts's
  // jsdom-backed counterpart, if any, would cover real localStorage).
  assert.equal(typeof globalThis.localStorage, "undefined");
  const first = createFrecencyStore({ storageKey: "test.frecency.persist" });
  first.record("x");
  const second = createFrecencyStore({ storageKey: "test.frecency.persist" });
  assert.equal(second.countOf("x"), 0, "without localStorage, a fresh store never sees another store's picks");
});
