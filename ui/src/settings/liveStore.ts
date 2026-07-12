// The real, IPC-backed settings store: the live counterpart to
// ./mockStore.ts. Same seam (it implements MockSettingsStore, so ./state.ts
// and ./view.ts drive it unchanged), but every read and write goes to the
// Rust core through the sidecar IPC commands frozen in contracts/ipc.md
// section 5f: get_settings, set_settings, purge_observation_buffer,
// export_backup, import_backup. The core owns config.changed (crates/core/
// src/config.rs), so this store never publishes that event itself; it
// SUBSCRIBES to it so a value changed anywhere else (the wizard writing a
// backend, a second window, the core reconciling) refreshes this screen.
//
// The UI speaks camelCase keys (SettingsState); the core speaks dotted Config
// keys (voice.enabled, model.label, ...). SETTINGS_KEY_MAP is the single
// source of truth for that translation, in both directions.
//
// The store keeps a synchronous in-memory cache so get() stays sync (the
// Settings screen reads it every render). Writes are optimistic: the cache
// updates and notifies at once, the set_settings command fires, and the value
// is then read back via get_settings to confirm the core actually stored it
// (contracts/ipc.md's "verify each control by reading it back"). If the core
// disagrees it is authoritative and the cache reconciles to what it holds; if
// the command fails the optimistic write rolls back.

import type { BusClient } from "../bus/mockClient.ts";
import type { ConfigChangedPayload } from "../bus/types.ts";
import {
  DEFAULT_SETTINGS,
  type SettingsState,
  type MockSettingsStore,
  type BackupArchiveCapable,
} from "./mockStore.ts";

/** A Tauri-style command invoker: `invoke(cmd, args) -> Promise<result>`. */
export type InvokeFn = <T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

/**
 * The camelCase (SettingsState) to dotted (core Config) key map. Only the
 * keys that round-trip through the core's config store are here; watchBuffer
 * Count and lastBackupAt are UI-side metadata (the observation buffer's size
 * and the last backup time), owned by purge_observation_buffer and the backup
 * commands respectively, not by set_settings. autoUpdateEnabled maps to
 * updater.auto_update, making core Config the single source of truth for it
 * and retiring the duplicate ui/src-tauri/updater-settings.json toggle
 * (docs/specs/ipc-bridge.md section 5).
 */
export const SETTINGS_KEY_MAP = {
  modelLabel: "model.label",
  voiceEnabled: "voice.enabled",
  pushToTalkKey: "voice.push_to_talk_key",
  speakingRate: "voice.speaking_rate",
  killSwitchChord: "killswitch.chord",
  watchAndSuggestEnabled: "watch.enabled",
  autoUpdateEnabled: "updater.auto_update",
  accentSyncEnabled: "appearance.accent_sync",
} as const satisfies Partial<Record<keyof SettingsState, string>>;

/** The subset of SettingsState keys that round-trip through the core config. */
export type MappedSettingKey = keyof typeof SETTINGS_KEY_MAP;

/** Dotted core key back to its camelCase SettingsState key. */
export const SETTINGS_KEY_MAP_REVERSE: Record<string, MappedSettingKey> = Object.fromEntries(
  (Object.entries(SETTINGS_KEY_MAP) as Array<[MappedSettingKey, string]>).map(([camel, dotted]) => [dotted, camel]),
);

export interface LiveSettingsStore extends MockSettingsStore, BackupArchiveCapable {
  /** Resolves once the initial get_settings load has finished (tests await it). */
  ready(): Promise<void>;
  /** Resolves once every in-flight write/verify has settled (tests await it). */
  settled(): Promise<void>;
  /** Detach the config.changed subscription. */
  dispose(): void;
}

export interface LiveSettingsStoreOptions {
  invoke: InvokeFn;
  /** Seed cache values shown before the first get_settings load resolves. */
  initial?: Partial<SettingsState>;
  /** Surfaced when a command rejects (the optimistic write is rolled back first). */
  onError?: (op: string, err: unknown) => void;
  /** Fired after a set is read back via get_settings, with the value the core actually holds. */
  onVerified?: (dottedKey: string, storedValue: unknown) => void;
}

/** Coerce a JSON config value to the type SettingsState declares for that key. */
function coerce<K extends keyof SettingsState>(key: K, value: unknown): SettingsState[K] {
  const def = DEFAULT_SETTINGS[key];
  if (typeof def === "boolean") return Boolean(value) as SettingsState[K];
  if (typeof def === "number") {
    const n = typeof value === "number" ? value : Number(value);
    return (Number.isFinite(n) ? n : def) as SettingsState[K];
  }
  if (typeof def === "string") return (typeof value === "string" ? value : String(value ?? "")) as SettingsState[K];
  return value as SettingsState[K];
}

/** Structural equality good enough for JSON config scalars (bool/number/string). */
function sameValue(a: unknown, b: unknown): boolean {
  return a === b || JSON.stringify(a) === JSON.stringify(b);
}

/**
 * The live settings store. `bus` carries config.changed FROM the core (the
 * real ui/src/bus tauriClient in the app, the mock bus in tests); `invoke`
 * carries commands TO the core.
 */
