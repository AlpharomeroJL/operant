// DOM mount for the onboarding wizard. Pure DOM, no bus and no wizard state
// access: same split as ui/src/render/workflowView.ts and every other
// view.ts in ui/src (callbacks in, elements out). ui/src/main.ts owns wiring
// this to ./state.ts.
//
// Every screen renders unconditionally from the snapshot it is given (no
// screen returns early with nothing appended), which is what makes the
// media-presence check (./mediaPresence.ts) a meaningful regression guard
// rather than a check the view can quietly dodge.

import "./wizard.css";
import type { WizardSnapshot, ScheduleOptionId, LocalCardSnapshot, AccessKeyCardSnapshot, StaticCardSnapshot } from "./state.ts";
import type { Provider } from "./accessKey.ts";
import { wizardShellStrings } from "./strings.ts";

export interface WizardMountOptions {
  onContinueWelcome?: () => void;
  onChooseChatGPT?: () => void;
  onChooseClaude?: () => void;
  onStartLocalDownload?: () => void;
  onPauseLocalDownload?: () => void;
  onResumeLocalDownload?: () => void;
  onCancelLocalDownload?: () => void;
  onContinueAfterLocalDownload?: () => void;
  onAccessKeyTextChange?: (text: string) => void;
  onChooseProviderManually?: (provider: Provider) => void;
  onContinueWithAccessKey?: () => void;
  onStartDemo?: () => void;
  onPlayMicSample?: () => void;
  onSkipMicCheck?: () => void;
  onContinueMicCheck?: () => void;
  onSaveAsWorkflow?: () => void;
  onContinueAfterDemo?: () => void;
  onChooseSchedule?: (id: ScheduleOptionId) => void;
  onFinishSchedule?: () => void;
}

const SCREEN_ORDER: WizardSnapshot["screen"][] = ["welcome", "setup_path", "mic_check", "guided_task", "schedule"];

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function button(label: string, className: string, onClick?: () => void, disabled = false): HTMLButtonElement {
  const b = el("button", className, label);
  b.type = "button";
  b.disabled = disabled;
  if (onClick) b.addEventListener("click", onClick);
  return b;
}

function renderWelcome(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const root = el("section", "op-wizard__screen");
  root.append(el("h2", "op-panel__title", snap.welcome.heading));
  root.append(el("p", "op-wizard__body", snap.welcome.body));
  root.append(button(snap.welcome.continueButton, "op-button op-button--primary", opts.onContinueWelcome));
  return root;
}

function staticCard(card: StaticCardSnapshot, onClick?: () => void): HTMLElement {
  const root = el("article", "op-wizard-card");
  root.append(el("h3", "op-wizard-card__title", card.title));
  root.append(el("p", "op-wizard-card__body", card.body));
  root.append(button(card.button, "op-button op-button--primary", onClick));
  return root;
}

function localCard(card: LocalCardSnapshot, opts: WizardMountOptions): HTMLElement {
  const root = el("article", "op-wizard-card");
  root.append(el("h3", "op-wizard-card__title", card.title));
  root.append(el("p", "op-wizard-card__body", card.body));

  if (card.diskLabel) root.append(el("p", "op-wizard-card__check", card.diskLabel));
  if (card.compatLabel) root.append(el("p", "op-wizard-card__check", card.compatLabel));

  if (card.phase === "downloading" || card.phase === "paused" || card.phase === "verifying" || card.phase === "resuming") {
    const track = el("div", "op-progress");
    track.setAttribute("role", "progressbar");
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", "100");
    track.setAttribute("aria-valuenow", String(card.percent));
    const fill = el("div", "op-progress__fill");
    fill.style.width = `${card.percent}%`;
    track.append(fill);
    root.append(track);
  }

  if (card.progressLabel) root.append(el("p", "op-wizard-card__status", card.progressLabel));

  if (card.errorWhat) {
    const err = el("div", "op-wizard-card__error");
    err.append(el("p", "op-wizard-card__error-what", card.errorWhat));
    if (card.errorWhy) err.append(el("p", "op-wizard-card__error-why", card.errorWhy));
    if (card.errorAction) err.append(el("p", "op-wizard-card__error-action", card.errorAction));
    root.append(err);
  }

  const actions = el("div", "op-wizard-card__actions");
  if (card.showPauseResume) {
    actions.append(
      button(card.pauseResumeLabel, "op-button", card.phase === "paused" ? opts.onResumeLocalDownload : opts.onPauseLocalDownload),
    );
  }
  if (card.showCancel) {
    actions.append(button(card.cancelLabel, "op-button", opts.onCancelLocalDownload));
  }
  if (!card.showPauseResume && !card.showCancel && !card.showContinue) {
    actions.append(button(card.buttonLabel, "op-button op-button--primary", opts.onStartLocalDownload, card.buttonDisabled));
  }
  if (card.showContinue) {
    actions.append(button(card.continueLabel, "op-button op-button--primary", opts.onContinueAfterLocalDownload));
  }
  root.append(actions);

  return root;
}

