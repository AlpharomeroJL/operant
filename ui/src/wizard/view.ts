// DOM mount for the onboarding wizard. Pure DOM, no bus and no wizard state
// access: same split as ui/src/render/workflowView.ts and every other
// view.ts in ui/src (callbacks in, elements out). ui/src/main.ts owns wiring
// this to ./state.ts.
//
// Every screen renders unconditionally from the snapshot it is given (no
// screen returns early with nothing appended), which is what makes the
// media-presence check (./mediaPresence.ts) a meaningful regression guard
// rather than a check the view can quietly dodge.
//
// Accessibility (X8): this is a `role="dialog" aria-modal="true"` surface,
// so it owns three things the WAI-ARIA APG modal dialog pattern requires
// and plain browsers do not give a rebuild-every-snapshot view for free:
// (1) focus does not survive a DOM rebuild by default (the focused node is
// destroyed), so every focusable control here is tagged with a
// ui/src/styles/focusPreserve.ts focus key and mountWizard restores it after
// rebuilding; (2) focus moves onto the new screen's heading when the screen
// itself changes, so a keyboard/AT user lands somewhere meaningful instead
// of wherever focus happened to be (or nowhere, on first mount); (3) Tab is
// trapped inside the dialog so it cannot walk out into whatever the modal
// backdrop is covering.

import "./wizard.css";
import type { WizardSnapshot, ScheduleOptionId, LocalCardSnapshot, AccessKeyCardSnapshot, StaticCardSnapshot } from "./state.ts";
import type { Provider } from "./accessKey.ts";
import { wizardShellStrings } from "./strings.ts";
import { captureFocus, restoreFocus, focusOnSectionChange, trapFocus, FOCUS_KEY_ATTR } from "../styles/focusPreserve.ts";

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

// The wizard has exactly one Escape-worthy affordance: canceling an
// in-progress local model download (the one operation a person might start
// then want to immediately back out of). Every other screen has no
// cancel-everything action by design (onboarding is a short, linear,
// mandatory path), so Escape intentionally does nothing there rather than
// inventing a dismiss behavior the product does not otherwise offer.
const ESCAPE_ACTION_ATTR = "data-op-escape-action";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function button(label: string, className: string, onClick?: () => void, disabled = false, focusKey?: string): HTMLButtonElement {
  const b = el("button", className, label);
  b.type = "button";
  b.disabled = disabled;
  if (focusKey) b.setAttribute(FOCUS_KEY_ATTR, focusKey);
  if (onClick) b.addEventListener("click", onClick);
  return b;
}

/** A screen's top heading: the section-change focus target (WAI-ARIA APG: move focus into new dialog content), so it needs tabindex="-1" to be programmatically focusable without joining the Tab order. */
function screenHeading(text: string): HTMLHeadingElement {
  const h = el("h2", "op-panel__title", text);
  h.tabIndex = -1;
  h.setAttribute("data-op-section-focus", "");
  return h;
}

function renderWelcome(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const root = el("section", "op-wizard__screen");
  root.append(screenHeading(snap.welcome.heading));
  root.append(el("p", "op-wizard__body", snap.welcome.body));
  root.append(button(snap.welcome.continueButton, "op-button op-button--primary", opts.onContinueWelcome, false, "welcome-continue"));
  return root;
}

function staticCard(card: StaticCardSnapshot, focusKey: string, onClick?: () => void): HTMLElement {
  const root = el("article", "op-wizard-card");
  root.append(el("h3", "op-wizard-card__title", card.title));
  root.append(el("p", "op-wizard-card__body", card.body));
  root.append(button(card.button, "op-button op-button--primary", onClick, false, focusKey));
  return root;
}