export function createLiveSettingsStore(bus: BusClient | undefined, opts: LiveSettingsStoreOptions): LiveSettingsStore {
  const { invoke, onError, onVerified } = opts;
  let state: SettingsState = { ...DEFAULT_SETTINGS, ...opts.initial };
  const listeners = new Set<(state: SettingsState) => void>();
  const inflight = new Set<Promise<unknown>>();

  function notify(): void {
    for (const fn of listeners) fn(state);
  }

  /** Update the cache locally (no IPC, no config.changed publish) and notify. */
  function setLocal<K extends keyof SettingsState>(key: K, value: SettingsState[K]): void {
    if (sameValue(state[key], value)) return;
    state = { ...state, [key]: value };
    notify();
  }

  function track<T>(p: Promise<T>): Promise<T> {
    const wrapped = p.finally(() => inflight.delete(wrapped));
    inflight.add(wrapped);
    return wrapped;
  }

  /** Read the whole config snapshot and fold every known dotted key into the cache. */
  async function loadFromCore(): Promise<void> {
    const snap = await invoke<Record<string, unknown>>("get_settings");
    if (!snap || typeof snap !== "object") return;
    let next = state;
    for (const [dotted, value] of Object.entries(snap)) {
      const camel = SETTINGS_KEY_MAP_REVERSE[dotted];
      if (!camel) continue; // core-only or unknown key: ignore, never crash (contracts/ipc.md section 9)
      next = { ...next, [camel]: coerce(camel, value) };
    }
    if (next !== state) {
      state = next;
      notify();
    }
  }

  // Load current values at construction (contracts/ipc.md: "Load current
  // values via get_settings at mount").
  const readyPromise = track(
    loadFromCore().catch((err) => {
      onError?.("get_settings", err);
    }),
  );

  // Subscribe to config.changed so a value changed elsewhere refreshes here.
  // The core emits dotted keys; map back to camelCase. setLocal's unchanged
  // guard makes the echo of our own set_settings a no-op, so there is no loop.
  const unsubscribe =
    bus?.subscribe("config.changed", (event) => {
      if (event.topic !== "config.changed") return;
      const payload = event.payload as ConfigChangedPayload;
      const camel = SETTINGS_KEY_MAP_REVERSE[payload.key];
      if (!camel) return;
      setLocal(camel, coerce(camel, payload.value));
    }) ?? (() => {});

  function set<K extends keyof SettingsState>(key: K, value: SettingsState[K]): void {
    if (sameValue(state[key], value)) return;
    const dotted = (SETTINGS_KEY_MAP as Partial<Record<keyof SettingsState, string>>)[key];
    if (!dotted) {
      // A UI-only key with no core home (should not arrive via the screen's
      // setters, but stay correct if it does): cache it, no IPC.
      setLocal(key, value);
      return;
    }
    const previous = state[key];
    setLocal(key, value); // optimistic
    track(
      (async () => {
        try {
          await invoke("set_settings", { key: dotted, value });
          // Read-back verification: the core is the source of truth.
          const snap = await invoke<Record<string, unknown>>("get_settings");
          const stored = snap?.[dotted];
          if (stored !== undefined && !sameValue(stored, value)) {
            setLocal(key, coerce(key, stored));
          }
          onVerified?.(dotted, stored);
        } catch (err) {
          setLocal(key, previous); // roll the optimistic write back
          onError?.("set_settings", err);
        }
      })(),
    );
  }

  function purgeWatchedData(): void {
    setLocal("watchBufferCount", 0); // optimistic: the buffer is now empty
    track(
      invoke<{ purged: boolean; total_writes: number }>("purge_observation_buffer").catch((err) => {
        onError?.("purge_observation_buffer", err);
      }),
    );
  }

  async function exportBackupArchive(): Promise<string> {
    const res = await track(invoke<{ bytes_b64: string }>("export_backup", {}));
    setLocal("lastBackupAt", new Date().toISOString());
    return res.bytes_b64;
  }

  async function importBackupArchive(bytesB64: string): Promise<void> {
    await track(invoke<{ imported: unknown }>("import_backup", { bytes_b64: bytesB64 }));
    // Import can rewrite many settings at once; re-read the whole snapshot.
    await track(loadFromCore());
    setLocal("lastBackupAt", new Date().toISOString());
  }

  return {
    get: () => state,
    set,
    purgeWatchedData,
    // The synchronous BackupPayload path stays available for parity with the
    // mock, but the app wires the archive methods above when this store is
    // live (see ui/src/main.ts): those are the real export_backup/import_backup.
    exportBackup: () => ({ v: 1, exportedAt: new Date().toISOString(), settings: state }),
    importBackup: (payload) => {
      if (!payload || payload.v !== 1 || typeof payload.settings !== "object" || payload.settings === null) {
        throw new Error("this backup file does not look right");
      }
      state = { ...DEFAULT_SETTINGS, ...payload.settings, lastBackupAt: new Date().toISOString() };
      notify();
    },
    exportBackupArchive,
    importBackupArchive,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    ready: () => readyPromise.then(() => undefined),
    async settled() {
      while (inflight.size) await Promise.allSettled([...inflight]);
    },
    dispose() {
      unsubscribe();
      listeners.clear();
    },
  };
}

/**
 * The real Tauri command invoker when the shell runs inside Tauri, else null
 * (npm run dev / Demo mode, where ui/src/main.ts falls back to the mock
 * store). Reads the global the webview exposes; no @tauri-apps/api import, so
 * the dev build and tests never need that dependency.
 */
export function getTauriInvoke(): InvokeFn | null {
  const g = globalThis as { __TAURI__?: { core?: { invoke?: unknown }; invoke?: unknown } };
  const t = g.__TAURI__;
  const fn = t?.core?.invoke ?? t?.invoke;
  return typeof fn === "function" ? (fn as InvokeFn) : null;
}

/** Decode a base64 string (a bytes_b64 archive from export_backup) to raw bytes. */
export function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

/** Encode raw bytes (an uploaded backup file) to the base64 import_backup wants. */
export function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  const chunk = 0x8000; // chunk so a big archive does not overflow the call stack
  for (let i = 0; i < bytes.length; i += chunk) {
    bin += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(bin);
}
