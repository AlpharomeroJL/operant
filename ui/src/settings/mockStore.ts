// A mocked settings store standing in for the real persisted config (the
// Rust core's config store). Same seam pattern as ui/src/bus/mockClient.ts
// (mocks the transport this lane does not own) and the localStorage-backed,
// in-memory-fallback pattern in ui/src/state/mode.ts: swap for a real
// Tauri-backed store later without changing the Settings screen. Every set()
// publishes config.changed on the bus when one is attached, matching
// contracts/bus_events.md's own note for that topic: "published by the
// config store on every set when a bus is attached".

import type { BusClient } from "../bus/mockClient.ts";
import { DEFAULT_KILL_SWITCH_CHORD } from "./chord.ts";

export interface SettingsState {
  modelLabel: string;
  voiceEnabled: boolean;
  pushToTalkKey: string;
  speakingRate: number;
  killSwitchChord: string;
  // Opt-in, OFF by default (docs/ARCHITECTURE.md's Watch-and-suggest v0 / FR-O6).
  watchAndSuggestEnabled: boolean;
  // Size of the local, redacted observation buffer FR-O6 describes as
  // "purgeable in one click"; this shell has no real detector to fill it, so
  // it only tracks a count the Settings screen's purge button can zero out.
  watchBufferCount: number;
  lastBackupAt: string | null;
}

export const DEFAULT_SETTINGS: SettingsState = {
  modelLabel: "",
  voiceEnabled: false,
  pushToTalkKey: "F9",
  speakingRate: 1,
  killSwitchChord: DEFAULT_KILL_SWITCH_CHORD,
  watchAndSuggestEnabled: false,
  watchBufferCount: 0,
  lastBackupAt: null,
};

export interface BackupPayload {
  v: 1;
  exportedAt: string;
  settings: SettingsState;
}

const STORAGE_KEY = "operant.ui.settings";

type Listener = (state: SettingsState) => void;

function readPersisted(): Partial<SettingsState> {
  try {
    const raw = typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? (parsed as Partial<SettingsState>) : {};
  } catch {
    return {};
  }
}

function persist(state: SettingsState): void {
  try {
    if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Storage unavailable (private mode, sandboxed webview): settings still
    // work in memory for the life of the session, same fallback as
    // ui/src/state/mode.ts.
  }
}

export interface MockSettingsStore {
  get(): SettingsState;
  set<K extends keyof SettingsState>(key: K, value: SettingsState[K]): void;
  purgeWatchedData(): void;
  exportBackup(): BackupPayload;
  importBackup(payload: BackupPayload): void;
  subscribe(fn: Listener): () => void;
}

/**
 * A fresh mock store per call, isolated from every other instance (tests
 * build independent stores the same way ui/src/state/mode.ts's
 * createModeStore does), optionally backed by localStorage and optionally
 * wired to a bus for config.changed.
 */
export function createMockSettingsStore(bus?: BusClient, initial: Partial<SettingsState> = {}): MockSettingsStore {
  let state: SettingsState = { ...DEFAULT_SETTINGS, ...readPersisted(), ...initial };
  const listeners = new Set<Listener>();

  function notify(): void {
    for (const fn of listeners) fn(state);
  }

  function set<K extends keyof SettingsState>(key: K, value: SettingsState[K]): void {
    const oldValue = state[key];
    if (oldValue === value) return;
    state = { ...state, [key]: value };
    persist(state);
    bus?.publish("config.changed", { key, value, old_value: oldValue });
    notify();
  }

  function purgeWatchedData(): void {
    set("watchBufferCount", 0);
  }

  function exportBackup(): BackupPayload {
    return { v: 1, exportedAt: new Date().toISOString(), settings: state };
  }

  function importBackup(payload: BackupPayload): void {
    if (!payload || payload.v !== 1 || typeof payload.settings !== "object" || payload.settings === null) {
      throw new Error("this backup file does not look right");
    }
    state = { ...DEFAULT_SETTINGS, ...payload.settings, lastBackupAt: new Date().toISOString() };
    persist(state);
    notify();
  }

  return {
    get: () => state,
    set,
    purgeWatchedData,
    exportBackup,
    importBackup,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}
