import { test } from "node:test";
import assert from "node:assert/strict";
import { buildQuickActionEntries, buildSettingsEntries, PALETTE_ACTION_ID, PALETTE_SETTING_ID } from "./quickActions.ts";

test("buildQuickActionEntries: one entry per nav target plus the theme cycle, all kind 'action', unique ids, non-empty titles", () => {
  const entries = buildQuickActionEntries();
  assert.equal(entries.length, 5);
  for (const e of entries) {
    assert.equal(e.kind, "action");
    assert.ok(e.title.length > 0, `entry ${e.id} must have a title`);
  }
  const ids = entries.map((e) => e.id);
  assert.equal(new Set(ids).size, ids.length, "every action id must be unique");
  assert.deepEqual(new Set(ids), new Set(Object.values(PALETTE_ACTION_ID)));
});

test("buildQuickActionEntries: the nav actions reuse the exact nav bar labels, so palette and header always agree", () => {
  const entries = buildQuickActionEntries();
  const byId = new Map(entries.map((e) => [e.id, e]));
  assert.equal(byId.get(PALETTE_ACTION_ID.navDashboard)?.title, "Dashboard");
  assert.equal(byId.get(PALETTE_ACTION_ID.navLibrary)?.title, "Library");
  assert.equal(byId.get(PALETTE_ACTION_ID.navRuns)?.title, "Runs");
  assert.equal(byId.get(PALETTE_ACTION_ID.navSettings)?.title, "Settings");
});

test("buildSettingsEntries: one entry per Settings section, all kind 'setting', unique ids, non-empty titles", () => {
  const entries = buildSettingsEntries();
  assert.equal(entries.length, 5);
  for (const e of entries) {
    assert.equal(e.kind, "setting");
    assert.ok(e.title.length > 0, `entry ${e.id} must have a title`);
  }
  const ids = entries.map((e) => e.id);
  assert.equal(new Set(ids).size, ids.length, "every setting id must be unique");
  assert.deepEqual(new Set(ids), new Set(Object.values(PALETTE_SETTING_ID)));
});

test("no id collides between quick actions and settings entries", () => {
  const ids = [...buildQuickActionEntries(), ...buildSettingsEntries()].map((e) => e.id);
  assert.equal(new Set(ids).size, ids.length);
});