function localCard(card: LocalCardSnapshot, opts: WizardMountOptions): HTMLElement {
  const root = el("article", "op-wizard-card");
  const titleId = "op-wizard-local-card-title";
  const title = el("h3", "op-wizard-card__title", card.title);
  title.id = titleId;
  root.append(title);
  root.append(el("p", "op-wizard-card__body", card.body));

  if (card.diskLabel) root.append(el("p", "op-wizard-card__check", card.diskLabel));
  if (card.compatLabel) root.append(el("p", "op-wizard-card__check", card.compatLabel));

  if (card.phase === "downloading" || card.phase === "paused" || card.phase === "verifying" || card.phase === "resuming") {
    const track = el("div", "op-progress");
    track.setAttribute("role", "progressbar");
    // WCAG 4.1.2 / axe's aria-progressbar-name: a progressbar needs an
    // accessible name of its own, distinct from the percent value it
    // already exposes via aria-valuenow. The card's own title ("Set up a
    // model on this device") is exactly what is downloading, so point at it
    // rather than inventing new copy.
    track.setAttribute("aria-labelledby", titleId);
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", "100");
    track.setAttribute("aria-valuenow", String(card.percent));
    const fill = el("div", "op-progress__fill");
    fill.style.width = `${card.percent}%`;
    track.append(fill);
    root.append(track);
  }

  if (card.progressLabel) {
    const status = el("p", "op-wizard-card__status", card.progressLabel);
    // The download's own words change as it moves through starting,
    // downloading (with a live percent), verifying, and so on: a live
    // region so a screen-reader user hears the progress without having to
    // keep re-navigating back to this paragraph.
    status.setAttribute("aria-live", "polite");
    status.setAttribute("aria-atomic", "true");
    root.append(status);
  }

  if (card.errorWhat) {
    const err = el("div", "op-wizard-card__error");
    // What happened, why, and the one suggested action, announced as soon
    // as it appears (operant-ux: every error states all three in one
    // sentence each) rather than requiring the person to notice and
    // navigate to it themselves.
    err.setAttribute("role", "alert");
    err.append(el("p", "op-wizard-card__error-what", card.errorWhat));
    if (card.errorWhy) err.append(el("p", "op-wizard-card__error-why", card.errorWhy));
    if (card.errorAction) err.append(el("p", "op-wizard-card__error-action", card.errorAction));
    root.append(err);
  }

  const actions = el("div", "op-wizard-card__actions");
  if (card.showPauseResume) {
    const pauseResume = button(
      card.pauseResumeLabel,
      "op-button",
      card.phase === "paused" ? opts.onResumeLocalDownload : opts.onPauseLocalDownload,
      false,
      "setup-local-pause-resume",
    );
    actions.append(pauseResume);
  }
  if (card.showCancel) {
    const cancel = button(card.cancelLabel, "op-button", opts.onCancelLocalDownload, false, "setup-local-cancel");
    // The one Escape-able action in the wizard (see ESCAPE_ACTION_ATTR
    // above): Escape while a download is cancelable triggers this same
    // button, matching the WAI-ARIA APG expectation that Escape backs out
    // of the current operation when the surface offers one.
    cancel.setAttribute(ESCAPE_ACTION_ATTR, "cancel-local-download");
    actions.append(cancel);
  }
  if (!card.showPauseResume && !card.showCancel && !card.showContinue) {
    actions.append(button(card.buttonLabel, "op-button op-button--primary", opts.onStartLocalDownload, card.buttonDisabled, "setup-local-primary"));
  }
  if (card.showContinue) {
    actions.append(button(card.continueLabel, "op-button op-button--primary", opts.onContinueAfterLocalDownload, false, "setup-local-continue"));
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
  input.setAttribute(FOCUS_KEY_ATTR, "setup-access-key-input");
  if (opts.onAccessKeyTextChange) {
    input.addEventListener("input", () => opts.onAccessKeyTextChange?.(input.value));
  }
  root.append(input);

  if (card.detectedLabel) {
    const detected = el("p", "op-wizard-card__check", card.detectedLabel);
    detected.setAttribute("aria-live", "polite");
    root.append(detected);
  }

  if (card.showManualPicker) {
    const pickerLabel = el("label", "op-field__label", card.providerLabel);
    const select = el("select", "op-field__input");
    select.id = "op-wizard-access-key-provider";
    select.setAttribute(FOCUS_KEY_ATTR, "setup-access-key-provider");
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

  root.append(
    button(card.buttonLabel, "op-button op-button--primary", opts.onContinueWithAccessKey, card.buttonDisabled, "setup-access-key-continue"),
  );
  return root;
}

function renderSetupPath(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const root = el("section", "op-wizard__screen");
  root.append(screenHeading(snap.setupPath.heading));
  root.append(el("p", "op-wizard__body", snap.setupPath.subheading));

  const grid = el("div", "op-wizard-card__grid");
  grid.append(staticCard(snap.setupPath.chatgpt, "setup-chatgpt", opts.onChooseChatGPT));
  grid.append(staticCard(snap.setupPath.claude, "setup-claude", opts.onChooseClaude));
  grid.append(localCard(snap.setupPath.local, opts));
  grid.append(accessKeyCard(snap.setupPath.accessKey, opts));
  root.append(grid);

  const demoLink = el("button", "op-wizard__demo-link", snap.setupPath.demoLink);
  demoLink.type = "button";
  demoLink.setAttribute(FOCUS_KEY_ATTR, "setup-demo-link");
  if (opts.onStartDemo) demoLink.addEventListener("click", opts.onStartDemo);
  root.append(demoLink);

  return root;
}

function renderMicCheck(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const m = snap.micCheck;
  const root = el("section", "op-wizard__screen");
  root.append(screenHeading(m.heading));
  root.append(el("p", "op-wizard__body", m.body));
  root.append(button(m.sampleButton, "op-button", opts.onPlayMicSample, false, "mic-sample-button"));

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
  actions.append(button(m.skipButton, "op-button", opts.onSkipMicCheck, false, "mic-skip"));
  actions.append(button(m.continueButton, "op-button op-button--primary", opts.onContinueMicCheck, false, "mic-continue"));
  root.append(actions);

  return root;
}

function renderGuidedTask(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const g = snap.guidedTask;
  const root = el("section", "op-wizard__screen");
  root.append(screenHeading(g.heading));
  root.append(el("p", "op-wizard__body", g.intro));

  const status = el("p", "op-status", g.done ? g.doneLabel : g.runningLabel);
  // Running -> done is the one state change on this screen that is not
  // already covered by the step list's own live region below; announce it
  // the same way.
  status.setAttribute("aria-live", "polite");
  root.append(status);

  const list = el("ol", "op-step-list");
  // Steps stream in one at a time while the guided task runs
  // (ui/src/wizard/guidedTask.ts against the same bus a live run would
  // use). "additions text" tells a screen reader to announce a newly added
  // row's text and an existing row's text if it changes (a step moving from
  // pending to done), without re-reading the whole list on every update.
  list.setAttribute("aria-live", "polite");
  list.setAttribute("aria-relevant", "additions text");
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
      root.append(button(g.demoContinueButton, "op-button op-button--primary", opts.onContinueAfterDemo, false, "guided-demo-continue"));
    }
  } else if (g.saved) {
    root.append(el("p", "op-wizard-card__check", g.savedHint));
  } else if (g.canSave) {
    root.append(button(g.saveButton, "op-button op-button--primary", opts.onSaveAsWorkflow, false, "guided-save"));
  }

  return root;
}

