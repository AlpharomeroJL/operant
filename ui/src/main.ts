import "./styles/base.css";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { modeStore, type UiMode } from "./state/mode.ts";
import { themeStore, type ThemeMode } from "./theme/store.ts";
import { createMockBusClient, type BusClient } from "./bus/mockClient.ts";
import { createRealClient } from "./bus/realClient.ts";
import { handshakeCore, type CoreCapabilities } from "./boot/coreGate.ts";
import { mountDemoBanner, renderBlockingScreen, renderErrorScreen } from "./boot/coreGateView.ts";
import type { BusEvent } from "./bus/types.ts";
import { isGlobalPaletteHotkey } from "./palette/palette.ts";
import { createMockCoreCommands, readForegroundWindowProcess } from "./bus/commands.ts";
import { createPaletteController, type PaletteCommit } from "./palette/state.ts";
import { mountPalette } from "./palette/view.ts";
import { buildQuickActionEntries, buildSettingsEntries, PALETTE_ACTION_ID } from "./palette/quickActions.ts";
import type { PaletteEntry } from "./palette/catalog.ts";
import { createRunViewer } from "./runViewer/state.ts";
import { mountRunViewer } from "./runViewer/view.ts";
import { createUndoScreen } from "./undo/state.ts";
import { mountUndoScreen } from "./undo/view.ts";
import { createMockUndoCommands } from "./undo/realJournal.ts";
import { journalForRun as fixtureJournalForRun } from "./undo/mockJournal.ts";
import {
  commonStrings,
  doctorStrings,
  navStrings,
  themeToggleStrings,
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
import { createTauriDashboardSource } from "./dashboard/source.ts";
import { mountDashboard } from "./dashboard/view.ts";
import { createGrantPrompt } from "./grants/state.ts";
import { mountGrantPrompt } from "./grants/view.ts";
import { createSettings } from "./settings/state.ts";
import { mountSettings, type SettingsSection } from "./settings/view.ts";
import { settingsDetailStrings } from "./settings/strings.ts";
import { chordPartsFromEvent, formatChord } from "./settings/chord.ts";
import type { BackupPayload } from "./settings/mockStore.ts";
import { createLiveSettingsStore, getTauriInvoke, base64ToBytes, bytesToBase64 } from "./settings/liveStore.ts";
import { createTray } from "./tray/state.ts";
import { mountTray } from "./tray/view.ts";
import { createToasts } from "./toasts/state.ts";
import { mountToast } from "./toasts/view.ts";
import { mountWorkflowView } from "./render/workflowView.ts";
import { createWizard } from "./wizard/state.ts";
import { mountWizard } from "./wizard/view.ts";
import { createMockBackendConfigurator } from "./wizard/engine.ts";
import { tourStore } from "./tour/state.ts";
import { mountTourCallout } from "./tour/view.ts";
import { createDoctor } from "./doctor/state.ts";
import { mountDoctor } from "./doctor/view.ts";
import { DEMO_DOCTOR_FINDINGS, demoHealthyFinding } from "./doctor/demoFindings.ts";

const root = document.querySelector<HTMLDivElement>("#app");
if (!root) {
  throw new Error("missing #app root element");
}

// Boot decision (contracts/ipc.md section 3). Inside the Tauri shell, gate
// every real-work surface on the capability handshake: a core that cannot
// automate gets a blocking screen, and a failed handshake gets an error
// screen, never a silent swap to canned data. Outside Tauri (a plain browser)
// or with the explicit Demo flag, run the canned mock, clearly labeled. The
// mock bus and simulateDemoRun survive only on this Demo path.
if (shouldBootRealCore()) {
  void bootAgainstCore(root);
} else {
  mountApp(root, createMockBusClient(), { demo: true });
}

/** True only inside the Tauri shell and without the explicit Demo override. */
function shouldBootRealCore(): boolean {
  return isTauriRuntime() && !demoFlagSet();
}

function isTauriRuntime(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as { __TAURI__?: unknown; __TAURI_INTERNALS__?: unknown };
  return Boolean(w.__TAURI__) || Boolean(w.__TAURI_INTERNALS__) || isTauri();
}

/** Explicit opt-in to canned Demo mode: ?demo in the URL or window.__OPERANT_DEMO__. */
function demoFlagSet(): boolean {
  try {
    if (typeof window === "undefined") return false;
    const w = window as { __OPERANT_DEMO__?: unknown };
    if (w.__OPERANT_DEMO__ === true) return true;
    return new URLSearchParams(window.location.search).has("demo");
  } catch {
    return false;
  }
}

/**
 * The Tauri boot path (contracts/ipc.md section 3): wait for core_ready, run
 * the get_capabilities handshake, then either build the real-work UI over the
 * real bus client, show the blocking screen naming each missing capability, or
 * show an error state. A failed connection NEVER falls back to the mock.
 */
async function bootAgainstCore(target: HTMLDivElement): Promise<void> {
  const connection = await handshakeCore({
    ready: () => invoke("core_ready"),
    capabilities: () => invoke<CoreCapabilities>("core_capabilities"),
  });
  if (connection.kind === "real") {
    mountApp(target, createRealClient(), { demo: false });
  } else if (connection.kind === "blocked") {
    renderBlockingScreen(target, connection.missing);
  } else {
    renderErrorScreen(target, connection.message);
  }
}

/**
 * Builds the full shell UI against a given bus client. In Demo mode a
 * persistent banner makes clear the run data is canned, never the user's real
 * computer; the real-core path always calls this with demo:false.
 */
function mountApp(root: HTMLDivElement, bus: BusClient, opts: { demo: boolean }): void {
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
      <button type="button" class="op-button op-header__doctor" id="op-doctor-open"></button>
    </header>
    <section class="op-panel op-screen" id="op-screen-dashboard" hidden aria-label="Dashboard">
      <div id="op-dashboard-mount"></div>
    </section>
    <main class="op-main" id="op-screen-runs">
      <section class="op-panel">
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
    <div class="op-wizard-shell" id="op-wizard-backdrop" hidden>
      <div id="op-wizard-mount"></div>
    </div>
    <div class="op-modal-backdrop" id="op-undo-backdrop" hidden>
      <div id="op-undo-mount"></div>
    </div>
    <div class="op-modal-backdrop" id="op-doctor-backdrop" hidden>
      <div id="op-doctor-mount"></div>
    </div>
    <div class="op-modal-backdrop op-palette-backdrop" id="op-palette-backdrop" hidden>
      <div id="op-palette-mount"></div>
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
    <div id="op-tour-mount"></div>
  </div>
`;

// Demo mode is honest about itself: a persistent banner saying the run data is
// canned, not the user's real computer. Only reached on the Demo path.
if (opts.demo) mountDemoBanner(root);

function byId<T extends HTMLElement>(id: string): T {
  const el = root!.querySelector<T>(`#${id}`);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

const appTitle = byId<HTMLHeadingElement>("op-app-title");
const modeToggleButton = byId<HTMLButtonElement>("op-mode-toggle");
const modeToggleLabel = byId<HTMLSpanElement>("op-mode-toggle-label");
const themeToggleButton = byId<HTMLButtonElement>("op-theme-toggle");
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
const paletteBackdrop = byId<HTMLElement>("op-palette-backdrop");
const paletteMount = byId<HTMLElement>("op-palette-mount");
const tourMount = byId<HTMLElement>("op-tour-mount");

appTitle.textContent = commonStrings.appName;
advancedHeading.textContent = advancedStrings.toggleLabel;
navDashboard.textContent = navStrings.dashboard;
navLibrary.textContent = navStrings.library;
navRuns.textContent = navStrings.runs;
navSettings.textContent = navStrings.settings;
explainClose.textContent = libraryStrings.closeExplain;

const runViewer = createRunViewer(bus);
// B10: the undo screen sends the preview_undo / undo_run commands and renders
// the core's echoed undo.previewed / undo.applied (contracts/ipc.md 5c). With
// no Tauri bridge yet (the whole shell still runs on the mock bus above), the
// dev/Demo core stand-in previews from ./undo/mockJournal.ts's fixture; this is
// the one line that swaps for a real invoke-backed UndoCommands once the bridge
// lands, with no change to ui/src/undo/state.ts.
const undoScreen = createUndoScreen(bus, {
  commands: createMockUndoCommands(bus, fixtureJournalForRun),
});
const registry = createMockRegistry();
const connectedTools = createConnectedToolsStore();

// The shell-to-core command seam (contracts/ipc.md section 5), the shell->core
// counterpart of the bus's core->shell BusClient. The palette issues real
// commands through it: start_explore (teach), dry_run (preview), and
// run_saved_workflow (run), with list_workflows sourcing the palette's saved-
// workflow rows. In dev/Demo this mock drives the same canned bus stream as
// before so the flight recorder still fills; the real shell swaps in a Tauri-
// backed CoreCommands (ui/src/bus/commands.ts), the same drop-in seam
// createMockBusClient is. start_explore's foreground-window context comes from
// readForegroundWindowProcess (the OS foreground window is ui/src-tauri's to
// resolve; dev/Demo uses a deterministic stub).
const coreCommands = createMockCoreCommands(bus, { registry, foregroundWindow: readForegroundWindowProcess });

const library = createLibrary(bus, {
  registry,
  onScheduleRequested: (_name, title) => {
    scheduleNotice = libraryStrings.scheduleNotice(title);
    renderLibraryPanel();
  },
});
// Shares Library's own registry instance (not a second createMockRegistry())
// so Up next/Recent runs show the exact same plain-language titles Library
// does for the same workflow name. The source is the real IPC data path
// (get_metrics/list_runs/get_run/list_triggers) under Tauri, and undefined in
// dev/Demo so the dashboard falls back to ./dashboard/mockMetrics.ts fixtures.
const dashboard = createDashboard(bus, { registry, source: createTauriDashboardSource() });
// Real config when running inside Tauri: get_settings/set_settings and a
// config.changed subscription onto the live core (contracts/ipc.md section
// 5f, via ./settings/liveStore.ts). Outside Tauri (npm run dev / Demo mode)
// getTauriInvoke() is null and this falls back to the mock store, unchanged.
const settingsInvoke = getTauriInvoke();
const settings = settingsInvoke
  ? createSettings(bus, { store: createLiveSettingsStore(bus, { invoke: settingsInvoke }) })
  : createSettings(bus);
// Shares Library's own registry instance too (see the dashboard comment
// just above), so the tray's Quick Runs menu (docs/specs/design.md section
// 3, Tray) shows the same plain-language titles Library's cards do for the
// same workflow name.
const tray = createTray(bus, { registry });
const toasts = createToasts(bus);

// The command palette (docs/specs/design.md section 3, Palette): a Raycast-
// grade floating overlay, opened by the global Ctrl+K/Cmd+K hotkey handled
// near the bottom of this file, mounted into op-palette-mount inside the
// op-palette-backdrop modal (same "mount once, gate visibility with the
// backdrop's hidden attribute" pattern as op-grant-backdrop/op-wizard-
// backdrop below). The palette fuzzy-matches over three source kinds
// (ui/src/palette/catalog.ts's PaletteEntryKind): saved workflows (this
// registry, kept live via registry.subscribe below, the same registry
// Library and the dashboard already share), quick actions, and settings
// sections (ui/src/palette/quickActions.ts, both static).
const paletteController = createPaletteController();

function refreshPaletteEntries(): void {
  // Saved-workflow rows come from list_workflows (contracts/ipc.md section 5c),
  // not the registry directly: the mock reads the shared registry in dev/Demo,
  // and a real core answers the same command. registry.subscribe still drives
  // the refresh so a newly taught/installed workflow shows up next open.
  const workflowEntries: PaletteEntry[] = coreCommands.listWorkflows().map((workflow) => ({
    id: workflow.id,
    kind: "workflow",
    title: workflow.description || workflow.name,
    subtitle: workflow.description ? workflow.name : undefined,
    keywords: [workflow.name],
  }));
  paletteController.setEntries([...workflowEntries, ...buildQuickActionEntries(), ...buildSettingsEntries()]);
}
registry.subscribe(refreshPaletteEntries);
refreshPaletteEntries();

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
// The engine-config seam (ui/src/wizard/engine.ts): a mocked configurator that
// writes real config.changed onto the same bus a live core would echo on. Swap
// this for a real invoke-backed configurator when the Tauri command bridge
// lands, the same drop-in the mock bus client itself will get.
const backendConfigurator = createMockBackendConfigurator(bus);
const wizard = createWizard(bus, { backend: backendConfigurator });
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

// docs/specs/design.md section 3's nav map. design.md section 3 calls the
// Home dashboard "the new default window view"; now that D4 (ui/src/
// dashboard/) has filled in its real content (hero, sparkline, Up next,
// Recent runs), this is that one-line flip.
type Screen = "dashboard" | "runs" | "library" | "settings";
let activeScreen: Screen = "dashboard";
// Which of the Settings screen's own sidebar sections is showing
// (docs/specs/design.md section 3.3). Same "own a local variable, pass it
// back into the mount function" pattern as activeScreen above, just scoped
// one level deeper.
let activeSettingsSection: SettingsSection = "general";

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

  // The compact run-state indicator at the top of the Runs screen (the
  // palette itself moved out to its own floating overlay, docs/specs/
  // design.md section 3; this status line is what stayed behind).
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
    // H1: the empty state's one specific action, before any run has ever
    // started (ui/src/runViewer/view.ts). Opens the same command palette the
    // dashboard and library empty states do, below.
    onTeach: () => openPalette(),
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
    toasts.dismiss();
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

/**
 * The bottom-right toast (design.md section 3's Toasts: "Bottom-right, one
 * line, verb-first... Amber only when an action is invited"), F11:
 * ui/src/toasts/ owns the state and the DOM building; this just mounts it
 * and wires its one action to the same ui/src/undo screen renderUndoEntry
 * above opens, for whichever run raised it.
 */
function renderToastPanel(): void {
  mountToast(toastMount, toasts.getSnapshot(), {
    onAction: (runId) => {
      toasts.dismiss();
      undoScreen.open(runId);
    },
  });
}

function closeExplain(): void {
  explainPanel.hidden = true;
  explainMount.textContent = "";
}

async function openExplain(name: string): Promise<void> {
  // library.explain is async because, with the real bridge wired, it round-trips
  // the explain_workflow command (contracts/ipc.md section 5c); in dev/Demo it
  // resolves the same locally rendered view. Callers below fire-and-forget it.
  const view = await library.explain(name);
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
    coreCommands.runSavedWorkflow(name);
    return;
  }

  const prompt = createGrantPrompt(caps, {
    onAllow: () => {
      coreCommands.runSavedWorkflow(name);
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

/**
 * Ctrl+Enter in the palette (design.md section 3's footer hint, rendered
 * on screen as "preview": contracts/microcopy_glossary.json maps that same
 * internal concept to that exact user-facing word). A preview never
 * touches library.run's own runtime bookkeeping (last-run time, minutes-
 * saved): those figures mean "the last time this actually ran," and a
 * preview, by definition, performs nothing for real, so it does not count.
 * Never needs a grant prompt either, for the same reason requestRun's
 * does: there is nothing here to approve.
 */
function previewWorkflow(name: string): void {
  // dry_run (contracts/ipc.md section 5b): the offline, deterministic preview
  // path. The mock CoreCommands publishes the same run.*(mode dry) pair this
  // did inline before; a real core streams it back over the bus instead.
  coreCommands.dryRunWorkflow(name);
}

/** Where a chosen palette quick action (ui/src/palette/quickActions.ts) actually lands: every id there must be handled here. */
function runQuickAction(id: string): void {
  switch (id) {
    case PALETTE_ACTION_ID.navDashboard:
      showScreen("dashboard");
      return;
    case PALETTE_ACTION_ID.navLibrary:
      showScreen("library");
      return;
    case PALETTE_ACTION_ID.navRuns:
      showScreen("runs");
      return;
    case PALETTE_ACTION_ID.navSettings:
      showScreen("settings");
      return;
    case PALETTE_ACTION_ID.cycleTheme:
      themeStore.cycle();
      return;
  }
}

/**
 * Turns a committed palette row (ui/src/palette/state.ts's PaletteController.commit)
 * into the same actions the rest of the shell already offers elsewhere:
 * running or previewing a workflow reuses requestRun/previewWorkflow above
 * (the exact grant-flow-aware path Library's own Run button uses), Tab-for-
 * details reuses openExplain, and a picked settings entry or quick action
 * just switches screens (ui/src/settings/view.ts has no separate routes to
 * deep-link into; see ui/src/palette/quickActions.ts's own header comment).
 * A run or a teach run also switches to the Runs screen so the flight
 * recorder that started is the thing actually on screen afterward.
 */
function handlePaletteCommit(commit: PaletteCommit): void {
  const { row, intent } = commit;
  switch (row.kind) {
    case "workflow":
      if (intent === "run") {
        requestRun(row.id);
        showScreen("runs");
      } else if (intent === "preview") {
        previewWorkflow(row.id);
        showScreen("runs");
      } else {
        openExplain(row.id);
      }
      return;
    case "action":
      runQuickAction(row.id);
      return;
    case "setting":
      showScreen("settings");
      return;
    case "teach": {
      // The free-text-to-teach-run path, now the real start_explore command
      // (contracts/ipc.md section 5b): the typed goal plus the foreground
      // window as context, reached through the "Teach this" fallback row. In
      // dev/Demo the mock streams the canned run and hands back a canceller;
      // the real transport returns null and a stop is a separate command.
      const stop = coreCommands.startExplore(row.subtitle ?? row.title);
      if (stop) {
        stopDemo?.();
        stopDemo = stop;
      }
      showScreen("runs");
      return;
    }
  }
}

function renderLibraryPanel(): void {
  const snapshot = library.getSnapshot();
  mountLibrary(libraryMount, snapshot, {
    onRun: requestRun,
    onSchedule: (name) => library.schedule(name),
    onExplain: openExplain,
    onReorder: (name, beforeName) => library.reorder(name, beforeName),
    onSearchChange: (query) => library.setSearchQuery(query),
    // H1: the empty state's one specific action, shown only with zero saved
    // workflows (ui/src/library/state.ts's emptyActionLabel). Same palette
    // the dashboard's own empty state opens, below.
    onTeach: () => openPalette(),
  });
  if (scheduleNotice) {
    const notice = document.createElement("p");
    notice.className = "op-settings__hint";
    notice.textContent = scheduleNotice;
    libraryMount.append(notice);
  }
}

function renderDashboardPanel(): void {
  // H1 (docs/specs/design.md section 3's Wizard finish screen, reused for
  // this screen's own first-run invite: "a single amber 'Teach your first
  // workflow' button"): opens the same command palette Ctrl+K does, below.
  mountDashboard(dashboardMount, dashboard.getSnapshot(), { onTeach: () => openPalette() });
}

function reportBackupProblem(): void {
  const notice = document.createElement("p");
  notice.className = "op-settings__hint";
  notice.textContent = settingsDetailStrings.backupInvalid;
  settingsMount.append(notice);
}

function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

function downloadBackup(payload: BackupPayload): void {
  downloadBlob(
    new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" }),
    `operant-backup-${payload.exportedAt.slice(0, 10)}.json`,
  );
}

// export_backup returns the archive as base64; decode to the raw JSON bytes
// (crates/recorder/src/backup.rs exports JSON-encoded bytes) and save them.
function downloadBackupArchive(bytesB64: string): void {
  downloadBlob(
    new Blob([base64ToBytes(bytesB64)], { type: "application/json" }),
    `operant-backup-${new Date().toISOString().slice(0, 10)}.json`,
  );
}

function exportBackup(): void {
  const archive = settings.exportBackupArchive();
  if (archive) archive.then(downloadBackupArchive).catch(reportBackupProblem);
  else downloadBackup(settings.exportBackup());
}

function importBackupFile(file: File): void {
  // Live store: hand the raw file bytes to import_backup as base64. Mock store
  // (dev/Demo): parse the settings-only JSON BackupPayload as before.
  if (settings.supportsBackupArchive()) {
    file
      .arrayBuffer()
      .then((buf) => settings.importBackupArchive(bytesToBase64(new Uint8Array(buf))) ?? Promise.resolve())
      .catch(reportBackupProblem);
    return;
  }
  file
    .text()
    .then((text) => settings.importBackup(JSON.parse(text) as BackupPayload))
    .catch(reportBackupProblem);
}

function renderSettingsPanel(): void {
  mountSettings(settingsMount, settings.getSnapshot(), {
    activeSection: activeSettingsSection,
    onSelectSection: (section) => {
      activeSettingsSection = section;
      renderSettingsPanel();
    },
    onVoiceToggle: (on) => settings.setVoiceEnabled(on),
    onSpeakingRateChange: (rate) => settings.setSpeakingRate(rate),
    onWatchAndSuggestToggle: (on) => settings.setWatchAndSuggest(on),
    onPurge: () => settings.purgeWatchedData(),
    onStartChordRecording: () => settings.startChordRecording(),
    onCancelChordRecording: () => settings.cancelChordRecording(),
    onExportBackup: exportBackup,
    onImportBackupFile: importBackupFile,
    onAutoUpdateToggle: (on) => settings.setAutoUpdateEnabled(on),
    // Appearance section: ui/src/theme/store.ts is another lane's module;
    // this only reads its current mode and calls its existing setter, the
    // same public API the header's own theme toggle (below) already uses.
    themeMode: themeStore.get(),
    onSetTheme: (mode) => themeStore.set(mode),
    onAccentSyncToggle: (on) => settings.setAccentSync(on),
    // Advanced section: ui/src/state/mode.ts is likewise another lane's
    // module; toggling it here flips the exact same store the header's
    // Default/Advanced button does, so the two never disagree.
    advancedModeOn: modeStore.get() === "advanced",
    onToggleAdvancedMode: () => modeStore.toggle(),
  });
}

/**
 * The tray preview (docs/specs/design.md section 3, Tray): the glyph
 * trigger plus its click-to-open menu and notifications. The menu's Quick
 * Runs only ever supply a workflow name; requestRun below is the same
 * capability-grant-aware path Library's own Run button and a picked
 * palette workflow row already use (ui/src/main.ts's own requestRun
 * function), so a Quick Run is never a second, tray-private way to start a
 * workflow. Open switches to the Home dashboard (design.md section 3: "the
 * new default window view"), the closest in-page stand-in this shell has
 * for "bring the real OS window to the front," which is ui/src-tauri's job,
 * out of this lane's owned path. Every action closes the menu afterward,
 * the same way choosing an item in a real OS tray menu would.
 */
function renderTrayPanel(): void {
  mountTray(trayMount, tray.getSnapshot(), {
    onDismissNotification: (id) => tray.dismissNotification(id),
    onToggleMenu: () => tray.toggleMenu(),
    onCloseMenu: () => tray.closeMenu(),
    onQuickRun: (name) => {
      requestRun(name);
      tray.closeMenu();
    },
    onOpen: () => {
      showScreen("dashboard");
      tray.closeMenu();
    },
    onPauseAll: () => {
      tray.pauseAll();
      tray.closeMenu();
    },
    onPanic: () => {
      tray.panic();
      tray.closeMenu();
    },
  });
}

/**
 * The palette overlay (design.md section 3): mounted unconditionally, same
 * as renderWizardPanel/requestRun's grant prompt below, with
 * op-palette-backdrop's own `hidden` attribute the only thing gating
 * whether it is actually visible and reachable. A commit
 * (ui/src/palette/state.ts's PaletteController.commit, reached through
 * Enter/Ctrl+Enter/Tab in ui/src/palette/view.ts's own keydown handling, or
 * a click) hands back what was picked and for what; handlePaletteCommit
 * above decides what that actually does.
 */
function renderPalette(): void {
  const snapshot = paletteController.getSnapshot();
  paletteBackdrop.hidden = !snapshot.open;
  mountPalette(paletteMount, snapshot, {
    onQueryChange: (text) => paletteController.setQuery(text),
    onMoveSelection: (delta) => paletteController.moveSelection(delta),
    onCommit: (intent, rowId) => {
      const commit = paletteController.commit(intent, rowId);
      if (commit) handlePaletteCommit(commit);
    },
    onClose: () => paletteController.close(),
  });
}

/**
 * First-run tour (ui/src/tour/*, H1: re-pointed at the new nav so it
 * completes on it -- Dashboard, Library, Runs, Settings, docs/specs/
 * design.md section 3's nav map). Held back while the wizard modal is still
 * up (renderWizardPanel below calls this every time it renders, so the
 * callout appears the moment wizardDismissed flips true, rather than
 * stacking a second overlay on top of the wizard's own full-window
 * takeover). Dismissing a callout ("Got it") both advances the tour and
 * switches to the screen its next callout is about (the tourStore.subscribe
 * below), so the tour actually walks the person across the nav instead of
 * narrating it from wherever they happen to already be sitting.
 */
function renderTour(): void {
  if (!wizardDismissed) {
    tourMount.textContent = "";
    return;
  }
  mountTourCallout(tourMount, tourStore.getSnapshot(), {
    onDismiss: () => tourStore.nextStep(),
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
  renderTour();
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
runViewer.subscribe(renderUndoEntry);
library.subscribe(renderLibraryPanel);
dashboard.subscribe(renderDashboardPanel);
settings.subscribe(renderSettingsPanel);
// The Settings screen's Appearance and Advanced sections mirror these two
// stores (see renderSettingsPanel above); re-render Settings whenever either
// changes, including from the header's own theme/mode toggles, so the two
// entry points never show stale state relative to each other.
themeStore.subscribe(() => renderSettingsPanel());
modeStore.subscribe(renderSettingsPanel);
tray.subscribe(renderTrayPanel);
toasts.subscribe(renderToastPanel);
wizard.subscribe(renderWizardPanel);
undoScreen.subscribe(renderUndoScreen);
paletteController.subscribe(renderPalette);
// H1: dismissing a tour callout both advances it and, for every step short
// of "done" (all four of which share ui/src/main.ts's own Screen type,
// ui/src/tour/state.ts's TourStep), switches to the screen that next
// callout is about, so the tour actually walks the new nav rather than only
// narrating it from wherever the person happens to already be.
tourStore.subscribe((snap) => {
  if (snap.step !== "done") showScreen(snap.step);
  renderTour();
});

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
// Load the dashboard's real numbers (metrics, recent runs, upcoming) once the
// panel and its subscription are wired above. A no-op in dev/Demo (no source);
// under Tauri it replaces the honest empty baseline with real data via the
// dashboard.subscribe(renderDashboardPanel) emit.
void dashboard.refresh();
renderSettingsPanel();
renderTrayPanel();
renderWizardPanel();
renderUndoScreen();
renderToastPanel();
renderPalette();

// Stop, Pause, intervene, and filmstrip scrubbing are wired through
// mountRunViewer's callbacks in renderRunViewer() above, not to static ids.
// Enter/Ctrl+Enter/Tab/Escape inside the palette itself are wired through
// mountPalette's own keydown handling in renderPalette() above; only the
// global summon hotkey and the click-outside-to-dismiss below are this
// file's to wire, the same split the wizard/grant modals already use
// (their own Escape/Tab handling lives in ui/src/wizard/view.ts, not here).

/**
 * Ctrl+K/Cmd+K opens the palette from anywhere in the shell (design.md
 * section 3: "a Raycast-grade... floating panel", reachable via "the
 * existing Ctrl+K/Cmd+K global hotkey"). Declines while the wizard or the
 * grant prompt is already up: both are their own modal already covering the
 * screen, and opening a second one on top would stack two competing
 * backdrops rather than reach either sensibly.
 */
function openPalette(): void {
  if (!wizardBackdrop.hidden || !grantBackdrop.hidden) return;
  paletteController.open();
}

paletteBackdrop.addEventListener("click", (event) => {
  // Only a direct click on the dimmed backdrop itself dismisses the
  // palette; a click that bubbled up from inside the floating panel must
  // not (the panel is a descendant of the backdrop, so every click inside
  // it also fires here unless this check narrows to the backdrop itself).
  if (event.target === paletteBackdrop) paletteController.close();
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
  // The global kill chord (docs/specs/guardian.md's panic chord, default
  // Ctrl+Alt+Shift+Space, re-recordable in Settings). SAFETY, never-cut: the
  // second, always-reachable trigger for the same two-path stop the tray's
  // panic row drives (tray.panic() -> ui/src/safety/panic.ts's stop + kill), so
  // a wedged or backgrounded window still has a way to halt a live loop.
  // Matched against the live configured chord so a re-recorded combination
  // takes effect at once; checked before the palette hotkey because a stop
  // outranks opening an overlay.
  if (formatChord(chordPartsFromEvent(event)) === settings.getSnapshot().state.killSwitchChord) {
    event.preventDefault();
    tray.panic();
    return;
  }
  if (isGlobalPaletteHotkey(event)) {
    event.preventDefault();
    openPalette();
  }
});

/**
 * "Check my setup" (the doctor screen, ui/src/doctor/*, C19/FR-U3). Unmounted
 * before this lane; wired here per docs/specs/ipc-bridge.md section 8b ("Doctor
 * + Gallery exist but are unmounted; wire if time permits"). Everything doctor
 * lives in this one appended block so the edit stays additive and merges
 * cleanly with the other lanes editing this file.
 *
 * The header's "Check my setup" button opens the modal and runs the checks; the
 * findings render from doctor.finding events, the exact events a real core
 * emits, so nothing here hard-codes a finding list into the view.
 */
const doctorOpenButton = byId<HTMLButtonElement>("op-doctor-open");
const doctorBackdrop = byId<HTMLElement>("op-doctor-backdrop");
const doctorMount = byId<HTMLElement>("op-doctor-mount");
doctorOpenButton.textContent = doctorStrings.title;

// The desktop app's command bridge, when the shell runs inside Tauri.
// contracts/ipc.md's run_doctor command is issued through it; a plain browser
// (dev/Demo, the mock bus) has no bridge, so the callers below fall back to
// canned findings. A later lane (the real BusClient) formalizes this seam; the
// doctor screen only needs "issue the command if we can, else show the demo."
type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
function tauriInvoke(): TauriInvoke | null {
  const g = globalThis as unknown as {
    __TAURI__?: { core?: { invoke?: TauriInvoke }; invoke?: TauriInvoke };
  };
  return g.__TAURI__?.core?.invoke ?? g.__TAURI__?.invoke ?? null;
}

const doctor = createDoctor(bus, {
  // On open: run the real checks (contracts/ipc.md section 5f, run_doctor). In
  // the app the core runs every check and publishes each result as a
  // doctor.finding event; in dev/Demo there is no core, so publish the canned
  // findings on the mock bus. Both land on the subscription below, so the
  // render path is identical. Determinism: the demo path calls no model and no
  // network (the real probing runs core-side, only in the app, never on the
  // replay/test path).
  runChecks: () => {
    const invoke = tauriInvoke();
    if (invoke) {
      void invoke("run_doctor", {});
      return;
    }
    for (const finding of DEMO_DOCTOR_FINDINGS) bus.publish("doctor.finding", finding);
  },
  // One-click fix. In the app this maps to `operant doctor --fix <id>` (the
  // finding's fix_command), issued as an optional `fix` arg to run_doctor -- an
  // additive, protocol-compatible extension per contracts/ipc.md section 9.2 --
  // and the core re-checks and republishes the finding, healthy. In dev/Demo,
  // stand in for that effect by publishing doctor.fixed plus the canned healthy
  // finding, so the card turns healthy in place either way.
  onFixRequested: (findingId) => {
    const invoke = tauriInvoke();
    if (invoke) {
      void invoke("run_doctor", { fix: findingId });
      return;
    }
    bus.publish("doctor.fixed", { finding_id: findingId });
    const healthy = demoHealthyFinding(findingId);
    if (healthy) bus.publish("doctor.finding", healthy);
  },
});

function renderDoctor(): void {
  mountDoctor(doctorMount, doctor.getSnapshot(), {
    onFix: (findingId) => doctor.fix(findingId),
    onClose: () => closeDoctor(),
  });
}

function openDoctor(): void {
  doctorBackdrop.hidden = false;
  doctor.open();
}

function closeDoctor(): void {
  doctorBackdrop.hidden = true;
}

doctor.subscribe(renderDoctor);
doctorOpenButton.addEventListener("click", openDoctor);
doctorBackdrop.addEventListener("click", (event) => {
  // Only a click on the dimmed backdrop itself dismisses the modal, not one
  // that bubbled up from inside the panel (the same narrowing the palette
  // backdrop above uses).
  if (event.target === doctorBackdrop) closeDoctor();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !doctorBackdrop.hidden) closeDoctor();
});
renderDoctor();
}
