// The onboarding wizard (C19, FR-U1/FR-U4; docs/specs/zero-code.md's five
// screens: welcome, "How should Operant think?", mic check, guided first
// task, schedule). Turns a handful of user actions plus the bus
// (contracts/bus_events.md) into what each screen should show. Pure and
// DOM-free, same split as every other module in ui/src (ui/src/library/state.ts,
// ui/src/settings/state.ts): runs under plain `node --test`; ui/src/main.ts
// binds it to the page and ui/src/wizard/view.ts renders it.
//
// The guided task and the "Just show me a demo" link both run against the
// bus a live run would use (ui/src/wizard/guidedTask.ts), through an
// internal ui/src/runViewer/state.ts instance the same way main.ts's own Run
// screen does, so the two stay in sync for free: the main shell's tray and
// run viewer see the wizard's own run exactly like any other run, with zero
// duplicated step-rendering logic.

import type { BusClient } from "../bus/mockClient.ts";
import { createRunViewer, type RunViewerSnapshot } from "../runViewer/state.ts";
import { runGuidedTask } from "./guidedTask.ts";
import { detectProviderFromKey, type Provider } from "./accessKey.ts";
import {
  startDownload,
  probeCompatibility,
  checkDiskSpace,
  formatBytes,
  type DownloadHandle,
  type DownloadEnvelope,
  type CompatibilityLevel,
  type DownloadErrorCode,
} from "./downloader.ts";
import type { ScreenContent } from "./mediaPresence.ts";
import {
  createMockBackendConfigurator,
  type BackendConfigurator,
  type BackendProvider,
  type ProbeState,
} from "./engine.ts";
import {
  welcomeStrings,
  setupPathStrings,
  providerDisplayNames,
  micCheckStrings,
  guidedTaskStrings,
  scheduleStrings,
  downloadErrorStrings,
  engineStatusStrings,
} from "./strings.ts";

export type WizardScreenId = "welcome" | "setup_path" | "mic_check" | "guided_task" | "schedule";

export type LocalPhase =
  | "idle"
  | "checking_disk"
  | "disk_low"
  | "checking_compat"
  | "compat_fail"
  | "starting"
  | "resuming"
  | "downloading"
  | "paused"
  | "verifying"
  | "complete"
  | "failed";

const RESTARTABLE_LOCAL_PHASES = new Set<LocalPhase>(["idle", "disk_low", "compat_fail", "failed"]);

export type ScheduleOptionId = "manual" | "daily" | "weekly" | "when_file_changes" | "when_app_opens" | "when_email_arrives";

const SCHEDULE_OPTION_IDS: readonly ScheduleOptionId[] = [
  "manual",
  "daily",
  "weekly",
  "when_file_changes",
  "when_app_opens",
  "when_email_arrives",
];

// Per-provider config the wizard writes through configure_backend
// (docs/specs/ipc-bridge.md section 7: "Config::set model/provider/key"). The
// wizard has no model picker (docs/specs/zero-code.md keeps first-run choice
// minimal), so it writes a sensible default the shell/core may resolve or
// override; a real model-selection surface is a later concern. The local path
// points at the on-device endpoint the local-model sidecar serves; that
// download plus VRAM/disk sidecar is a separate IPC surface whose live wiring
// is a C2 follow-up.
const PROVIDER_DEFAULTS: Record<BackendProvider, { model: string; endpoint?: string }> = {
  chatgpt: { model: "gpt-4o" },
  claude: { model: "claude-sonnet-5" },
  local: { model: "llama3.1:8b", endpoint: "http://127.0.0.1:11434" },
};

interface LocalState {
  phase: LocalPhase;
  percent: number;
  diskOk: boolean | null;
  diskNeededLabel: string;
  compatLevel: CompatibilityLevel | null;
  errorCode: DownloadErrorCode | null;
  handle: DownloadHandle | null;
}

const INITIAL_LOCAL: LocalState = {
  phase: "idle",
  percent: 0,
  diskOk: null,
  diskNeededLabel: "",
  compatLevel: null,
  errorCode: null,
  handle: null,
};

interface AccessKeyState {
  text: string;
  detected: Provider | null;
  manual: Provider | null;
}

interface MicCheckState {
  played: boolean;
  level: number;
}

