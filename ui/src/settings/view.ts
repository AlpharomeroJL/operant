// DOM mount for the Settings screen (docs/specs/design.md section 3.3:
// "Sidebar sections: General, Thinking engines (with probe badges), Voice,
// Privacy ..., Appearance ..., Advanced."). Pure DOM, no bus and no store
// access: same split as ui/src/render/workflowView.ts (callbacks in,
// elements out). main.ts owns wiring this to ./state.ts, the bus, and the
// theme/mode stores it does not own but reads for the Appearance and
// Advanced sections below.
//
// Only the active section's content is mounted at a time (the same "show
// one thing, not everything at once" idea the wizard's one-decision-per-
// screen restyle uses): main.ts keeps the active section in its own local
// variable, the same pattern it already uses for the outer app nav's
// activeScreen, and passes it back in as activeSection on every mount.

import "./settings.css";
import type { SettingsSnapshot } from "./state.ts";
import { settingsStrings, themeToggleStrings } from "../strings/default.ts";
import { settingsDetailStrings as D } from "./strings.ts";
import type { ThemeMode } from "../theme/store.ts";

export type SettingsSection = "general" | "engines" | "voice" | "privacy" | "appearance" | "advanced";

/** Sidebar order and labels, docs/specs/design.md section 3.3's own list order. */
export const SETTINGS_SECTIONS: ReadonlyArray<{ id: SettingsSection; label: string }> = [
  { id: "general", label: settingsStrings.generalSectionTitle },
  { id: "engines", label: settingsStrings.thinkingEnginesSectionTitle },
  { id: "voice", label: settingsStrings.voiceSectionTitle },
  { id: "privacy", label: settingsStrings.privacySectionTitle },
  { id: "appearance", label: settingsStrings.appearanceSectionTitle },
  { id: "advanced", label: settingsStrings.advancedSectionTitle },
];

