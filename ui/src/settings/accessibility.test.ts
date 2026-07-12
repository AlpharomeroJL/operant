// X8 app-accessibility bar for the restyled Settings screen (docs/specs/
// design.md section 3.3): an axe-core scan of every sidebar section, plus
// the couple of conditional branches within them (chord recording in
// progress, a connected model with probe badges, Advanced mode already on).
// Same pattern as ui/src/dashboard/accessibility.test.ts and
// ui/src/wizard/accessibility.test.ts.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createSettings } from "./state.ts";
import { mountSettings, SETTINGS_SECTIONS, type SettingsSection } from "./view.ts";
import type { BackendProfile } from "./backendProfile.ts";

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

function mount(section: SettingsSection) {
  const env = createDomEnv();
  const settings = createSettings();
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  mountSettings(container, settings.getSnapshot(), {
    activeSection: section,
    themeMode: "system",
    advancedModeOn: false,
  });
  return { env, settings, container };
}

test("every sidebar section renders with no axe violations", async () => {
  for (const entry of SETTINGS_SECTIONS) {
    const { env, settings, container } = mount(entry.id);
    try {
      await assertNoViolations(container, `settings section: ${entry.id}`);
    } finally {
      settings.dispose();
      env.cleanup();
    }
  }
});

test("General mid chord-recording (the cancel button and recording hint visible): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const settings = createSettings();
    settings.startChordRecording();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountSettings(container, settings.getSnapshot(), { activeSection: "general" });
    await assertNoViolations(container, "settings General, recording a new shortcut");
    settings.dispose();
  } finally {
    env.cleanup();
  }
});

test("Thinking engines with a connected model (probe badges rendered): no axe violations", async () => {
  const env = createDomEnv();
  try {
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
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountSettings(container, settings.getSnapshot(), { activeSection: "engines" });

    const badges = root.querySelectorAll(".op-settings__badges .op-badge");
    assert.equal(badges.length, 4, "a fully-capable probed model shows all four badges");

    await assertNoViolations(container, "settings Thinking engines, connected");
    settings.dispose();
  } finally {
    env.cleanup();
  }
});

test("Advanced mode already on (the off-switch and its hint visible): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const settings = createSettings();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountSettings(container, settings.getSnapshot(), { activeSection: "advanced", advancedModeOn: true });
    await assertNoViolations(container, "settings Advanced, already on");
    settings.dispose();
  } finally {
    env.cleanup();
  }
});

test("the sidebar nav marks exactly the active section as pressed, and switching sections calls back with its id", () => {
  const env = createDomEnv();
  try {
    const settings = createSettings();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const selections: SettingsSection[] = [];
    const root = mountSettings(container, settings.getSnapshot(), {
      activeSection: "voice",
      onSelectSection: (section) => selections.push(section),
    });

    const buttons = Array.from(root.querySelectorAll<HTMLButtonElement>(".op-settings__nav-button"));
    assert.equal(buttons.length, SETTINGS_SECTIONS.length);
    const pressed = buttons.filter((b) => b.getAttribute("aria-pressed") === "true");
    assert.equal(pressed.length, 1);
    assert.equal(pressed[0]?.textContent, "Voice");

    const advancedButton = buttons.find((b) => b.textContent === "Advanced");
    advancedButton?.click();
    assert.deepEqual(selections, ["advanced"]);

    settings.dispose();
  } finally {
    env.cleanup();
  }
});

test("only the active section's content is mounted, not every section at once", () => {
  const env = createDomEnv();
  try {
    const settings = createSettings();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const root = mountSettings(container, settings.getSnapshot(), { activeSection: "privacy" });

    assert.ok(root.querySelector(".op-settings__content")?.textContent?.includes("Watch for repeated actions"));
    assert.equal(root.querySelector(".op-settings__content")?.textContent?.includes("Color theme"), false);

    settings.dispose();
  } finally {
    env.cleanup();
  }
});
