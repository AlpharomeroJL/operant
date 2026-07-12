// Tests for the live, IPC-backed settings store (./liveStore.ts): the real
// counterpart to ./mockStore.ts. A fake invoke stands in for the Rust core's
// sidecar commands (contracts/ipc.md section 5f) so every path is exercised
// deterministically with no live core: the camelCase<->dotted key mapping,
// the get_settings load at construction, the config.changed subscription, the
// read-back verification after each set, purge_observation_buffer, and
// export_backup/import_backup.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { createSettings } from "./state.ts";
import {
  createLiveSettingsStore,
  getTauriInvoke,
  base64ToBytes,
  bytesToBase64,
  SETTINGS_KEY_MAP,
  SETTINGS_KEY_MAP_REVERSE,
  type InvokeFn,
} from "./liveStore.ts";

/**
 * A deterministic stand-in for the core: get_settings returns a mutable dotted
 * config map, set_settings writes into it, and the backup/purge commands
 * record their calls. `coerceSet` lets a test make the core store a different
 * value than asked (to exercise read-back reconciliation).
 */
function fakeCore(
  seed: Record<string, unknown> = {},
  coerceSet?: (key: string, value: unknown) => unknown,
) {
  const config: Record<string, unknown> = { ...seed };
  const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = [];
  const invoke: InvokeFn = async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push({ cmd, args });
    switch (cmd) {
      case "get_settings":
        return { ...config } as T;
      case "set_settings": {
        const key = args!.key as string;
        config[key] = coerceSet ? coerceSet(key, args!.value) : args!.value;
        return { ok: true } as T;
      }
      case "purge_observation_buffer":
        return { purged: true, total_writes: 9 } as T;
      case "export_backup":
        return { bytes_b64: "QUJD" } as T; // base64 of "ABC"
      case "import_backup":
        return { imported: { workflows: 2 } } as T;
      default:
        throw new Error(`unexpected command: ${cmd}`);
    }
  };
  return { invoke, config, calls };
}

const cmdNames = (calls: Array<{ cmd: string }>): string[] => calls.map((c) => c.cmd);

test("the key map covers the core-backed settings and inverts cleanly", () => {
  assert.equal(SETTINGS_KEY_MAP.voiceEnabled, "voice.enabled");
  assert.equal(SETTINGS_KEY_MAP.autoUpdateEnabled, "updater.auto_update");
  assert.equal(SETTINGS_KEY_MAP.killSwitchChord, "killswitch.chord");
  // Every camel->dotted entry inverts back to itself.
  for (const [camel, dotted] of Object.entries(SETTINGS_KEY_MAP)) {
    assert.equal(SETTINGS_KEY_MAP_REVERSE[dotted], camel);
  }
  // UI-only metadata is deliberately absent (owned by purge/backup, not set_settings).
  assert.equal((SETTINGS_KEY_MAP as Record<string, string>).watchBufferCount, undefined);
  assert.equal((SETTINGS_KEY_MAP as Record<string, string>).lastBackupAt, undefined);
});

test("loads current values via get_settings at construction, mapping dotted keys to camelCase", async () => {
  const core = fakeCore({
    "voice.enabled": true,
    "voice.speaking_rate": 1.5,
    "updater.auto_update": false,
    "killswitch.chord": "Ctrl+Alt+Q",
  });
  const store = createLiveSettingsStore(undefined, { invoke: core.invoke });
  await store.ready();

  const s = store.get();
  assert.equal(s.voiceEnabled, true);
  assert.equal(s.speakingRate, 1.5);
  assert.equal(s.autoUpdateEnabled, false);
  assert.equal(s.killSwitchChord, "Ctrl+Alt+Q");
  assert.ok(cmdNames(core.calls).includes("get_settings"));
});

test("a setter maps to set_settings with the dotted key, then reads it back via get_settings", async () => {
  const core = fakeCore();
  const verified: Array<[string, unknown]> = [];
  const store = createLiveSettingsStore(undefined, {
    invoke: core.invoke,
    onVerified: (k, v) => verified.push([k, v]),
  });
  await store.ready();
  core.calls.length = 0; // drop the mount-time get_settings

  store.set("voiceEnabled", true);
  assert.equal(store.get().voiceEnabled, true, "optimistic: visible immediately");
  await store.settled();

  // The write mapped to the dotted key, and a get_settings read-back followed it.
  assert.deepEqual(core.calls[0], { cmd: "set_settings", args: { key: "voice.enabled", value: true } });
  assert.equal(core.calls[1].cmd, "get_settings");
  assert.equal(core.config["voice.enabled"], true);
  assert.deepEqual(verified, [["voice.enabled", true]]);
});

test("the core is authoritative: a read-back mismatch reconciles the cached value", async () => {
  // A core that clamps speaking rate to 2 no matter what is asked.
  const core = fakeCore({}, (key, value) => (key === "voice.speaking_rate" ? 2 : value));
  const store = createLiveSettingsStore(undefined, { invoke: core.invoke });
  await store.ready();

  store.set("speakingRate", 5);
  assert.equal(store.get().speakingRate, 5, "optimistic value first");
  await store.settled();
  assert.equal(store.get().speakingRate, 2, "read-back reconciled to what the core stored");
});

test("a rejected set_settings rolls the optimistic write back and reports the error", async () => {
  const errors: string[] = [];
  const invoke: InvokeFn = async <T>(cmd: string): Promise<T> => {
    if (cmd === "get_settings") return {} as T;
    if (cmd === "set_settings") throw new Error("core refused");
    throw new Error(`unexpected: ${cmd}`);
  };
  const store = createLiveSettingsStore(undefined, { invoke, onError: (op) => errors.push(op) });
  await store.ready();

  store.set("voiceEnabled", true);
  assert.equal(store.get().voiceEnabled, true, "optimistic");
  await store.settled();
  assert.equal(store.get().voiceEnabled, false, "rolled back after the failure");
  assert.deepEqual(errors, ["set_settings"]);
});

