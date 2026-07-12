import "./styles/base.css";
import { modeStore, type UiMode } from "./state/mode.ts";
import { themeStore, type ThemeMode } from "./theme/store.ts";
import { createMockBusClient } from "./bus/mockClient.ts";
import type { BusEvent } from "./bus/types.ts";
import { isGlobalPaletteHotkey, submitGoal } from "./palette/palette.ts";
import { createRunViewer } from "./runViewer/state.ts";
import { mountRunViewer } from "./runViewer/view.ts";
import { createUndoScreen } from "./undo/state.ts";
import { mountUndoScreen } from "./undo/view.ts";
import {
  paletteStrings,
  commonStrings,
  navStrings,
  themeToggleStrings,
  dashboardStrings,
  undoEntryStrings,
} from "./strings/default.ts";
import { advancedStrings } from "./advanced/strings.ts";
import { advancedSurfaceVisibility } from "./advanced/state.ts";
import { mountDslEditor, mountRawWorkflowDetails, mountAuditBrowser, mountConnectedTools } from "./advanced/view.ts";
import { createConnectedToolsStore } from "./advanced/connectedTools.ts";
import { createMockRegistry, type MockWorkflowRecord } from "./library/mockRegistry.ts";
import { createLibrary } from "./library/state.ts";
import { mountLibrary } from "./library/view.ts";
import { libraryStrings } from "./library/strings.ts";
import { createDashboard } from "./dashboard/state.ts";
import { mountDashboard } from "./dashboard/view.ts";
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
    <section class="op-panel op-screen" id="op-screen-dashboard" hidden aria-label="Dashboard">
      <div id="op-dashboard-mount"></div>
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
      <div id="op-run-viewer-mount"></div>
      <div id="op-undo-entry-mount"></div>
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
    <div class="op-modal-backdrop" id="op-undo-backdrop" hidden>
      <div id="op-undo-mount"></div>
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
    <div id="op-toast-mount"></div>
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
const paletteHeading = byId<HTMLHeadingElement>("op-palette-heading");
const paletteLabel = byId<HTMLLabelElement>("op-palette-label");
const paletteInput = byId<HTMLInputElement>("op-palette-input");
const paletteSubmit = byId<HTMLButtonElement>("op-palette-submit");
const paletteForm = byId<HTMLFormElement>("op-palette-form");
const runStatusDot = byId<HTMLSpanElement>("op-run-status-dot");
const runStatusLabel = byId<HTMLSpanElement>("op-run-status-label");
// The flight recorder (docs/specs/design.md section 3) is built by
// ui/src/runViewer/view.ts's mountRunViewer into this mount point, rather than
// the hand-wired inline markup this screen used before: adopting that view is
// the small main.ts follow-up ui/src/runViewer/view.ts's header comment flagged
// (the filmstrip, mode chips, scrub sync, and inline safety-check card all live
// in that one tested view now instead of being duplicated here).
const runViewerMount = byId<HTMLElement>("op-run-viewer-mount");
const undoEntryMount = byId<HTMLElement>("op-undo-entry-mount");
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
const dashboardMount = byId<HTMLElement>("op-dashboard-mount");
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
const undoBackdrop = byId<HTMLElement>("op-undo-backdrop");
const undoMount = byId<HTMLElement>("op-undo-mount");
const toastMount = byId<HTMLElement>("op-toast-mount");

appTitle.textContent = commonStrings.appName;
paletteHeading.textContent = paletteStrings.placeholder;
paletteLabel.textContent = paletteStrings.placeholder;
paletteInput.placeholder = paletteStrings.placeholder;
paletteInput.title = paletteStrings.hint;
paletteSubmit.textContent = paletteStrings.submit;
advancedHeading.textContent = advancedStrings.toggleLabel;
navDashboard.textContent = navStrings.dashboard;
navLibrary.textContent = navStrings.library;
navRuns.textContent = navStrings.runs;
navSettings.textContent = navStrings.settings;
explainClose.textContent = libraryStrings.closeExplain;

const bus = createMockBusClient();
const runViewer = createRunViewer(bus);
const undoScreen = createUndoScreen(bus);
const registry = createMockRegistry();
const connectedTools = createConnectedToolsStore();

const library = createLibrary(bus, {
  registry,
  onScheduleRequested: (_name, title) => {
    scheduleNotice = libraryStrings.scheduleNotice(title);
    renderLibraryPanel();
  },
});
// Shares Library's own registry instance (not a second createMockRegistry())
// so Up next/Recent runs show the exact same plain-language titles Library
// does for the same workflow name.
const dashboard = createDashboard(bus, { registry });
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
// The one toast this shell shows at a time (design.md section 3's Toasts:
// "Bottom-right, one line"): set from run.completed below, cleared on
// dismiss or once its own Undo action opens ui/src/undo for that run.
let activeToast: { runId: string; message: string } | null = null;

