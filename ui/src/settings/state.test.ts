// Tests for the combined Settings view-model: the mock store
// (./mockStore.ts) plus model-profile text and kill-switch chord recording
// layered on top. Each is unit-tested on its own already (chord.test.ts,
// backendProfile.test.ts, mockStore.test.ts); this proves they compose the
// way the Settings screen actually uses them.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createSettings } from "./state.ts";
import type { BackendProfile } from "./backendProfile.ts";

test("toggles persist to the mock store and are visible in the next snapshot", () => {
  const settings = createSettings();
  assert.equal(settings.getSnapshot().state.watchAndSuggestEnabled, false);

  settings.setWatchAndSuggest(true);
  assert.equal(settings.getSnapshot().state.watchAndSuggestEnabled, true);

  settings.setVoiceEnabled(true);
  settings.setSpeakingRate(1.2);
  const snap = settings.getSnapshot();
  assert.equal(snap.state.voiceEnabled, true);
  assert.equal(snap.state.speakingRate, 1.2);
});

test("automatic update checks default on and can be toggled off", () => {
  const settings = createSettings();
  assert.equal(settings.getSnapshot().state.autoUpdateEnabled, true);

  settings.setAutoUpdateEnabled(false);
  assert.equal(settings.getSnapshot().state.autoUpdateEnabled, false);

  settings.setAutoUpdateEnabled(true);
  assert.equal(settings.getSnapshot().state.autoUpdateEnabled, true);
});

test("subscribers are notified when a toggle changes", () => {
  const settings = createSettings();
  const seen: boolean[] = [];
  settings.subscribe((snap) => seen.push(snap.state.watchAndSuggestEnabled));

  settings.setWatchAndSuggest(true);

  assert.deepEqual(seen, [true]);
});

test("purge zeroes the watched buffer and is visible on the next snapshot", () => {
  const settings = createSettings(undefined, { initial: { watchBufferCount: 4 } });
  assert.equal(settings.getSnapshot().state.watchBufferCount, 4);
  settings.purgeWatchedData();
  assert.equal(settings.getSnapshot().state.watchBufferCount, 0);
});

test("no model connected yet reads as a plain one-liner", () => {
  const settings = createSettings();
  assert.deepEqual(settings.getSnapshot().modelProfileLines, ["No model connected yet."]);
});

test("setBackendProfile updates the plain-language profile lines and the model label", () => {
  const settings = createSettings();
  const profile: BackendProfile = {
    backend_id: "anthropic",
    vision: true,
    tool_use: true,
    context_length: 32768,
    streaming: true,
    probed_at: "2026-07-11T00:00:00Z",
  };

  settings.setBackendProfile(profile, "Claude");

  const snap = settings.getSnapshot();
  assert.equal(snap.state.modelLabel, "Claude");
  assert.equal(snap.modelProfileLines.length, 4);
  assert.match(snap.modelProfileLines[0], /can see images/);
});

test("chord recording: a full chord saves and stops recording; a bare key does not", () => {
  const settings = createSettings();
  assert.equal(settings.getSnapshot().recordingChord, false);

  settings.startChordRecording();
  assert.equal(settings.getSnapshot().recordingChord, true);

  const bareKey = settings.recordChordKey({ key: "k" });
  assert.deepEqual(bareKey, { done: false, usable: false });
  assert.equal(settings.getSnapshot().recordingChord, true, "still recording: no modifier yet");
  assert.equal(settings.getSnapshot().state.killSwitchChord, "Ctrl+Alt+Shift+Space", "unchanged so far");

  const fullChord = settings.recordChordKey({ key: "x", ctrlKey: true, altKey: true });
  assert.deepEqual(fullChord, { done: true, usable: true });
  assert.equal(settings.getSnapshot().recordingChord, false);
  assert.equal(settings.getSnapshot().state.killSwitchChord, "Ctrl+Alt+X");
});

test("cancelChordRecording stops recording without changing the saved chord", () => {
  const settings = createSettings();
  settings.startChordRecording();
  settings.cancelChordRecording();

  assert.equal(settings.getSnapshot().recordingChord, false);
  assert.equal(settings.getSnapshot().state.killSwitchChord, "Ctrl+Alt+Shift+Space");
});

test("recordChordKey is a no-op when not recording", () => {
  const settings = createSettings();
  const result = settings.recordChordKey({ key: "x", ctrlKey: true });
  assert.deepEqual(result, { done: false, usable: false });
  assert.equal(settings.getSnapshot().state.killSwitchChord, "Ctrl+Alt+Shift+Space");
});

test("backup label reflects no-backup-yet, then the export time after one", () => {
  const settings = createSettings();
  assert.equal(settings.getSnapshot().lastBackupLabel, "You have not made a backup yet.");

  const backup = settings.exportBackup();
  const other = createSettings();
  other.importBackup(backup);

  assert.match(other.getSnapshot().lastBackupLabel, /^Last backup: /);
});

test("dispose stops notifying subscribers", () => {
  const settings = createSettings();
  let calls = 0;
  settings.subscribe(() => calls++);
  settings.dispose();
  settings.setVoiceEnabled(true);
  assert.equal(calls, 0);
});