interface GuidedTaskState {
  demo: boolean;
  saved: boolean;
  stop: (() => void) | null;
}

interface ScheduleState {
  selected: ScheduleOptionId | null;
}

interface BackendState {
  provider: BackendProvider | null;
  model: string | null;
  /** True once a setup path has written real config through configure_backend. */
  configured: boolean;
  /** The honest probe state; `not_implemented` while probe_backend is unwired. */
  probe: ProbeState;
}

interface InternalState {
  screen: WizardScreenId;
  complete: boolean;
  local: LocalState;
  accessKey: AccessKeyState;
  micCheck: MicCheckState;
  guidedTask: GuidedTaskState;
  schedule: ScheduleState;
  backend: BackendState;
}

export interface StaticCardSnapshot {
  title: string;
  body: string;
  button: string;
}

export interface LocalCardSnapshot {
  title: string;
  body: string;
  /** design.md section 3: the local-model download card "shows size". A plain-language rendering of how big the download is, always present (unlike diskLabel/compatLabel, which stay null until their own check has run). */
  sizeLabel: string;
  phase: LocalPhase;
  diskLabel: string | null;
  compatLabel: string | null;
  percent: number;
  progressLabel: string;
  buttonLabel: string;
  buttonDisabled: boolean;
  showPauseResume: boolean;
  pauseResumeLabel: string;
  showCancel: boolean;
  cancelLabel: string;
  showContinue: boolean;
  continueLabel: string;
  errorWhat?: string;
  errorWhy?: string;
  errorAction?: string;
}

export interface AccessKeyCardSnapshot {
  title: string;
  body: string;
  placeholder: string;
  text: string;
  providerLabel: string;
  detectedLabel: string | null;
  showManualPicker: boolean;
  manual: Provider | null;
  buttonLabel: string;
  buttonDisabled: boolean;
}

export interface MicCheckSnapshot {
  heading: string;
  body: string;
  sampleButton: string;
  levelMeterLabel: string;
  level: number;
  played: boolean;
  skipButton: string;
  continueButton: string;
}

export interface GuidedTaskStepView {
  id: string;
  sentence: string;
  status: string;
}

export interface GuidedTaskSnapshot {
  heading: string;
  intro: string;
  demo: boolean;
  steps: GuidedTaskStepView[];
  runningLabel: string;
  doneLabel: string;
  done: boolean;
  saveButton: string;
  saved: boolean;
  savedHint: string;
  demoContinueButton: string;
  demoContinueHint: string;
  canSave: boolean;
  canContinueDemo: boolean;
}

export interface ScheduleSnapshot {
  heading: string;
  body: string;
  options: { id: ScheduleOptionId; label: string }[];
  selected: ScheduleOptionId | null;
  continueButton: string;
  canContinue: boolean;
}

export interface BackendSnapshot {
  /** True once a setup path wrote real config; the confirmation renders only then. */
  configured: boolean;
  provider: BackendProvider | null;
  model: string | null;
  /** The honest probe state (never a faked green result while probe_backend is unwired). */
  probeState: ProbeState;
  /** "You're set up with X." Empty until configured. */
  confirmLabel: string;
  /** The honest probe line; empty until configured. Never claims a working connection unless it is one. */
  probeLabel: string;
}

export interface WizardSnapshot {
  screen: WizardScreenId;
  complete: boolean;
  welcome: { heading: string; body: string; continueButton: string };
  setupPath: {
    heading: string;
    subheading: string;
    demoLink: string;
    chatgpt: StaticCardSnapshot;
    claude: StaticCardSnapshot;
    local: LocalCardSnapshot;
    accessKey: AccessKeyCardSnapshot;
  };
  micCheck: MicCheckSnapshot;
  guidedTask: GuidedTaskSnapshot;
  schedule: ScheduleSnapshot;
  backend: BackendSnapshot;
  /** The active screen's content, for the media-presence check (ui/src/wizard/mediaPresence.ts). */
  mediaContent: ScreenContent;
}

type ScreenData = Omit<WizardSnapshot, "screen" | "complete" | "mediaContent">;

export interface DownloadSimOptions {
  totalBytes?: number;
  ticks?: number;
  tickMs?: number;
  failAt?: number;
  failCode?: DownloadErrorCode;
}