function renderSchedule(snap: WizardSnapshot, opts: WizardMountOptions): HTMLElement {
  const s = snap.schedule;
  const root = el("section", "op-wizard__screen");
  root.append(screenHeading(s.heading));
  root.append(el("p", "op-wizard__body", s.body));

  const list = el("div", "op-wizard-schedule__options");
  list.setAttribute("role", "radiogroup");
  list.setAttribute("aria-label", s.heading);
  for (const option of s.options) {
    const optionLabel = el("label", "op-wizard-schedule__option");
    const radio = el("input");
    radio.type = "radio";
    radio.name = "op-wizard-schedule";
    radio.value = option.id;
    radio.checked = s.selected === option.id;
    radio.setAttribute(FOCUS_KEY_ATTR, `schedule-option-${option.id}`);
    if (opts.onChooseSchedule) {
      radio.addEventListener("change", () => opts.onChooseSchedule?.(option.id));
    }
    optionLabel.append(radio, document.createTextNode(` ${option.label}`));
    list.append(optionLabel);
  }
  root.append(list);

  root.append(button(s.continueButton, "op-button op-button--primary", opts.onFinishSchedule, !s.canContinue, "schedule-continue"));
  return root;
}

/**
 * Mount the wizard into `container`. Clears the container first so it can be
 * re-mounted on every snapshot change, the same pattern every screen in
 * ui/src uses. Always renders exactly one non-empty screen: there is no
 * branch here that leaves `container` empty, which is what
 * ./mediaPresence.ts's check is standing guard for.
 *
 * Also carries keyboard focus across that rebuild (captureFocus/
 * restoreFocus), moves focus onto the new screen's heading when the screen
 * itself changes (focusOnSectionChange), traps Tab inside the dialog
 * (trapFocus), and maps Escape onto the one cancel action the wizard offers
 * (the local-download Cancel button, when it is on screen). See this file's
 * header comment for why a rebuild-every-snapshot dialog needs all four.
 */
export function mountWizard(container: HTMLElement, snapshot: WizardSnapshot, opts: WizardMountOptions = {}): HTMLElement {
  const captured = captureFocus(container);
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

  const restored = restoreFocus(container, captured);
  if (!restored) focusOnSectionChange(container, snapshot.screen);

  trapFocus(root);
  root.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    const escapeTarget = root.querySelector<HTMLElement>(`[${ESCAPE_ACTION_ATTR}]`);
    escapeTarget?.click();
  });

  return root;
}
