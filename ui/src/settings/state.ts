// The Settings screen (docs/specs/ui.md: "settings (backends with the probe
// profile shown, voice, kill-switch chord, privacy: watch-and-suggest toggle
// with purge button, backup/export)"). Combines the mock store
// (./mockStore.ts), the plain-language model profile (./backendProfile.ts),
// and kill-switch chord recording (./chord.ts) into one snapshot. Pure and
// DOM-free, same split as ui/src/runViewer/state.ts.

import type { BusClient } from "../bus/mockClient.ts";
import {
  createMockSettingsStore,
  type MockSettingsStore,
  type SettingsState,
  type BackupPayload,
} from "./mockStore.ts";
import { describeBackendProfile, type BackendProfile } from "./backendProfile.ts";
import { chordPartsFromEvent, formatChord, isUsableChord, type ChordKeyEvent } from "./chord.ts";
import { settingsDetailStrings as D } from "./strings.ts";

export interface SettingsSnapshot {
  state: SettingsState;
  modelProfileLines: string[];
  recordingChord: boolean;
  lastBackupLabel: string;
}

export interface RecordChordResult {
  done: boolean;
  usable: boolean;
}

export interface SettingsView {
  getSnapshot(): SettingsSnapshot;
  subscribe(fn: (snap: SettingsSnapshot) => void): () => void;
  setVoiceEnabled(on: boolean): void;
  setPushToTalkKey(key: string): void;
  setSpeakingRate(rate: number): void;
  setWatchAndSuggest(on: boolean): void;
  purgeWatchedData(): void;
  setAutoUpdateEnabled(on: boolean): void;
  startChordRecording(): void;
  recordChordKey(event: ChordKeyEvent): RecordChordResult;
  cancelChordRecording(): void;
  exportBackup(): BackupPayload;
  importBackup(payload: BackupPayload): void;
  setBackendProfile(profile: BackendProfile | null, label?: string): void;
  dispose(): void;
}

export interface CreateSettingsOptions {
  store?: MockSettingsStore;
  initial?: Partial<SettingsState>;
}

export function createSettings(bus?: BusClient, opts: CreateSettingsOptions = {}): SettingsView {
  const store = opts.store ?? createMockSettingsStore(bus, opts.initial);
  let profile: BackendProfile | null = null;
  let recording = false;
  const listeners = new Set<(snap: SettingsSnapshot) => void>();

  function snapshot(): SettingsSnapshot {
    const s = store.get();
    return {
      state: s,
      modelProfileLines: describeBackendProfile(profile),
      recordingChord: recording,
      lastBackupLabel: s.lastBackupAt ? D.backupLastLabel(s.lastBackupAt) : D.backupNever,
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  const unsubscribeStore = store.subscribe(() => emit());

  function recordChordKey(event: ChordKeyEvent): RecordChordResult {
    if (!recording) return { done: false, usable: false };
    const parts = chordPartsFromEvent(event);
    const usable = isUsableChord(parts);
    if (usable) {
      recording = false;
      store.set("killSwitchChord", formatChord(parts));
      // store.set already emits via the subscription above.
      return { done: true, usable: true };
    }
    emit();
    return { done: false, usable: false };
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    setVoiceEnabled(on) {
      store.set("voiceEnabled", on);
    },
    setPushToTalkKey(key) {
      store.set("pushToTalkKey", key);
    },
    setSpeakingRate(rate) {
      store.set("speakingRate", rate);
    },
    setWatchAndSuggest(on) {
      store.set("watchAndSuggestEnabled", on);
    },
    purgeWatchedData() {
      store.purgeWatchedData();
    },
    setAutoUpdateEnabled(on) {
      store.set("autoUpdateEnabled", on);
    },
    startChordRecording() {
      recording = true;
      emit();
    },
    recordChordKey,
    cancelChordRecording() {
      recording = false;
      emit();
    },
    exportBackup: () => store.exportBackup(),
    importBackup(payload) {
      store.importBackup(payload);
    },
    setBackendProfile(p, label) {
      profile = p;
      if (label !== undefined) {
        store.set("modelLabel", label);
      } else {
        emit();
      }
    },
    dispose() {
      unsubscribeStore();
      listeners.clear();
    },
  };
}