export interface CreateWizardOptions {
  /** Simulated free disk space, bytes. Defaults to plenty of room. */
  diskFreeBytes?: number;
  /** Simulated space the local model needs, bytes. Defaults to 4 GB. */
  diskNeededBytes?: number;
  /** Simulated available graphics memory, MB. Defaults to plenty. */
  vramMb?: number;
  vramMinMb?: number;
  vramSlowMb?: number;
  download?: DownloadSimOptions;
  /** Delay between guided-task steps, ms. Tests pass something tiny. */
  guidedTaskStepDelayMs?: number;
  /**
   * The engine-config seam. Defaults to a mocked configurator wired to `bus`
   * (ui/src/wizard/engine.ts); ui/src/main.ts injects it explicitly so a real
   * invoke-backed configurator can be swapped in later without touching the
   * wizard, and tests inject spies to assert what config gets written.
   */
  backend?: BackendConfigurator;
}

export interface Wizard {
  getSnapshot(): WizardSnapshot;
  subscribe(fn: (snap: WizardSnapshot) => void): () => void;

  continueWelcome(): void;

  chooseChatGPT(): void;
  chooseClaude(): void;

  startLocalDownload(): void;
  pauseLocalDownload(): void;
  resumeLocalDownload(): void;
  cancelLocalDownload(): void;
  continueAfterLocalDownload(): void;

  setAccessKeyText(text: string): void;
  chooseProviderManually(provider: Provider): void;
  continueWithAccessKey(): void;

  startDemo(): void;

  playMicSample(): void;
  skipMicCheck(): void;
  continueMicCheck(): void;

  saveAsWorkflow(): void;
  continueAfterDemo(): void;

  chooseSchedule(id: ScheduleOptionId): void;
  finishSchedule(): void;

  dispose(): void;
}

function localSnapshot(local: LocalState, diskNeededBytes: number): LocalCardSnapshot {
  const L = setupPathStrings.cards.local;
  const percent = Math.round(local.percent);
  const checking = local.phase === "checking_disk" || local.phase === "checking_compat";
  const handleActive =
    local.phase === "starting" || local.phase === "resuming" || local.phase === "downloading" || local.phase === "paused" || local.phase === "verifying";

  let progressLabel = "";
  let errorWhat: string | undefined;
  let errorWhy: string | undefined;
  let errorAction: string | undefined;

  switch (local.phase) {
    case "checking_disk":
      progressLabel = L.diskCheckLabel;
      break;
    case "checking_compat":
      progressLabel = L.compatCheckLabel;
      break;
    case "starting":
      progressLabel = L.download.starting;
      break;
    case "resuming":
      progressLabel = L.download.resuming;
      break;
    case "downloading":
      progressLabel = L.download.downloading(percent);
      break;
    case "paused":
      progressLabel = L.download.paused;
      break;
    case "verifying":
      progressLabel = L.download.verifying;
      break;
    case "complete":
      progressLabel = L.download.complete;
      break;
    case "failed": {
      progressLabel = L.download.failed;
      const entry = local.errorCode === "CHECKSUM_MISMATCH" ? downloadErrorStrings.checksum_mismatch : downloadErrorStrings.network_error;
      errorWhat = entry.what;
      errorWhy = entry.why;
      errorAction = entry.action;
      break;
    }
    default:
      progressLabel = "";
  }

  return {
    title: L.title,
    body: L.body,
    sizeLabel: L.sizeLabel(formatBytes(diskNeededBytes)),
    phase: local.phase,
    diskLabel: local.diskOk === null ? null : local.diskOk ? L.diskCheckOk : L.diskCheckLow(local.diskNeededLabel),
    compatLabel:
      local.compatLevel === null
        ? null
        : local.compatLevel === "ok"
          ? L.compatCheckOk
          : local.compatLevel === "slow"
            ? L.compatCheckSlow
            : L.compatCheckFail,
    percent,
    progressLabel,
    buttonLabel: local.phase === "failed" ? L.download.retryButton : L.button,
    buttonDisabled: local.phase === "compat_fail" || checking || handleActive || local.phase === "complete",
    showPauseResume: handleActive,
    pauseResumeLabel: local.phase === "paused" ? L.download.resumeButton : L.download.pauseButton,
    showCancel: handleActive,
    cancelLabel: L.download.cancelButton,
    showContinue: local.phase === "complete",
    continueLabel: L.continueButton,
    errorWhat,
    errorWhy,
    errorAction,
  };
}

