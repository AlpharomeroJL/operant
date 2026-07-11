import "./styles/base.css";
import { modeStore, type UiMode } from "./state/mode.ts";
import { createMockBusClient } from "./bus/mockClient.ts";
import type { BusEvent } from "./bus/types.ts";
import { isGlobalPaletteHotkey, submitGoal } from "./palette/palette.ts";
import { createRunViewer } from "./runViewer/state.ts";
import { paletteStrings, runViewerStrings, commonStrings } from "./strings/default.ts";
import { advancedStrings } from "./advanced/strings.ts";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) {
  throw new Error("missing #app root element");
}

// Static skeleton only: structure and ids, no baked-in copy. Every visible
// string is assigned below from ui/src/strings (default) or
// ui/src/advanced (advanced), so this file has nothing for the microcopy
// lint to check and nowhere for jargon to hide.
root.innerHTML = `
  <div class="op-app">
    <header class="op-header">
      <h1 class="op-header__title" id="op-app-title"></h1>
      <button type="button" class="op-mode-toggle" id="op-mode-toggle" aria-pressed="false">
        <span id="op-mode-toggle-label"></span>
      </button>
    </header>
    <main class="op-main">
      <section class="op-panel" aria-labelledby="op-palette-heading">
        <h2 class="op-panel__title" id="op-palette-heading"></h2>
        <form class="op-palette" id="op-palette-form">
          <label class="op-visually-hidden" id="op-palette-label" for="op-palette-input"></label>
          <input class="op-palette__input" id="op-palette-input" type="text" autocomplete="off" />
          <button type="submit" class="op-button op-button--primary" id="op-palette-submit"></button>
        </form>
        <p class="op-status">
          <span class="op-status__dot" id="op-run-status-dot" data-state="idle"></span>
          <span id="op-run-status-label"></span>
        </p>
      </section>
      <section class="op-panel" aria-labelledby="op-run-viewer-heading">
        <h2 class="op-panel__title" id="op-run-viewer-heading"></h2>
        <p><span id="op-model-indicator"></span></p>
        <ol class="op-step-list" id="op-step-list"></ol>
        <div>
          <button type="button" class="op-button" id="op-stop-button"></button>
          <button type="button" class="op-button" id="op-pause-button"></button>
        </div>
        <form class="op-palette op-intervene" id="op-intervene-form" hidden>
          <label class="op-visually-hidden" id="op-intervene-label" for="op-intervene-input"></label>
          <input class="op-palette__input" id="op-intervene-input" type="text" autocomplete="off" />
          <button type="submit" class="op-button" id="op-intervene-submit"></button>
        </form>
      </section>
    </main>
    <section class="op-advanced-panel" id="op-advanced-panel" hidden aria-labelledby="op-advanced-heading">
      <h2 class="op-panel__title" id="op-advanced-heading"></h2>
      <pre id="op-advanced-log"></pre>
    </section>
  </div>
`;

