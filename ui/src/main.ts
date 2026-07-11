import "./styles/base.css";
import { modeStore, type UiMode } from "./state/mode.ts";
import { createMockBusClient, simulateDemoRun, DEMO_STEP_SENTENCES } from "./bus/mockClient.ts";
import { RUN_MODE_EXPLORE, type BusEvent } from "./bus/types.ts";
import { trayStrings, paletteStrings, runViewerStrings, commonStrings } from "./strings/default.ts";
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
const advancedPanel = byId<HTMLElement>("op-advanced-panel");
const advancedHeading = byId<HTMLHeadingElement>("op-advanced-heading");
const advancedLog = byId<HTMLPreElement>("op-advanced-log");

appTitle.textContent = commonStrings.appName;
paletteHeading.textContent = paletteStrings.placeholder;
paletteLabel.textContent = paletteStrings.placeholder;
paletteInput.placeholder = paletteStrings.placeholder;
paletteSubmit.textContent = paletteStrings.submit;
runViewerHeading.textContent = runViewerStrings.title;
stopButton.textContent = runViewerStrings.stop;
pauseButton.textContent = runViewerStrings.pause;
advancedHeading.textContent = advancedStrings.navAuditBrowser;
advancedLog.textContent = advancedStrings.auditEmpty;

const bus = createMockBusClient();

interface StepRow {
  id: string;
  sentence: string;
  status: "pending" | "ok" | "failed" | "retried";
}

let steps: StepRow[] = [];
let runState: "idle" | "running" | "halted" = "idle";
let lastEvents: BusEvent[] = [];
let stopDemo: (() => void) | null = null;

function renderMode(mode: UiMode): void {
  const isAdvanced = mode === "advanced";
  modeToggleButton.setAttribute("aria-pressed", String(isAdvanced));
  modeToggleLabel.textContent = isAdvanced ? advancedStrings.toggleLabel : advancedStrings.toggleOffLabel;
  advancedPanel.hidden = !isAdvanced;
}

function renderSteps(): void {
  stepList.textContent = "";
  for (const step of steps) {
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
}

function renderRunStatus(): void {
  runStatusDot.dataset.state = runState;
  runStatusLabel.textContent =
    runState === "running" ? trayStrings.running : runState === "halted" ? trayStrings.halted : trayStrings.idle;
}

function renderAdvancedLog(): void {
  advancedLog.textContent = lastEvents.length
    ? JSON.stringify(lastEvents.slice(-20), null, 2)
    : advancedStrings.auditEmpty;
}

function upsertStep(id: string, status: StepRow["status"]): void {
  const sentence = DEMO_STEP_SENTENCES[id] ?? id;
  const existing = steps.find((s) => s.id === id);
  if (existing) {
    existing.status = status;
  } else {
    steps.push({ id, sentence, status });
  }
  renderSteps();
}

bus.subscribe("*", (event) => {
  lastEvents.push(event);
  renderAdvancedLog();

  switch (event.topic) {
    case "run.started":
      steps = [];
      runState = "running";
      modelIndicator.textContent =
        event.payload.mode === RUN_MODE_EXPLORE ? runViewerStrings.modelOn : runViewerStrings.modelOff;
      renderRunStatus();
      renderSteps();
      break;
    case "run.step.proposed":
      upsertStep(event.payload.step.id, "pending");
      break;
    case "run.step.executed":
      upsertStep(event.payload.step_id, event.payload.outcome);
      break;
    case "run.step.failed":
      upsertStep(event.payload.step_id, "failed");
      break;
    case "run.completed":
      runState = "idle";
      renderRunStatus();
      break;
    case "run.halted":
      runState = "halted";
      renderRunStatus();
      break;
    default:
      break;
  }
});

modeToggleButton.addEventListener("click", () => {
  modeStore.toggle();
});
modeStore.subscribe(renderMode);
renderMode(modeStore.get());
renderRunStatus();

paletteForm.addEventListener("submit", (event) => {
  event.preventDefault();
  stopDemo?.();
  stopDemo = simulateDemoRun(bus, { stepDelayMs: 450 });
  paletteInput.value = "";
});

stopButton.addEventListener("click", () => {
  stopDemo?.();
  stopDemo = null;
  runState = "idle";
  renderRunStatus();
});