test("config.changed from elsewhere (a dotted key) refreshes the screen", async () => {
  const bus = createMockBusClient();
  const core = fakeCore();
  const store = createLiveSettingsStore(bus, { invoke: core.invoke });
  await store.ready();

  const seen: boolean[] = [];
  store.subscribe((s) => seen.push(s.watchAndSuggestEnabled));

  bus.publish("config.changed", { key: "watch.enabled", value: true, old_value: false });

  assert.equal(store.get().watchAndSuggestEnabled, true);
  assert.deepEqual(seen, [true]);
});

test("the config.changed echo of our own set does not notify a second time", async () => {
  const bus = createMockBusClient();
  const core = fakeCore();
  const store = createLiveSettingsStore(bus, { invoke: core.invoke });
  await store.ready();

  let notifications = 0;
  store.subscribe(() => (notifications += 1));

  store.set("voiceEnabled", true); // one notification (optimistic)
  await store.settled();
  bus.publish("config.changed", { key: "voice.enabled", value: true }); // echo: already true

  assert.equal(notifications, 1);
});

test("an unknown dotted config key is ignored, never crashes (load and subscription)", async () => {
  const bus = createMockBusClient();
  const core = fakeCore({ "voice.enabled": true, "some.core.only.key": 42 });
  const store = createLiveSettingsStore(bus, { invoke: core.invoke });
  await store.ready();
  assert.equal(store.get().voiceEnabled, true);

  // A live-emitted key this screen does not model must not throw.
  assert.doesNotThrow(() => bus.publish("config.changed", { key: "another.unknown", value: 1 }));
});

test("purgeWatchedData calls purge_observation_buffer and zeroes the local count", async () => {
  const core = fakeCore();
  const store = createLiveSettingsStore(undefined, { invoke: core.invoke, initial: { watchBufferCount: 7 } });
  await store.ready();
  assert.equal(store.get().watchBufferCount, 7);

  store.purgeWatchedData();
  assert.equal(store.get().watchBufferCount, 0);
  await store.settled();
  assert.ok(cmdNames(core.calls).includes("purge_observation_buffer"));
});

test("exportBackupArchive calls export_backup and returns the archive base64", async () => {
  const core = fakeCore();
  const store = createLiveSettingsStore(undefined, { invoke: core.invoke });
  await store.ready();

  const bytesB64 = await store.exportBackupArchive();
  assert.equal(bytesB64, "QUJD");
  assert.ok(cmdNames(core.calls).includes("export_backup"));
  assert.ok(store.get().lastBackupAt, "records when the backup happened");
});

test("importBackupArchive calls import_backup with the bytes and reloads config", async () => {
  const core = fakeCore();
  const store = createLiveSettingsStore(undefined, { invoke: core.invoke });
  await store.ready();
  // The imported backup changes a value the reload must pick up.
  core.config["voice.enabled"] = true;
  core.calls.length = 0;

  await store.importBackupArchive("QUJD");

  const importCall = core.calls.find((c) => c.cmd === "import_backup");
  assert.deepEqual(importCall?.args, { bytes_b64: "QUJD" });
  assert.ok(cmdNames(core.calls).includes("get_settings"), "reloads after import");
  assert.equal(store.get().voiceEnabled, true, "reloaded value is visible");
  assert.ok(store.get().lastBackupAt);
});

test("dispose detaches the config.changed subscription", async () => {
  const bus = createMockBusClient();
  const core = fakeCore();
  const store = createLiveSettingsStore(bus, { invoke: core.invoke });
  await store.ready();

  let notifications = 0;
  store.subscribe(() => (notifications += 1));
  store.dispose();

  bus.publish("config.changed", { key: "voice.enabled", value: true });
  assert.equal(notifications, 0);
});

test("base64 helpers round-trip and decode a known value", () => {
  assert.deepEqual([...base64ToBytes("QUJD")], [65, 66, 67]); // "ABC"
  const bytes = new Uint8Array([0, 1, 2, 250, 255, 128, 64]);
  assert.deepEqual([...base64ToBytes(bytesToBase64(bytes))], [...bytes]);
});

test("getTauriInvoke: null outside Tauri, the injected invoke inside it", () => {
  const g = globalThis as { __TAURI__?: unknown };
  assert.equal(getTauriInvoke(), null);

  const fn: InvokeFn = async () => undefined as never;
  g.__TAURI__ = { core: { invoke: fn } };
  try {
    assert.equal(getTauriInvoke(), fn);
  } finally {
    delete g.__TAURI__;
  }
});

test("composes through createSettings: the view-model drives the live store and disposes it", async () => {
  const bus = createMockBusClient();
  const core = fakeCore();
  const store = createLiveSettingsStore(bus, { invoke: core.invoke });
  const settings = createSettings(bus, { store });
  await store.ready();

  // The mock store has no archive backup; the live store does.
  assert.equal(settings.supportsBackupArchive(), true);

  settings.setWatchAndSuggest(true);
  assert.equal(settings.getSnapshot().state.watchAndSuggestEnabled, true);
  await store.settled();
  assert.equal(core.config["watch.enabled"], true);

  // Disposing the view-model disposes the store's bus subscription.
  let notifications = 0;
  settings.subscribe(() => (notifications += 1));
  settings.dispose();
  bus.publish("config.changed", { key: "watch.enabled", value: false });
  assert.equal(notifications, 0);
});
