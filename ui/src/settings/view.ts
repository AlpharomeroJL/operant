// DOM mount for the Settings screen (docs/specs/ui.md). Pure DOM, no bus and
// no store access: same split as ui/src/render/workflowView.ts (callbacks
// in, elements out). main.ts owns wiring this to ./state.ts and the bus.

import type { SettingsSnapshot } from "./state.ts";
import { settingsStrings } from "../strings/default.ts";
import { settingsDetailStrings as D } from "./strings.ts";

export interface SettingsMountOptions {
  onVoiceToggle?: (on: boolean) => void;
  onSpeakingRateChange?: (rate: number) => void;
  onWatchAndSuggestToggle?: (on: boolean) => void;
  onPurge?: () => void;
  onStartChordRecording?: () => void;
  onCancelChordRecording?: () => void;
  onExportBackup?: () => void;
  onImportBackupFile?: (file: File) => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function section(titleText: string): { root: HTMLElement; body: HTMLElement } {
  const root = el("section", "op-settings__section");
  root.append(el("h3", "op-panel__title", titleText));
  const body = el("div", "op-settings__section-body");
  root.append(body);
  return { root, body };
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

export function mountSettings(container: HTMLElement, snapshot: SettingsSnapshot, opts: SettingsMountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("section", "op-settings");
  root.setAttribute("aria-labelledby", "op-settings-heading");
  const heading = el("h2", "op-panel__title", settingsStrings.title);
  heading.id = "op-settings-heading";
  root.append(heading);

  // Model.
  {
    const { root: sec, body } = section(settingsStrings.modelSectionTitle);
    body.append(el("p", "op-settings__model-label", snapshot.state.modelLabel || D.modelNotConnected));
    for (const line of snapshot.modelProfileLines) body.append(el("p", "op-settings__profile-line", line));
    root.append(sec);
  }

  // Voice.
  {
    const { root: sec, body } = section(settingsStrings.voiceSectionTitle);
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

    root.append(sec);
  }

  // Kill switch.
  {
    const { root: sec, body } = section(settingsStrings.killSwitchSectionTitle);
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
    root.append(sec);
  }

  // Privacy.
  {
    const { root: sec, body } = section(settingsStrings.privacySectionTitle);
    body.append(toggleRow(settingsStrings.watchAndSuggestToggle, snapshot.state.watchAndSuggestEnabled, opts.onWatchAndSuggestToggle));

    const purge = el("button", "op-button", settingsStrings.purgeButton);
    purge.type = "button";
    if (opts.onPurge) purge.addEventListener("click", opts.onPurge);
    body.append(purge);

    root.append(sec);
  }

  // Backup and export.
  {
    const { root: sec, body } = section(settingsStrings.backupSectionTitle);
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
    root.append(sec);
  }

  container.append(root);
  return root;
}
