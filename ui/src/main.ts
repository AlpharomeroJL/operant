import "./styles/base.css";
import { modeStore, type UiMode } from "./state/mode.ts";
import { themeStore, type ThemeMode } from "./theme/store.ts";
import { createMockBusClient } from "./bus/mockClient.ts";
import type { BusEvent } from "./bus/types.ts";
import { isGlobalPaletteHotkey, submitGoal } from "./palette/palette.ts";
import { createRunViewer } from "./runViewer/state.ts";
import {
  paletteStrings,
  runViewerStrings,
  commonStrings,
  navStrings,
  dashboardStrings,
  themeToggleStrings,
} from "./strings/default.ts";
import { advancedStrings } from "./advanced/strings.ts";
import { advancedSurfaceVisibility } from "./advanced/state.ts";
import { mountDslEditor, mountRawWorkflowDetails, mountAuditBrowser, mountConnectedTools } from "./advanced/view.ts";
import { createConnectedToolsStore } from "./advanced/connectedTools.ts";
import { createMockRegistry, type MockWorkflowRecord } from "./library/mockRegistry.ts";
import { createLibrary } from "./library/state.ts";
import { mountLibrary } from "./library/view.ts";
import { libraryStrings } from "./library/strings.ts";
import { createGrantPrompt } from "./grants/state.ts";
import { mountGrantPrompt } from "./grants/view.ts";
import { createSettings } from "./settings/state.ts";
import { mountSettings } from "./settings/view.ts";
import { settingsDetailStrings } from "./settings/strings.ts";
import type { BackupPayload } from "./settings/mockStore.ts";
import { createTray } from "./tray/state.ts";
import { mountTray } from "./tray/view.ts";
import { mountWorkflowView } from "./render/workflowView.ts";
import { createWizard } from "./wizard/state.ts";
import { mountWizard } from "./wizard/view.ts";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) {
  throw new Error("missing #app root element");
}