function accessKeySnapshot(a: AccessKeyState): AccessKeyCardSnapshot {
  const K = setupPathStrings.cards.accessKey;
  const provider = a.detected ?? a.manual;
  const trimmed = a.text.trim();
  return {
    title: K.title,
    body: K.body,
    placeholder: K.placeholder,
    text: a.text,
    providerLabel: K.providerLabel,
    detectedLabel: a.detected ? K.providerAutoDetected(providerDisplayNames[a.detected]) : trimmed ? K.providerPickManually : null,
    showManualPicker: !a.detected && trimmed.length > 0,
    manual: a.manual,
    buttonLabel: K.button,
    buttonDisabled: !(trimmed.length > 0 && provider !== null),
  };
}

function probeLabelFor(state: ProbeState): string {
  switch (state) {
    case "checking":
      return engineStatusStrings.probe.checking;
    case "reachable":
      return engineStatusStrings.probe.reachable;
    case "unreachable":
      return engineStatusStrings.probe.unreachable;
    case "not_implemented":
      return engineStatusStrings.probe.not_implemented;
    case "unavailable":
      return engineStatusStrings.probe.unavailable;
    default:
      return "";
  }
}

function backendSnapshot(b: BackendState): BackendSnapshot {
  return {
    configured: b.configured,
    provider: b.provider,
    model: b.model,
    probeState: b.probe,
    confirmLabel: b.configured && b.provider ? engineStatusStrings.confirm(engineStatusStrings.names[b.provider]) : "",
    probeLabel: b.configured ? probeLabelFor(b.probe) : "",
  };
}

function guidedTaskSnapshot(g: GuidedTaskState, runSnap: RunViewerSnapshot): GuidedTaskSnapshot {
  const G = guidedTaskStrings;
  const done = runSnap.runState === "done";
  return {
    heading: g.demo ? G.headingDemo : G.headingReal,
    intro: g.demo ? G.introDemo : G.introReal,
    demo: g.demo,
    steps: runSnap.steps.map((s) => ({ id: s.id, sentence: s.sentence, status: s.status })),
    runningLabel: G.runningLabel,
    doneLabel: G.doneLabel,
    done,
    saveButton: G.saveButton,
    saved: g.saved,
    savedHint: G.savedHint,
    demoContinueButton: G.demoContinueButton,
    demoContinueHint: G.demoContinueHint,
    canSave: done && !g.demo && !g.saved,
    canContinueDemo: done && g.demo,
  };
}

function mediaContentFor(screen: WizardScreenId, data: ScreenData): ScreenContent {
  switch (screen) {
    case "welcome":
      return { screen, visible: [data.welcome.heading, data.welcome.body, data.welcome.continueButton] };
    case "setup_path":
      return {
        screen,
        visible: [
          data.setupPath.heading,
          data.setupPath.subheading,
          data.setupPath.chatgpt.title,
          data.setupPath.claude.title,
          data.setupPath.local.title,
          data.setupPath.accessKey.title,
          data.setupPath.demoLink,
        ],
      };
    case "mic_check":
      return {
        screen,
        visible: [
          data.micCheck.heading,
          data.micCheck.body,
          data.micCheck.levelMeterLabel,
          // Present only once a real path configured a backend; the media-presence
          // check filters the empty strings the demo path leaves here.
          data.backend.confirmLabel,
          data.backend.probeLabel,
        ],
        audible: { cueLabel: data.micCheck.sampleButton },
      };
    case "guided_task":
      return {
        screen,
        visible: [data.guidedTask.heading, data.guidedTask.intro, ...data.guidedTask.steps.map((s) => s.sentence)],
      };
    case "schedule":
      return {
        screen,
        visible: [data.schedule.heading, data.schedule.body, ...data.schedule.options.map((o) => o.label)],
      };
    default: {
      const exhaustive: never = screen;
      return exhaustive;
    }
  }
}