// docs/specs/design.md section 3's nav map. design.md section 3 calls the
// Home dashboard "the new default window view"; now that D4 (ui/src/
// dashboard/) has filled in its real content (hero, sparkline, Up next,
// Recent runs), this is that one-line flip.
type Screen = "dashboard" | "runs" | "library" | "settings";
let activeScreen: Screen = "dashboard";

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

  // The compact run-state indicator that sits next to the palette launcher.
  runStatusDot.dataset.state = snapshot.runState;
  runStatusLabel.textContent = snapshot.runStateLabel;

  // The flight recorder itself (filmstrip, mode chip, streaming step list with
  // inline safety-check cards, Stop/Pause, intervene) is rebuilt from the
  // snapshot by the shared view; scrub selection and Stop/Pause's demo-timer
  // freeze are wired through these callbacks.
  mountRunViewer(runViewerMount, snapshot, {
    onStop: () => {
      stopDemo?.();
      stopDemo = null;
      runViewer.stop();
    },
    onTogglePause: () => {
      if (runViewer.getSnapshot().runState === "running") {
        // A paused run must not keep quietly finishing in the background: freeze
        // the demo's own timers so nothing more streams in until resumed.
        stopDemo?.();
      }
      runViewer.togglePause();
    },
    onIntervene: (text) => {
      runViewer.intervene(text);
    },
    onSelectStep: (stepId) => runViewer.select(stepId),
  });
}

/**
 * The run-detail "Undo this run" entry point (docs/specs/design.md section
 * 3: "From any completed run, 'Undo this run' opens a preview..."), a small
 * button beside the flight recorder shown only once its run reaches "done".
 * ui/src/runViewer/view.ts is not touched for this (that view stays another
 * lane's, per this packet's owned-paths split): this reads the same public
 * snapshot renderRunViewer above already does and renders into its own
 * sibling mount instead.
 */
function renderUndoEntry(): void {
  undoEntryMount.textContent = "";
  const snap = runViewer.getSnapshot();
  if (snap.runState !== "done" || !snap.runId) return;
  const runId = snap.runId;

  const button = document.createElement("button");
  button.type = "button";
  button.className = "op-button op-undo-entry";
  button.textContent = undoEntryStrings.undoThisRun;
  button.addEventListener("click", () => {
    dismissToast();
    undoScreen.open(runId);
  });
  undoEntryMount.append(button);
}

/** The Undo screen itself (ui/src/undo/), reachable from renderUndoEntry above and from the toast's action below. */
function renderUndoScreen(): void {
  const snap = undoScreen.getSnapshot();
  undoBackdrop.hidden = snap.phase === "closed";
  mountUndoScreen(undoMount, snap, {
    onConfirm: () => undoScreen.confirm(),
    onClose: () => undoScreen.close(),
  });
}

function dismissToast(): void {
  if (!activeToast) return;
  activeToast = null;
  renderToast();
}

/**
 * The bottom-right toast (design.md section 3's Toasts: "Bottom-right, one
 * line, verb-first... Amber only when an action is invited"). Its one
 * action opens the same ui/src/undo screen renderUndoEntry above does, for
 * whichever run just raised it; the message text reuses dashboardStrings'
 * existing run-outcome copy (ui/src/dashboard/view.ts's own recent-runs
 * rows say the same thing) rather than inventing a second wording for it.
 */
function renderToast(): void {
  toastMount.textContent = "";
  if (!activeToast) return;
  const { runId, message } = activeToast;

  const toast = document.createElement("div");
  toast.className = "op-toast";
  toast.setAttribute("role", "status");

  const text = document.createElement("span");
  text.className = "op-toast__message";
  text.textContent = message;

  const action = document.createElement("button");
  action.type = "button";
  action.className = "op-toast__action";
  action.textContent = undoEntryStrings.undoThisRun;
  action.addEventListener("click", () => {
    dismissToast();
    undoScreen.open(runId);
  });

  toast.append(text, action);
  toastMount.append(toast);
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
    onReorder: (name, beforeName) => library.reorder(name, beforeName),
    onSearchChange: (query) => library.setSearchQuery(query),
  });
  if (scheduleNotice) {
    const notice = document.createElement("p");
    notice.className = "op-settings__hint";
    notice.textContent = scheduleNotice;
    libraryMount.append(notice);
  }
}

function renderDashboardPanel(): void {
  mountDashboard(dashboardMount, dashboard.getSnapshot());
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
// The toast's own trigger (design.md section 3's Toasts, "Run complete, 14
// steps" is that section's own example): a completed run raises it exactly
// once, independent of however many times runViewer's snapshot re-renders
// afterward (a scrub selection after done, for instance, must not re-show
// it).
bus.subscribe("run.completed", (event) => {
  if (event.topic !== "run.completed") return;
  const { run_id, outcome, steps } = event.payload;
  activeToast = {
    runId: run_id,
    message: outcome === "ok" ? dashboardStrings.outcomeOk(steps) : dashboardStrings.outcomeFailed,
  };
  renderToast();
});
connectedTools.subscribe(() => {
  if (modeStore.get() === "advanced") renderAdvancedTools();
});
runViewer.subscribe(renderRunViewer);
runViewer.subscribe(renderUndoEntry);
library.subscribe(renderLibraryPanel);
dashboard.subscribe(renderDashboardPanel);
settings.subscribe(renderSettingsPanel);
tray.subscribe(renderTrayPanel);
wizard.subscribe(renderWizardPanel);
undoScreen.subscribe(renderUndoScreen);

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
renderUndoEntry();
renderLibraryPanel();
renderDashboardPanel();
renderSettingsPanel();
renderTrayPanel();
renderWizardPanel();
renderUndoScreen();
renderToast();

paletteForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const stop = submitGoal(bus, paletteInput.value);
  if (stop) {
    stopDemo?.();
    stopDemo = stop;
    paletteInput.value = "";
  }
});

// Stop, Pause, intervene, and filmstrip scrubbing are wired through
// mountRunViewer's callbacks in renderRunViewer() above, not to static ids.

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