// Static skeleton only: structure and ids, no baked-in copy. Every visible
// string is assigned below from ui/src/strings (default) or ui/src/advanced
// (advanced), or from a module's own default-mode strings.ts (library,
// grants, settings, tray), so this file has nothing for the microcopy lint
// to check and nowhere for jargon to hide.
root.innerHTML = `
  <div class="op-app">
    <header class="op-header">
      <h1 class="op-header__title" id="op-app-title"></h1>
      <nav class="op-nav" id="op-nav" aria-label="Screens">
        <button type="button" class="op-nav__button" id="op-nav-dashboard" aria-pressed="false"></button>
        <button type="button" class="op-nav__button" id="op-nav-library" aria-pressed="false"></button>
        <button type="button" class="op-nav__button" id="op-nav-runs" aria-pressed="true"></button>
        <button type="button" class="op-nav__button" id="op-nav-settings" aria-pressed="false"></button>
      </nav>
      <div id="op-tray-mount"></div>
      <button type="button" class="op-theme-toggle" id="op-theme-toggle"></button>
      <button type="button" class="op-mode-toggle" id="op-mode-toggle" aria-pressed="false">
        <span id="op-mode-toggle-label"></span>
      </button>
    </header>
    <section class="op-panel op-screen op-dashboard-placeholder" id="op-screen-dashboard" hidden aria-labelledby="op-dashboard-heading">
      <h2 class="op-panel__title" id="op-dashboard-heading"></h2>
      <p class="op-empty" id="op-dashboard-body"></p>
    </section>
    <main class="op-main" id="op-screen-runs">
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
    <section class="op-panel op-screen" id="op-screen-library" hidden aria-label="Library">
      <div id="op-library-mount"></div>
    </section>
    <section class="op-panel op-screen" id="op-screen-settings" hidden aria-label="Settings">
      <div id="op-settings-mount"></div>
    </section>
    <section class="op-panel op-explain-panel" id="op-explain-panel" hidden aria-labelledby="op-explain-heading">
      <div class="op-explain-panel__header">
        <h2 class="op-panel__title" id="op-explain-heading"></h2>
        <button type="button" class="op-button" id="op-explain-close"></button>
      </div>
      <div id="op-explain-mount"></div>
    </section>
    <div class="op-modal-backdrop" id="op-grant-backdrop" hidden>
      <div id="op-grant-mount"></div>
    </div>
    <div class="op-modal-backdrop" id="op-wizard-backdrop" hidden>
      <div id="op-wizard-mount"></div>
    </div>
    <section class="op-advanced-panel" id="op-advanced-panel" hidden aria-labelledby="op-advanced-heading">
      <h2 class="op-panel__title" id="op-advanced-heading"></h2>
      <div class="op-advanced-panel__grid">
        <div id="op-advanced-editor"></div>
        <div id="op-advanced-raw"></div>
        <div id="op-advanced-audit"></div>
        <div id="op-advanced-tools"></div>
      </div>
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
const themeToggleButton = byId<HTMLButtonElement>("op-theme-toggle");
const dashboardHeading = byId<HTMLHeadingElement>("op-dashboard-heading");
const dashboardBody = byId<HTMLParagraphElement>("op-dashboard-body");
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
const advancedDsl = byId<HTMLElement>("op-advanced-editor");
const advancedRaw = byId<HTMLElement>("op-advanced-raw");
const advancedAudit = byId<HTMLElement>("op-advanced-audit");
const advancedTools = byId<HTMLElement>("op-advanced-tools");

const navDashboard = byId<HTMLButtonElement>("op-nav-dashboard");
const navLibrary = byId<HTMLButtonElement>("op-nav-library");
const navRuns = byId<HTMLButtonElement>("op-nav-runs");
const navSettings = byId<HTMLButtonElement>("op-nav-settings");
const screenDashboard = byId<HTMLElement>("op-screen-dashboard");
const screenLibrary = byId<HTMLElement>("op-screen-library");
const screenRuns = byId<HTMLElement>("op-screen-runs");
const screenSettings = byId<HTMLElement>("op-screen-settings");
const trayMount = byId<HTMLElement>("op-tray-mount");
const libraryMount = byId<HTMLElement>("op-library-mount");
const settingsMount = byId<HTMLElement>("op-settings-mount");
const explainPanel = byId<HTMLElement>("op-explain-panel");
const explainHeading = byId<HTMLHeadingElement>("op-explain-heading");
const explainClose = byId<HTMLButtonElement>("op-explain-close");
const explainMount = byId<HTMLElement>("op-explain-mount");
const grantBackdrop = byId<HTMLElement>("op-grant-backdrop");
const grantMount = byId<HTMLElement>("op-grant-mount");
const wizardBackdrop = byId<HTMLElement>("op-wizard-backdrop");
const wizardMount = byId<HTMLElement>("op-wizard-mount");

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
advancedHeading.textContent = advancedStrings.toggleLabel;
navDashboard.textContent = navStrings.dashboard;
navLibrary.textContent = navStrings.library;
navRuns.textContent = navStrings.runs;
navSettings.textContent = navStrings.settings;
dashboardHeading.textContent = dashboardStrings.title;
dashboardBody.textContent = dashboardStrings.placeholderBody;
explainClose.textContent = libraryStrings.closeExplain;

const bus = createMockBusClient();
const runViewer = createRunViewer(bus);
const registry = createMockRegistry();
const connectedTools = createConnectedToolsStore();

const library = createLibrary(bus, {
  registry,
  onScheduleRequested: (_name, title) => {
    scheduleNotice = libraryStrings.scheduleNotice(title);
    renderLibraryPanel();
  },
});
const settings = createSettings(bus);
const tray = createTray(bus);

// First-run onboarding (C19, FR-U1/FR-U4). Shown until the wizard reports
// complete, then never again on this device. Same localStorage-with-
// in-memory-fallback pattern as ui/src/state/mode.ts and
// ui/src/settings/mockStore.ts, so a sandboxed webview with no storage just
// falls back to "show it again next launch" instead of throwing.
const WIZARD_DONE_KEY = "operant.wizard.completed";
function wizardAlreadyDone(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(WIZARD_DONE_KEY) === "1";
  } catch {
    return false;
  }
}
function markWizardDone(): void {
  try {
    if (typeof localStorage !== "undefined") localStorage.setItem(WIZARD_DONE_KEY, "1");
  } catch {
    // Storage unavailable: the wizard just shows again next launch.
  }
}
const wizard = createWizard(bus);
let wizardDismissed = wizardAlreadyDone();

// The currently streaming canned demo, if any: cancels the timers behind a
// run so Stop (and Pause, which freezes progress until resumed) do not let
// steps that were already scheduled keep arriving after the button is
// pressed. The run's own state (running/paused/halted/done) lives in
// runViewer, not here; this only tracks the demo's own timers.
let stopDemo: (() => void) | null = null;
let lastEvents: BusEvent[] = [];
let scheduleNotice: string | null = null;
// The workflow last opened via Explain: also what the Advanced DSL editor
// and raw-details panes show, so a developer looking at one is looking at
// the other, the same workflow, in plain English and in raw form.
let selectedWorkflowName: string | null = null;

// docs/specs/design.md section 3's nav map. "runs" stays the initial screen
// for now, even though design.md section 3 calls the Home dashboard "the new
// default window view": its real content is a later packet's job (this
// packet's op-screen-dashboard is a themed placeholder, see the shell markup
// above), so the working default stays the screen with actual functionality
// behind it. Flipping the default to "dashboard" once that packet lands is a
// one-line change here.
type Screen = "dashboard" | "runs" | "library" | "settings";
let activeScreen: Screen = "runs";

function selectedRecord(): MockWorkflowRecord | undefined {
  return selectedWorkflowName ? registry.get(selectedWorkflowName) : undefined;
}

function renderScreen(): void {
  screenDashboard.hidden = activeScreen !== "dashboard";
  screenRuns.hidden = activeScreen !== "runs";
  screenLibrary.hidden = activeScreen !== "library";
  screenSettings.hidden = activeScreen !== "settings";
  navDashboard.setAttribute("aria-pressed", String(activeScreen === "dashboard"));
  navRuns.setAttribute("aria-pressed", String(activeScreen === "runs"));
  navLibrary.setAttribute("aria-pressed", String(activeScreen === "library"));
  navSettings.setAttribute("aria-pressed", String(activeScreen === "settings"));
}

function showScreen(screen: Screen): void {
  activeScreen = screen;
  renderScreen();
}

function renderMode(mode: UiMode): void {
  const isAdvanced = mode === "advanced";
  modeToggleButton.setAttribute("aria-pressed", String(isAdvanced));
  modeToggleLabel.textContent = isAdvanced ? advancedStrings.toggleLabel : advancedStrings.toggleOffLabel;
  advancedPanel.hidden = !isAdvanced;
  renderAdvancedSurfaces(mode);
}

function renderAdvancedSurfaces(mode: UiMode): void {
  const visibility = advancedSurfaceVisibility(mode);
  advancedDsl.hidden = !visibility.dslEditor;
  advancedRaw.hidden = !visibility.rawWorkflowDetails;
  advancedAudit.hidden = !visibility.auditBrowser;
  advancedTools.hidden = !visibility.connectedTools;
  if (visibility.dslEditor) renderAdvancedDsl();
  if (visibility.rawWorkflowDetails) renderAdvancedRaw();
  if (visibility.auditBrowser) renderAdvancedAudit();
  if (visibility.connectedTools) renderAdvancedTools();
}

function renderAdvancedDsl(): void {
  mountDslEditor(advancedDsl, selectedRecord());
}

function renderAdvancedRaw(): void {
  mountRawWorkflowDetails(advancedRaw, selectedRecord());
}

function renderAdvancedAudit(): void {
  mountAuditBrowser(advancedAudit, lastEvents);
}

function renderAdvancedTools(): void {
  mountConnectedTools(advancedTools, connectedTools.list(), {
    onToggle: (name, enabled) => connectedTools.setEnabled(name, enabled),
  });
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

function closeExplain(): void {
  explainPanel.hidden = true;
  explainMount.textContent = "";
}

function openExplain(name: string): void {
  const view = library.explain(name);
  if (!view) return;
  selectedWorkflowName = name;
  explainHeading.textContent = view.title;
  mountWorkflowView(explainMount, view);
  explainPanel.hidden = false;
  renderAdvancedDsl();
  renderAdvancedRaw();
}

function closeGrantPrompt(): void {
  grantBackdrop.hidden = true;
  grantMount.textContent = "";
}

/** Run a saved workflow from the library. A workflow with no capabilities skips the grant prompt entirely, same as docs/specs/registry.md's install flow only requires approval when there is something to approve. */
function requestRun(name: string): void {
  const record = registry.get(name);
  if (!record) return;
  const caps = record.manifest.capabilities;
  const needsGrant = Boolean((caps.paths && caps.paths.length) || (caps.apps && caps.apps.length) || caps.network);
  if (!needsGrant) {
    library.run(name);
    return;
  }

  const prompt = createGrantPrompt(caps, {
    onAllow: () => {
      library.run(name);
      closeGrantPrompt();
    },
    onDeny: () => closeGrantPrompt(),
  });
  mountGrantPrompt(grantMount, prompt.getSnapshot(), {
    onAllow: () => prompt.allow(),
    onDeny: () => prompt.deny(),
  });
  grantBackdrop.hidden = false;
}

function renderLibraryPanel(): void {
  const snapshot = library.getSnapshot();
  mountLibrary(libraryMount, snapshot, {
    onRun: requestRun,
    onSchedule: (name) => library.schedule(name),
    onExplain: openExplain,
  });
  if (scheduleNotice) {
    const notice = document.createElement("p");
    notice.className = "op-settings__hint";
    notice.textContent = scheduleNotice;
    libraryMount.append(notice);
  }
}

function downloadBackup(payload: BackupPayload): void {
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `operant-backup-${payload.exportedAt.slice(0, 10)}.json`;
  a.click();
  URL.revokeObjectURL(url);
}

function importBackupFile(file: File): void {
  file
    .text()
    .then((text) => settings.importBackup(JSON.parse(text) as BackupPayload))
    .catch(() => {
      const notice = document.createElement("p");
      notice.className = "op-settings__hint";
      notice.textContent = settingsDetailStrings.backupInvalid;
      settingsMount.append(notice);
    });
}

function renderSettingsPanel(): void {
  mountSettings(settingsMount, settings.getSnapshot(), {
    onVoiceToggle: (on) => settings.setVoiceEnabled(on),
    onSpeakingRateChange: (rate) => settings.setSpeakingRate(rate),
    onWatchAndSuggestToggle: (on) => settings.setWatchAndSuggest(on),
    onPurge: () => settings.purgeWatchedData(),
    onStartChordRecording: () => settings.startChordRecording(),
    onCancelChordRecording: () => settings.cancelChordRecording(),
    onExportBackup: () => downloadBackup(settings.exportBackup()),
    onImportBackupFile: importBackupFile,
    onAutoUpdateToggle: (on) => settings.setAutoUpdateEnabled(on),
  });
}

function renderTrayPanel(): void {
  mountTray(trayMount, tray.getSnapshot(), {
    onDismissNotification: (id) => tray.dismissNotification(id),
  });
}

/**
 * The onboarding wizard renders as a modal overlay in front of everything
 * else until it reports complete, then hides for good on this device
 * (WIZARD_DONE_KEY above). Every screen it shows comes straight from
 * wizard.getSnapshot(); this function owns none of that copy, same split as
 * every other render* function here.
 */
function renderWizardPanel(): void {
  const snap = wizard.getSnapshot();
  if (snap.complete && !wizardDismissed) {
    wizardDismissed = true;
    markWizardDone();
  }
  wizardBackdrop.hidden = wizardDismissed;
  if (wizardDismissed) return;

  mountWizard(wizardMount, snap, {
    onContinueWelcome: () => wizard.continueWelcome(),
    onChooseChatGPT: () => wizard.chooseChatGPT(),
    onChooseClaude: () => wizard.chooseClaude(),
    onStartLocalDownload: () => wizard.startLocalDownload(),
    onPauseLocalDownload: () => wizard.pauseLocalDownload(),
    onResumeLocalDownload: () => wizard.resumeLocalDownload(),
    onCancelLocalDownload: () => wizard.cancelLocalDownload(),
    onContinueAfterLocalDownload: () => wizard.continueAfterLocalDownload(),
    onAccessKeyTextChange: (text) => wizard.setAccessKeyText(text),
    onChooseProviderManually: (provider) => wizard.chooseProviderManually(provider),
    onContinueWithAccessKey: () => wizard.continueWithAccessKey(),
    onStartDemo: () => wizard.startDemo(),
    onPlayMicSample: () => wizard.playMicSample(),
    onSkipMicCheck: () => wizard.skipMicCheck(),
    onContinueMicCheck: () => wizard.continueMicCheck(),
    onSaveAsWorkflow: () => wizard.saveAsWorkflow(),
    onContinueAfterDemo: () => wizard.continueAfterDemo(),
    onChooseSchedule: (id) => wizard.chooseSchedule(id),
    onFinishSchedule: () => wizard.finishSchedule(),
  });
}

bus.subscribe("*", (event) => {
  lastEvents.push(event);
  if (modeStore.get() === "advanced") renderAdvancedAudit();
});
connectedTools.subscribe(() => {
  if (modeStore.get() === "advanced") renderAdvancedTools();
});
runViewer.subscribe(renderRunViewer);
library.subscribe(renderLibraryPanel);
settings.subscribe(renderSettingsPanel);
tray.subscribe(renderTrayPanel);
wizard.subscribe(renderWizardPanel);

navDashboard.addEventListener("click", () => showScreen("dashboard"));
navLibrary.addEventListener("click", () => showScreen("library"));
navRuns.addEventListener("click", () => showScreen("runs"));
navSettings.addEventListener("click", () => showScreen("settings"));
explainClose.addEventListener("click", closeExplain);

modeToggleButton.addEventListener("click", () => {
  modeStore.toggle();
});
modeStore.subscribe(renderMode);

/**
 * Dark/light/system (docs/specs/design.md section 3's Settings > Appearance
 * choice, wired here as one compact header control, ui/src/theme/store.ts).
 * themeStore.init() applies the resolved theme to <html data-theme="..."> so
 * ui/src/styles/tokens.css's [data-theme] overrides take effect immediately
 * on load, before anything else renders: every screen mounted after this
 * point (including the very first renderScreen()/renderMode() below) reads
 * whichever theme's custom properties are already in force, so nothing ever
 * paints with a stale or unthemed color.
 */
function renderThemeToggle(mode: ThemeMode): void {
  themeToggleButton.textContent = themeToggleStrings[mode];
  themeToggleButton.title = themeToggleStrings.hint;
}
themeToggleButton.addEventListener("click", () => themeStore.cycle());
themeStore.subscribe((mode) => renderThemeToggle(mode));
themeStore.init();
renderThemeToggle(themeStore.get());

renderScreen();
renderMode(modeStore.get());
renderRunViewer();
renderLibraryPanel();
renderSettingsPanel();
renderTrayPanel();
renderWizardPanel();

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
  if (settings.getSnapshot().recordingChord) {
    event.preventDefault();
    if (event.key === "Escape") {
      settings.cancelChordRecording();
      return;
    }
    settings.recordChordKey({
      key: event.key,
      ctrlKey: event.ctrlKey,
      altKey: event.altKey,
      shiftKey: event.shiftKey,
      metaKey: event.metaKey,
    });
    return;
  }
  if (isGlobalPaletteHotkey(event)) {
    event.preventDefault();
    paletteInput.focus();
    paletteInput.select();
  }
});