function byId<T extends HTMLElement>(id: string): T {
  const el = root!.querySelector<T>(`#${id}`);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

const appTitle = byId<HTMLHeadingElement>("op-app-title");
const modeToggleButton = byId<HTMLButtonElement>("op-mode-toggle");
const modeToggleLabel = byId<HTMLSpanElement>("op-mode-toggle-label");
const paletteHeading = byId<HTMLHeadingElement>("op-palette-heading");
const paletteLabel = byId<HTMLLabelElement>("op-palette-label");
const paletteInput = byId<HTMLInputElement>("op-palette-input");
const paletteSubmit = byId<HTMLButtonElement>("op-palette-submit");
const paletteForm = byId<HTMLFormElement>("op-palette-form");
const runStatusDot = byId<HTMLSpanElement>("op-run-status-dot");
const runStatusLabel = byId<HTMLSpanElement>("op-run-status-label");
const runViewerHeading = byId<HTMLHeadingElement>("op-run-viewer-heading");
const modelIndicator = byId<HTMLSpanElement>("op-model-indicator");
const stepList = byId<HTMLOListElement>("op-step-list");
const stopButton = byId<HTMLButtonElement>("op-stop-button");
const pauseButton = byId<HTMLButtonElement>("op-pause-button");
const interveneForm = byId<HTMLFormElement>("op-intervene-form");
const interveneLabel = byId<HTMLLabelElement>("op-intervene-label");
const interveneInput = byId<HTMLInputElement>("op-intervene-input");
const interveneSubmit = byId<HTMLButtonElement>("op-intervene-submit");
const advancedPanel = byId<HTMLElement>("op-advanced-panel");
const advancedHeading = byId<HTMLHeadingElement>("op-advanced-heading");
const advancedLog = byId<HTMLPreElement>("op-advanced-log");

appTitle.textContent = commonStrings.appName;
paletteHeading.textContent = paletteStrings.placeholder;
paletteLabel.textContent = paletteStrings.placeholder;
paletteInput.placeholder = paletteStrings.placeholder;
paletteInput.title = paletteStrings.hint;
paletteSubmit.textContent = paletteStrings.submit;
runViewerHeading.textContent = runViewerStrings.title;
stopButton.textContent = runViewerStrings.stop;
interveneLabel.textContent = runViewerStrings.intervenePlaceholder;
interveneInput.placeholder = runViewerStrings.intervenePlaceholder;
interveneSubmit.textContent = runViewerStrings.interveneSubmit;
advancedHeading.textContent = advancedStrings.navAuditBrowser;
advancedLog.textContent = advancedStrings.auditEmpty;

const bus = createMockBusClient();
const runViewer = createRunViewer(bus);

// The currently streaming canned demo, if any: cancels the timers behind a
// run so Stop (and Pause, which freezes progress until resumed) do not let
// steps that were already scheduled keep arriving after the button is
// pressed. The run's own state (running/paused/halted/done) lives in
// runViewer, not here; this only tracks the demo's own timers.
let stopDemo: (() => void) | null = null;
let lastEvents: BusEvent[] = [];

function renderMode(mode: UiMode): void {
  const isAdvanced = mode === "advanced";
  modeToggleButton.setAttribute("aria-pressed", String(isAdvanced));
  modeToggleLabel.textContent = isAdvanced ? advancedStrings.toggleLabel : advancedStrings.toggleOffLabel;
  advancedPanel.hidden = !isAdvanced;
}

function renderAdvancedLog(): void {
  advancedLog.textContent = lastEvents.length
    ? JSON.stringify(lastEvents.slice(-20), null, 2)
    : advancedStrings.auditEmpty;
}

function renderRunViewer(): void {
  const snapshot = runViewer.getSnapshot();

  runStatusDot.dataset.state = snapshot.runState;
  runStatusLabel.textContent = snapshot.runStateLabel;
  modelIndicator.textContent = snapshot.modelIndicatorLabel;

  stepList.textContent = "";
  for (const step of snapshot.steps) {
    const li = document.createElement("li");
    li.className = "op-step";

    const dot = document.createElement("span");
    dot.className = "op-status__dot";
    dot.dataset.state = step.status;
    dot.setAttribute("aria-hidden", "true");

    const statusText = document.createElement("span");
    statusText.className = "op-visually-hidden";
    statusText.textContent = runViewerStrings.stepStatus[step.status];

    const label = document.createElement("span");
    label.className = "op-step__sentence";
    label.textContent = step.sentence;

    li.append(dot, statusText, label);
    stepList.append(li);
  }

  stopButton.disabled = !snapshot.canStop;
  pauseButton.disabled = !snapshot.canPause;
  pauseButton.textContent = snapshot.pauseButtonLabel;

  interveneForm.hidden = !snapshot.showIntervene;
  if (!snapshot.showIntervene) {
    interveneInput.value = "";
  }
}

bus.subscribe("*", (event) => {
  lastEvents.push(event);
  renderAdvancedLog();
});
runViewer.subscribe(renderRunViewer);

modeToggleButton.addEventListener("click", () => {
  modeStore.toggle();
});
modeStore.subscribe(renderMode);
renderMode(modeStore.get());
renderRunViewer();

paletteForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const stop = submitGoal(bus, paletteInput.value);
  if (stop) {
    stopDemo?.();
    stopDemo = stop;
    paletteInput.value = "";
  }
});

stopButton.addEventListener("click", () => {
  stopDemo?.();
  stopDemo = null;
  runViewer.stop();
});

pauseButton.addEventListener("click", () => {
  if (runViewer.getSnapshot().runState === "running") {
    // A paused run must not keep quietly finishing in the background: freeze
    // the demo's own timers so nothing more streams in until resumed.
    stopDemo?.();
  }
  runViewer.togglePause();
});

interveneForm.addEventListener("submit", (event) => {
  event.preventDefault();
  if (runViewer.intervene(interveneInput.value)) {
    interveneInput.value = "";
  }
});

document.addEventListener("keydown", (event) => {
  if (isGlobalPaletteHotkey(event)) {
    event.preventDefault();
    paletteInput.focus();
    paletteInput.select();
  }
});