function toSnapshot(s: InternalState, runSnap: RunViewerSnapshot, diskNeededBytes: number): WizardSnapshot {
  const data: ScreenData = {
    welcome: { heading: welcomeStrings.heading, body: welcomeStrings.body, continueButton: welcomeStrings.continueButton },
    setupPath: {
      heading: setupPathStrings.heading,
      subheading: setupPathStrings.subheading,
      demoLink: setupPathStrings.demoLink,
      chatgpt: { ...setupPathStrings.cards.chatgpt },
      claude: { ...setupPathStrings.cards.claude },
      local: localSnapshot(s.local, diskNeededBytes),
      accessKey: accessKeySnapshot(s.accessKey),
    },
    micCheck: {
      heading: micCheckStrings.heading,
      body: micCheckStrings.body,
      sampleButton: micCheckStrings.sampleButton,
      levelMeterLabel: micCheckStrings.levelMeterLabel,
      level: s.micCheck.level,
      played: s.micCheck.played,
      skipButton: micCheckStrings.skipButton,
      continueButton: micCheckStrings.continueButton,
    },
    guidedTask: guidedTaskSnapshot(s.guidedTask, runSnap),
    schedule: {
      heading: scheduleStrings.heading,
      body: scheduleStrings.body,
      options: SCHEDULE_OPTION_IDS.map((id) => ({ id, label: scheduleStrings.options[id] })),
      selected: s.schedule.selected,
      continueButton: scheduleStrings.continueButton,
      canContinue: s.schedule.selected !== null,
    },
    backend: backendSnapshot(s.backend),
  };

  return {
    screen: s.screen,
    complete: s.complete,
    ...data,
    mediaContent: mediaContentFor(s.screen, data),
  };
}