function accessKeyCard(card: AccessKeyCardSnapshot, opts: WizardMountOptions): HTMLElement {
  const root = el("article", "op-wizard-card");
  root.append(el("h3", "op-wizard-card__title", card.title));
  root.append(el("p", "op-wizard-card__body", card.body));

  const input = el("input", "op-field__input");
  input.type = "password";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.placeholder = card.placeholder;
  input.value = card.text;
  input.setAttribute("aria-label", card.placeholder);
  if (opts.onAccessKeyTextChange) {
    input.addEventListener("input", () => opts.onAccessKeyTextChange?.(input.value));
  }
  root.append(input);

  if (card.detectedLabel) root.append(el("p", "op-wizard-card__check", card.detectedLabel));

  if (card.showManualPicker) {
    const pickerLabel = el("label", "op-field__label", card.providerLabel);
    const select = el("select", "op-field__input");
    select.id = "op-wizard-access-key-provider";
    pickerLabel.htmlFor = select.id;
    const blank = el("option", undefined, "");
    blank.value = "";
    select.append(blank);
    for (const [value, text] of [
      ["chatgpt", "ChatGPT"],
      ["claude", "Claude"],
    ] as const) {
      const option = el("option", undefined, text);
      option.value = value;
      option.selected = card.manual === value;
      select.append(option);
    }
    if (opts.onChooseProviderManually) {
      select.addEventListener("change", () => {
        if (select.value === "chatgpt" || select.value === "claude") opts.onChooseProviderManually?.(select.value);
      });
    }
    root.append(pickerLabel, select);
  }

  root.append(button(card.buttonLabel, "op-button op-button--primary", opts.onContinueWithAccessKey, card.buttonDisabled));
  return root;
}

function renderSetupPath(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const root = el("section", "op-wizard__screen");
  root.append(el("h2", "op-panel__title", snap.setupPath.heading));
  root.append(el("p", "op-wizard__body", snap.setupPath.subheading));

  const grid = el("div", "op-wizard-card__grid");
  grid.append(staticCard(snap.setupPath.chatgpt, opts.onChooseChatGPT));
  grid.append(staticCard(snap.setupPath.claude, opts.onChooseClaude));
  grid.append(localCard(snap.setupPath.local, opts));
  grid.append(accessKeyCard(snap.setupPath.accessKey, opts));
  root.append(grid);

  const demoLink = el("button", "op-wizard__demo-link", snap.setupPath.demoLink);
  demoLink.type = "button";
  if (opts.onStartDemo) demoLink.addEventListener("click", opts.onStartDemo);
  root.append(demoLink);

  return root;
}