export interface SettingsMountOptions {
  activeSection?: SettingsSection;
  onSelectSection?: (section: SettingsSection) => void;
  onVoiceToggle?: (on: boolean) => void;
  onSpeakingRateChange?: (rate: number) => void;
  onWatchAndSuggestToggle?: (on: boolean) => void;
  onPurge?: () => void;
  onStartChordRecording?: () => void;
  onCancelChordRecording?: () => void;
  onExportBackup?: () => void;
  onImportBackupFile?: (file: File) => void;
  onAutoUpdateToggle?: (on: boolean) => void;
  /** Appearance section: ui/src/theme/store.ts's current mode and setter. Read here, not owned here (that module is another lane's, ui/src/settings/* only consumes its public API). */
  themeMode?: ThemeMode;
  onSetTheme?: (mode: ThemeMode) => void;
  onAccentSyncToggle?: (on: boolean) => void;
  /** Advanced section: ui/src/state/mode.ts's current Default/Advanced mode, the same store the header's own toggle reads. */
  advancedModeOn?: boolean;
  onToggleAdvancedMode?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function sectionRoot(titleText: string): HTMLElement {
  const root = el("section", "op-settings__section");
  root.append(el("h3", "op-panel__title", titleText));
  return root;
}

function sectionBody(root: HTMLElement): HTMLElement {
  const body = el("div", "op-settings__section-body");
  root.append(body);
  return body;
}

function subsection(root: HTMLElement, titleText: string): HTMLElement {
  const wrap = el("div", "op-settings__subsection");
  wrap.append(el("h4", "op-settings__subsection-title", titleText));
  const body = el("div", "op-settings__section-body");
  wrap.append(body);
  root.append(wrap);
  return body;
}

function toggleRow(labelText: string, checked: boolean, onChange?: (on: boolean) => void): HTMLElement {
  const label = el("label", "op-settings__toggle");
  const checkbox = el("input");
  checkbox.type = "checkbox";
  checkbox.checked = checked;
  if (onChange) checkbox.addEventListener("change", () => onChange(checkbox.checked));
  label.append(checkbox, document.createTextNode(` ${labelText}`));
  return label;
}

function buildGeneralSection(snapshot: SettingsSnapshot, opts: SettingsMountOptions): HTMLElement {
  const root = sectionRoot(settingsStrings.generalSectionTitle);

  // Emergency stop shortcut.
  {
    const body = subsection(root, settingsStrings.killSwitchSectionTitle);
    body.append(el("p", "op-settings__chord", D.killSwitchCurrentLabel(snapshot.state.killSwitchChord)));
    if (snapshot.recordingChord) {
      body.append(el("p", "op-settings__hint", D.killSwitchRecordingHint));
      const cancel = el("button", "op-button", D.killSwitchCancelButton);
      cancel.type = "button";
      if (opts.onCancelChordRecording) cancel.addEventListener("click", opts.onCancelChordRecording);
      body.append(cancel);
    } else {
      const change = el("button", "op-button", D.killSwitchChangeButton);
      change.type = "button";
      if (opts.onStartChordRecording) change.addEventListener("click", opts.onStartChordRecording);
      body.append(change);
    }
  }

  // Backup and export.
  {
    const body = subsection(root, settingsStrings.backupSectionTitle);
    body.append(el("p", "op-settings__backup-label", snapshot.lastBackupLabel));

    const exportBtn = el("button", "op-button", D.backupExportButton);
    exportBtn.type = "button";
    if (opts.onExportBackup) exportBtn.addEventListener("click", opts.onExportBackup);

    const importBtn = el("button", "op-button", D.backupImportButton);
    importBtn.type = "button";
    const fileInput = el("input");
    fileInput.type = "file";
    fileInput.accept = "application/json";
    fileInput.hidden = true;
    fileInput.addEventListener("change", () => {
      const file = fileInput.files?.[0];
      if (file && opts.onImportBackupFile) opts.onImportBackupFile(file);
    });
    importBtn.addEventListener("click", () => fileInput.click());

    body.append(exportBtn, importBtn, fileInput);
  }

  // Updates.
  {
    const body = subsection(root, settingsStrings.updatesSectionTitle);
    body.append(toggleRow(D.autoUpdateToggle, snapshot.state.autoUpdateEnabled, opts.onAutoUpdateToggle));
    body.append(el("p", "op-settings__hint", D.autoUpdateHint));
  }

  return root;
}

function buildEnginesSection(snapshot: SettingsSnapshot): HTMLElement {
  const root = sectionRoot(settingsStrings.thinkingEnginesSectionTitle);
  const body = sectionBody(root);
  body.append(el("p", "op-settings__model-label", snapshot.state.modelLabel || D.modelNotConnected));

  if (snapshot.modelProfileBadges.length) {
    const badges = el("div", "op-settings__badges");
    for (const label of snapshot.modelProfileBadges) {
      badges.append(el("span", "op-badge", label));
    }
    body.append(badges);
  }

  for (const line of snapshot.modelProfileLines) body.append(el("p", "op-settings__profile-line", line));
  return root;
}

function buildVoiceSection(snapshot: SettingsSnapshot, opts: SettingsMountOptions): HTMLElement {
  const root = sectionRoot(settingsStrings.voiceSectionTitle);
  const body = sectionBody(root);
  body.append(toggleRow(D.voiceEnableToggle, snapshot.state.voiceEnabled, opts.onVoiceToggle));

  const keyLabel = el("label", "op-field__label", D.pushToTalkLabel);
  const keyValue = el("span", "op-field__input", snapshot.state.pushToTalkKey);
  keyLabel.append(keyValue);
  body.append(keyLabel);

  const rateLabel = el("label", "op-field__label", D.speakingRateLabel);
  const rateInput = el("input", "op-field__input");
  rateInput.type = "range";
  rateInput.min = "0.5";
  rateInput.max = "2";
  rateInput.step = "0.1";
  rateInput.value = String(snapshot.state.speakingRate);
  if (opts.onSpeakingRateChange) {
    rateInput.addEventListener("change", () => opts.onSpeakingRateChange?.(Number(rateInput.value)));
  }
  rateLabel.append(rateInput);
  body.append(rateLabel);

  return root;
}

function buildPrivacySection(snapshot: SettingsSnapshot, opts: SettingsMountOptions): HTMLElement {
  const root = sectionRoot(settingsStrings.privacySectionTitle);
  const body = sectionBody(root);
  body.append(toggleRow(settingsStrings.watchAndSuggestToggle, snapshot.state.watchAndSuggestEnabled, opts.onWatchAndSuggestToggle));

  const purge = el("button", "op-button", settingsStrings.purgeButton);
  purge.type = "button";
  if (opts.onPurge) purge.addEventListener("click", opts.onPurge);
  body.append(purge);

  return root;
}

function buildAppearanceSection(snapshot: SettingsSnapshot, opts: SettingsMountOptions): HTMLElement {
  const root = sectionRoot(settingsStrings.appearanceSectionTitle);
  const body = sectionBody(root);

  body.append(el("p", "op-field__label", D.appearanceThemeLabel));
  const swatches = el("div", "op-settings__swatches");
  swatches.setAttribute("role", "group");
  swatches.setAttribute("aria-label", D.appearanceThemeLabel);
  const modes: ReadonlyArray<{ id: ThemeMode; label: string }> = [
    { id: "dark", label: themeToggleStrings.dark },
    { id: "light", label: themeToggleStrings.light },
    { id: "system", label: themeToggleStrings.system },
  ];
  for (const mode of modes) {
    const btn = el("button", "op-settings__swatch", mode.label);
    btn.type = "button";
    btn.setAttribute("aria-pressed", String(opts.themeMode === mode.id));
    if (opts.onSetTheme) btn.addEventListener("click", () => opts.onSetTheme?.(mode.id));
    swatches.append(btn);
  }
  body.append(swatches);

  body.append(toggleRow(D.accentSyncToggle, snapshot.state.accentSyncEnabled, opts.onAccentSyncToggle));

  return root;
}

function buildAdvancedSection(opts: SettingsMountOptions): HTMLElement {
  const root = sectionRoot(settingsStrings.advancedSectionTitle);
  const body = sectionBody(root);
  body.append(el("p", "op-settings__hint", D.advancedSectionBody));

  if (opts.advancedModeOn) {
    body.append(el("p", "op-settings__hint", D.advancedOnHint));
    const off = el("button", "op-button", D.advancedCloseButton);
    off.type = "button";
    if (opts.onToggleAdvancedMode) off.addEventListener("click", opts.onToggleAdvancedMode);
    body.append(off);
  } else {
    const on = el("button", "op-button", D.advancedOpenButton);
    on.type = "button";
    if (opts.onToggleAdvancedMode) on.addEventListener("click", opts.onToggleAdvancedMode);
    body.append(on);
  }

  return root;
}

function buildSectionContent(activeSection: SettingsSection, snapshot: SettingsSnapshot, opts: SettingsMountOptions): HTMLElement {
  switch (activeSection) {
    case "general":
      return buildGeneralSection(snapshot, opts);
    case "engines":
      return buildEnginesSection(snapshot);
    case "voice":
      return buildVoiceSection(snapshot, opts);
    case "privacy":
      return buildPrivacySection(snapshot, opts);
    case "appearance":
      return buildAppearanceSection(snapshot, opts);
    case "advanced":
      return buildAdvancedSection(opts);
    default: {
      const exhaustive: never = activeSection;
      return exhaustive;
    }
  }
}

export function mountSettings(container: HTMLElement, snapshot: SettingsSnapshot, opts: SettingsMountOptions = {}): HTMLElement {
  container.textContent = "";
  const activeSection = opts.activeSection ?? "general";

  const root = el("section", "op-settings");
  root.setAttribute("aria-labelledby", "op-settings-heading");
  // Visually hidden, not removed: the region's accessible name still comes
  // from here (aria-labelledby above), while the visible heading a sighted
  // person actually reads is the active section's own name below (built by
  // sectionRoot), which is more specific and already visible right next to
  // the sidebar button that is pressed for it.
  const heading = el("h2", "op-panel__title op-visually-hidden", settingsStrings.title);
  heading.id = "op-settings-heading";
  root.append(heading);

  const nav = el("nav", "op-settings__nav");
  nav.setAttribute("aria-label", settingsStrings.title);
  for (const entry of SETTINGS_SECTIONS) {
    const btn = el("button", "op-settings__nav-button", entry.label);
    btn.type = "button";
    btn.setAttribute("aria-pressed", String(entry.id === activeSection));
    if (opts.onSelectSection) btn.addEventListener("click", () => opts.onSelectSection?.(entry.id));
    nav.append(btn);
  }
  root.append(nav);

  const content = el("div", "op-settings__content");
  content.append(buildSectionContent(activeSection, snapshot, opts));
  root.append(content);

  container.append(root);
  return root;
}