export function createWizard(bus: BusClient, opts: CreateWizardOptions = {}): Wizard {
  const diskFreeBytes = opts.diskFreeBytes ?? 40_000_000_000;
  const diskNeededBytes = opts.diskNeededBytes ?? 4_000_000_000;
  const vramMb = opts.vramMb ?? 8000;
  const vramMinMb = opts.vramMinMb ?? 4000;
  const vramSlowMb = opts.vramSlowMb ?? 6000;
  const downloadOpts = opts.download ?? {};
  const backend = opts.backend ?? createMockBackendConfigurator(bus);

  let state: InternalState = {
    screen: "welcome",
    complete: false,
    local: { ...INITIAL_LOCAL },
    accessKey: { text: "", detected: null, manual: null },
    micCheck: { played: false, level: 0 },
    guidedTask: { demo: false, saved: false, stop: null },
    schedule: { selected: null },
    backend: { provider: null, model: null, configured: false, probe: "idle" },
  };

  // Guards for the async probe: `disposed` stops a late probe from touching a
  // torn-down wizard, and `backendGeneration` stops a stale probe (from a setup
  // path the user backed out of) clobbering a newer one. Determinism: the
  // config write is synchronous, only the probe label settles on a microtask.
  let disposed = false;
  let backendGeneration = 0;

  const listeners = new Set<(snap: WizardSnapshot) => void>();
  const runViewerInternal = createRunViewer(bus);
  const unsubscribeRunViewer = runViewerInternal.subscribe(() => emit());

  function emit(): void {
    const snap = toSnapshot(state, runViewerInternal.getSnapshot(), diskNeededBytes);
    for (const fn of listeners) fn(snap);
  }

  function abandonLocalDownload(): void {
    state.local.handle?.cancel();
  }

  function continueWelcome(): void {
    if (state.screen !== "welcome") return;
    state = { ...state, screen: "setup_path" };
    emit();
  }

  // Writes real engine config through configure_backend and kicks off the
  // honest probe. Never blocks navigation on the round trip and never keeps the
  // access key (it is handed to the configurator and dropped). Mutates state
  // but does not emit: the caller emits once, after it also sets the screen.
  function configureBackendFor(provider: BackendProvider, apiKey?: string): void {
    const def = PROVIDER_DEFAULTS[provider];
    const generation = ++backendGeneration;
    state = { ...state, backend: { provider, model: def.model, configured: true, probe: "checking" } };

    // configure_backend: provider + model (+ endpoint for local) become real
    // config; the shell/core takes the access key for secure storage. Fire and
    // forget: the config.changed echoes fire synchronously inside the mock.
    void backend.configureBackend({ provider, model: def.model, apiKey, endpoint: def.endpoint });

    // probe_backend: honest by contract. A not-yet-implemented core answers
    // not_implemented, which we surface as "probe unavailable"; a rejected probe
    // degrades to the same honest "could not check" state. Never a faked green.
    void backend.probeBackend({ provider, model: def.model, endpoint: def.endpoint }).then(
      (res) => {
        if (disposed || generation !== backendGeneration) return;
        state = { ...state, backend: { ...state.backend, probe: res.state } };
        emit();
      },
      () => {
        if (disposed || generation !== backendGeneration) return;
        state = { ...state, backend: { ...state.backend, probe: "unavailable" } };
        emit();
      },
    );
  }

  function chooseChatGPT(): void {
    if (state.screen !== "setup_path") return;
    abandonLocalDownload();
    configureBackendFor("chatgpt");
    state = { ...state, screen: "mic_check" };
    emit();
  }

  function chooseClaude(): void {
    if (state.screen !== "setup_path") return;
    abandonLocalDownload();
    configureBackendFor("claude");
    state = { ...state, screen: "mic_check" };
    emit();
  }

  function handleDownloadEvent(ev: DownloadEnvelope): void {
    switch (ev.topic) {
      case "download.started": {
        const resumedFrom = Number(ev.payload.resumedFrom ?? 0);
        state = {
          ...state,
          local: { ...state.local, phase: resumedFrom > 0 ? "resuming" : "downloading", percent: resumedFrom > 0 ? state.local.percent : 0 },
        };
        break;
      }
      case "download.progress": {
        const percent = Number(ev.payload.percent ?? 0);
        state = { ...state, local: { ...state.local, phase: percent >= 100 ? "verifying" : "downloading", percent } };
        break;
      }
      case "download.paused":
        state = { ...state, local: { ...state.local, phase: "paused" } };
        break;
      case "download.completed":
        state = { ...state, local: { ...state.local, phase: "complete", percent: 100 } };
        break;
      case "download.failed":
        state = {
          ...state,
          local: { ...state.local, phase: "failed", errorCode: (ev.payload.code as DownloadErrorCode) ?? "HTTP_ERROR" },
        };
        break;
      default:
        return;
    }
    emit();
  }

  function startLocalDownload(): void {
    if (state.screen !== "setup_path") return;
    if (!RESTARTABLE_LOCAL_PHASES.has(state.local.phase)) return;

    state = {
      ...state,
      local: { ...state.local, phase: "checking_disk", diskOk: null, compatLevel: null, errorCode: null, handle: null },
    };
    emit();

    const disk = checkDiskSpace(diskFreeBytes, diskNeededBytes);
    if (!disk.ok) {
      state = { ...state, local: { ...state.local, phase: "disk_low", diskOk: false, diskNeededLabel: formatBytes(disk.shortfallBytes) } };
      emit();
      return;
    }
    state = { ...state, local: { ...state.local, diskOk: true, phase: "checking_compat" } };
    emit();

    const compat = probeCompatibility(vramMb, vramMinMb, vramSlowMb);
    if (compat.level === "fail") {
      state = { ...state, local: { ...state.local, phase: "compat_fail", compatLevel: "fail" } };
      emit();
      return;
    }
    state = { ...state, local: { ...state.local, compatLevel: compat.level, phase: "starting", percent: 0 } };
    emit();

    const handle = startDownload({ ...downloadOpts, onEvent: handleDownloadEvent });
    state = { ...state, local: { ...state.local, handle } };
  }

  function pauseLocalDownload(): void {
    if (!state.local.handle) return;
    if (state.local.phase === "paused" || state.local.phase === "complete" || state.local.phase === "failed") return;
    state.local.handle.pause();
  }

  function resumeLocalDownload(): void {
    if (!state.local.handle || state.local.phase !== "paused") return;
    state.local.handle.resume();
  }

  function cancelLocalDownload(): void {
    abandonLocalDownload();
    state = { ...state, local: { ...INITIAL_LOCAL } };
    emit();
  }

  function continueAfterLocalDownload(): void {
    if (state.screen !== "setup_path" || state.local.phase !== "complete") return;
    // The download itself is a separate IPC surface (its live wiring is a C2
    // follow-up); here we write the config that selects the on-device model so
    // teaching runs against it afterward.
    configureBackendFor("local");
    state = { ...state, screen: "mic_check" };
    emit();
  }

  function setAccessKeyText(text: string): void {
    if (state.screen !== "setup_path") return;
    const detected = detectProviderFromKey(text);
    state = { ...state, accessKey: { text, detected, manual: detected ? null : state.accessKey.manual } };
    emit();
  }

  function chooseProviderManually(provider: Provider): void {
    if (state.screen !== "setup_path") return;
    state = { ...state, accessKey: { ...state.accessKey, manual: provider } };
    emit();
  }

  function continueWithAccessKey(): void {
    if (state.screen !== "setup_path") return;
    const provider = state.accessKey.detected ?? state.accessKey.manual;
    const key = state.accessKey.text.trim();
    if (!key || !provider) return;
    abandonLocalDownload();
    configureBackendFor(provider, key);
    // Key safety: hand the key off once, then drop it from webview state so no
    // snapshot, re-render, or later screen can hold or leak the raw value.
    state = { ...state, screen: "mic_check", accessKey: { text: "", detected: null, manual: null } };
    emit();
  }

  function beginGuidedTask(demo: boolean): void {
    state.guidedTask.stop?.();
    const { stop } = runGuidedTask(bus, { demo, stepDelayMs: opts.guidedTaskStepDelayMs });
    state = { ...state, screen: "guided_task", guidedTask: { demo, saved: false, stop } };
    emit();
  }

  function startDemo(): void {
    if (state.screen !== "setup_path") return;
    abandonLocalDownload();
    beginGuidedTask(true);
  }

  function playMicSample(): void {
    if (state.screen !== "mic_check") return;
    state = { ...state, micCheck: { played: true, level: 0.62 } };
    emit();
  }

  function skipMicCheck(): void {
    if (state.screen !== "mic_check") return;
    beginGuidedTask(false);
  }

  function continueMicCheck(): void {
    if (state.screen !== "mic_check") return;
    beginGuidedTask(false);
  }

  function saveAsWorkflow(): void {
    if (state.screen !== "guided_task" || state.guidedTask.demo || state.guidedTask.saved) return;
    const runSnap = runViewerInternal.getSnapshot();
    if (runSnap.runState !== "done") return;
    const name = "first-task";
    bus.publish("workflow.compiled", {
      name,
      version: "1.0.0",
      manifest_path: `workflows/${name}.ts`,
      dsl_path: `workflows/${name}.ts`,
      source_run_id: runSnap.runId ?? "",
    });
    state = { ...state, guidedTask: { ...state.guidedTask, saved: true }, screen: "schedule" };
    emit();
  }

  function continueAfterDemo(): void {
    if (state.screen !== "guided_task" || !state.guidedTask.demo) return;
    if (runViewerInternal.getSnapshot().runState !== "done") return;
    state = { ...state, screen: "setup_path" };
    emit();
  }

  function chooseSchedule(id: ScheduleOptionId): void {
    if (state.screen !== "schedule") return;
    state = { ...state, schedule: { selected: id } };
    emit();
  }

  function finishSchedule(): void {
    if (state.screen !== "schedule" || !state.schedule.selected) return;
    state = { ...state, complete: true };
    emit();
  }

  function dispose(): void {
    disposed = true;
    state.guidedTask.stop?.();
    abandonLocalDownload();
    unsubscribeRunViewer();
    runViewerInternal.dispose();
    listeners.clear();
  }

  return {
    getSnapshot: () => toSnapshot(state, runViewerInternal.getSnapshot(), diskNeededBytes),
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    continueWelcome,
    chooseChatGPT,
    chooseClaude,
    startLocalDownload,
    pauseLocalDownload,
    resumeLocalDownload,
    cancelLocalDownload,
    continueAfterLocalDownload,
    setAccessKeyText,
    chooseProviderManually,
    continueWithAccessKey,
    startDemo,
    playMicSample,
    skipMicCheck,
    continueMicCheck,
    saveAsWorkflow,
    continueAfterDemo,
    chooseSchedule,
    finishSchedule,
    dispose,
  };
}