function renderMicCheck(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const m = snap.micCheck;
  const root = el("section", "op-wizard__screen");
  root.append(el("h2", "op-panel__title", m.heading));
  root.append(el("p", "op-wizard__body", m.body));
  root.append(button(m.sampleButton, "op-button", opts.onPlayMicSample));

  const meter = el("div", "op-mic-meter");
  meter.setAttribute("role", "meter");
  meter.setAttribute("aria-label", m.levelMeterLabel);
  meter.setAttribute("aria-valuemin", "0");
  meter.setAttribute("aria-valuemax", "1");
  meter.setAttribute("aria-valuenow", String(m.level));
  const meterFill = el("div", "op-mic-meter__fill");
  meterFill.style.width = `${Math.round(m.level * 100)}%`;
  meter.append(meterFill);
  root.append(el("p", "op-field__label", m.levelMeterLabel));
  root.append(meter);

  const actions = el("div", "op-wizard-card__actions");
  actions.append(button(m.skipButton, "op-button", opts.onSkipMicCheck));
  actions.append(button(m.continueButton, "op-button op-button--primary", opts.onContinueMicCheck));
  root.append(actions);

  return root;
}

function renderGuidedTask(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const g = snap.guidedTask;
  const root = el("section", "op-wizard__screen");
  root.append(el("h2", "op-panel__title", g.heading));
  root.append(el("p", "op-wizard__body", g.intro));

  root.append(el("p", "op-status", g.done ? g.doneLabel : g.runningLabel));

  const list = el("ol", "op-step-list");
  for (const step of g.steps) {
    const li = el("li", "op-step");
    const dot = el("span", "op-status__dot");
    dot.dataset.state = step.status;
    dot.setAttribute("aria-hidden", "true");
    const label = el("span", "op-step__sentence", step.sentence);
    li.append(dot, label);
    list.append(li);
  }
  root.append(list);

  if (g.demo) {
    if (g.canContinueDemo) {
      root.append(el("p", "op-wizard-card__check", g.demoContinueHint));
      root.append(button(g.demoContinueButton, "op-button op-button--primary", opts.onContinueAfterDemo));
    }
  } else if (g.saved) {
    root.append(el("p", "op-wizard-card__check", g.savedHint));
  } else if (g.canSave) {
    root.append(button(g.saveButton, "op-button op-button--primary", opts.onSaveAsWorkflow));
  }

  return root;
}

function renderSchedule(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const s = snap.schedule;
  const root = el("section", "op-wizard__screen");
  root.append(el("h2", "op-panel__title", s.heading));
  root.append(el("p", "op-wizard__body", s.body));

  const list = el("div", "op-wizard-schedule__options");
  for (const option of s.options) {
    const optionLabel = el("label", "op-wizard-schedule__option");
    const radio = el("input");
    radio.type = "radio";
    radio.name = "op-wizard-schedule";
    radio.value = option.id;
    radio.checked = s.selected === option.id;
    if (opts.onChooseSchedule) {
      radio.addEventListener("change", () => opts.onChooseSchedule?.(option.id));
    }
    optionLabel.append(radio, document.createTextNode(` ${option.label}`));
    list.append(optionLabel);
  }
  root.append(list);

  root.append(button(s.continueButton, "op-button op-button--primary", opts.onFinishSchedule, !s.canContinue));
  return root;
}

/**
 * Mount the wizard into `container`. Clears the container first so it can be
 * re-mounted on every snapshot change, the same pattern every screen in
 * ui/src uses. Always renders exactly one non-empty screen: there is no
 * branch here that leaves `container` empty, which is what
 * ./mediaPresence.ts's check is standing guard for.
 */
export function mountWizard(container: HTMLElement, snapshot: WizardSnapshot, opts: WizardMountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("div", "op-wizard");
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-modal", "true");
  root.setAttribute("aria-label", wizardShellStrings.dialogLabel);

  const stepIndex = SCREEN_ORDER.indexOf(snapshot.screen);
  const progress = el("p", "op-wizard__step", wizardShellStrings.stepLabel(stepIndex + 1, SCREEN_ORDER.length));
  root.append(progress);

  switch (snapshot.screen) {
    case "welcome":
      root.append(renderWelcome(snapshot, opts));
      break;
    case "setup_path":
      root.append(renderSetupPath(snapshot, opts));
      break;
    case "mic_check":
      root.append(renderMicCheck(snapshot, opts));
      break;
    case "guided_task":
      root.append(renderGuidedTask(snapshot, opts));
      break;
    case "schedule":
      root.append(renderSchedule(snapshot, opts));
      break;
  }

  container.append(root);
  return root;
}
