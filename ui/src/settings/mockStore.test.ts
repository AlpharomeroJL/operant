import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createMockSettingsStore, DEFAULT_SETTINGS } from "./mockStore.ts";

test("defaults: watch-and-suggest is off, kill switch chord is the guardian spec default", () => {
  const store = createMockSettingsStore();
  assert.equal(store.get().watchAndSuggestEnabled, false);
  assert.equal(store.get().killSwitchChord, "Ctrl+Alt+Shift+Space");
});

test("defaults: automatic update checks are on", () => {
  const store = createMockSettingsStore();
  assert.equal(store.get().autoUpdateEnabled, true);
});

test("defaults: accent sync is off (D6, docs/specs/design.md section 3.3's Appearance section)", () => {
  const store = createMockSettingsStore();
  assert.equal(store.get().accentSyncEnabled, false);
});

test("accent sync can be turned on and back off", () => {
  const store = createMockSettingsStore();
  store.set("accentSyncEnabled", true);
  assert.equal(store.get().accentSyncEnabled, true);
  store.set("accentSyncEnabled", false);
  assert.equal(store.get().accentSyncEnabled, false);
});

test("automatic update checks can be turned off and back on", () => {
  const store = createMockSettingsStore();
  store.set("autoUpdateEnabled", false);
  assert.equal(store.get().autoUpdateEnabled, false);
  store.set("autoUpdateEnabled", true);
  assert.equal(store.get().autoUpdateEnabled, true);
});

test("a toggle set on the store persists: reading it back returns the new value", () => {
  const store = createMockSettingsStore();
  store.set("watchAndSuggestEnabled", true);
  assert.equal(store.get().watchAndSuggestEnabled, true);
  store.set("voiceEnabled", true);
  assert.equal(store.get().voiceEnabled, true);
  // Untouched fields are unaffected.
  assert.equal(store.get().killSwitchChord, DEFAULT_SETTINGS.killSwitchChord);
});

test("set is a no-op when the value is unchanged: no notification, no bus publish", () => {
  const bus = createMockBusClient();
  const events: unknown[] = [];
  bus.subscribe("config.changed", (e) => events.push(e));
  const store = createMockSettingsStore(bus);
  let calls = 0;
  store.subscribe(() => calls++);

  store.set("voiceEnabled", DEFAULT_SETTINGS.voiceEnabled);

  assert.equal(calls, 0);
  assert.equal(events.length, 0);
});

test("set publishes config.changed with key, value, and old_value when a bus is attached", () => {
  const bus = createMockBusClient();
  const received: Array<{ key: string; value: unknown; old_value?: unknown }> = [];
  bus.subscribe("config.changed", (e) => {
    if (e.topic === "config.changed") received.push(e.payload);
  });
  const store = createMockSettingsStore(bus);

  store.set("watchAndSuggestEnabled", true);

  assert.equal(received.length, 1);
  assert.deepEqual(received[0], { key: "watchAndSuggestEnabled", value: true, old_value: false });
});

test("subscribers see every change and can unsubscribe", () => {
  const store = createMockSettingsStore();
  const seen: boolean[] = [];
  const unsubscribe = store.subscribe((s) => seen.push(s.voiceEnabled));

  store.set("voiceEnabled", true);
  unsubscribe();
  store.set("voiceEnabled", false);

  assert.deepEqual(seen, [true]);
});

test("independent stores do not share state", () => {
  const a = createMockSettingsStore();
  const b = createMockSettingsStore();
  a.set("watchAndSuggestEnabled", true);
  assert.equal(a.get().watchAndSuggestEnabled, true);
  assert.equal(b.get().watchAndSuggestEnabled, false);
});

test("purgeWatchedData zeroes the observation buffer count", () => {
  const store = createMockSettingsStore(undefined, { watchBufferCount: 7 });
  assert.equal(store.get().watchBufferCount, 7);
  store.purgeWatchedData();
  assert.equal(store.get().watchBufferCount, 0);
});

test("exportBackup round-trips through importBackup on a fresh store", () => {
  const store = createMockSettingsStore();
  store.set("killSwitchChord", "Ctrl+Alt+X");
  store.set("speakingRate", 1.4);
  const backup = store.exportBackup();

  const fresh = createMockSettingsStore();
  fresh.importBackup(backup);

  assert.equal(fresh.get().killSwitchChord, "Ctrl+Alt+X");
  assert.equal(fresh.get().speakingRate, 1.4);
  assert.ok(fresh.get().lastBackupAt, "importing a backup records when it happened");
});

test("importBackup refuses a malformed payload", () => {
  const store = createMockSettingsStore();
  assert.throws(() => store.importBackup({ v: 2, exportedAt: "", settings: {} } as never));
});
