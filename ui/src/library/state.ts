// The workflow library (docs/specs/ui.md: "workflow library (cards: name,
// plain summary, last run, minutes saved badge, run/schedule/explain
// buttons)"). Turns the mock registry (./mockRegistry.ts) plus workflow.*
// and run.* bus events (contracts/bus_events.md) into the cards the screen
// shows. Pure and DOM-free, same split as ui/src/runViewer/state.ts.

import type { BusClient } from "../bus/mockClient.ts";
import type { CommandClient } from "../bus/commandClient.ts";
import { RUN_MODE_REPLAY, type BusEvent } from "../bus/types.ts";
import { renderWorkflow, type RenderedWorkflow } from "../../../sdk/ts/src/render/index.js";
import { createMockRegistry, type MockRegistry, type MockWorkflowRecord } from "./mockRegistry.ts";
import { createUnavailableSchedulerCommands, isNotImplemented, TRIGGER_KIND_CRON, type SchedulerCommands } from "../scheduler/commands.ts";
import { commonStrings, workflowLibraryStrings } from "../strings/default.ts";
import { libraryStrings } from "./strings.ts";
import { assignGlyph } from "./glyph.ts";

export type LastRunStatus = "pending" | "ok" | "failed";

export interface LibraryCard {
  name: string;
  title: string;
  summary: string;
  lastRunLabel: string;
  minutesSaved: number;
  minutesSavedLabel: string;
  runLabel: string;
  scheduleLabel: string;
  explainLabel: string;
  /** design.md section 3, Library: "auto-assigned duotone glyph and hue." The letter half of that glyph (see ./glyph.ts); the hue half is glyphHueRotationDeg. */
  glyphLetter: string;
  /** Degrees to rotate the shared accent color's hue by for this card's glyph (./glyph.ts, ./view.ts's CSS filter); reuses the one signal color tokens.ts defines rather than a second, independent color set. */
  glyphHueRotationDeg: number;
  /** design.md section 3, Library: "a last-run dot." "pending" (never run; reuses the existing idle status color) until the first run.completed. */
  lastRunStatus: LastRunStatus;
}

export interface LibrarySnapshot {
  title: string;
  cards: LibraryCard[];
  empty: boolean;
  emptyLabel: string;
  /**
   * H1 (docs/specs/design.md section 4's copy rule, "Empty states invite one
   * specific action"). Set only when the library truly has zero saved
   * workflows -- null while `empty` is true only because a search matched
   * nothing (workflowLibraryStrings.noMatches), since "teach a workflow" is
   * not the right invite when the person already has some and just typed a
   * search that missed. ./view.ts renders a button only when this is
   * non-null.
   */
  emptyActionLabel: string | null;
  /** design.md section 3, Library: "Search filters live." The text currently typed into the search field; round-tripped back into the input's value so ui/src/styles/focusPreserve.ts's rebuild-carries-focus mechanism keeps it in place across a re-render. */
  searchQuery: string;
  searchLabel: string;
  searchPlaceholder: string;
}

interface WorkflowRuntime {
  lastRunAt: number | null;
  /** null until the first run.completed for this workflow; independent of lastRunAt only in that both start null together and are always set together, kept as two fields because they mean different things (a timestamp vs an outcome), same as ui/src/dashboard/state.ts's RecentRunRecord. */
  lastRunOutcome: "ok" | "failed" | null;
  exploreMs: number | null;
  replayMsTotal: number;
  replayCount: number;
}

interface PendingRun {
  workflowName: string;
  mode: string;
  startedAt: number;
}

/**
 * What happened when a person pressed Schedule on a card. `scheduled` is true
 * only when the core actually created a recurring trigger; `unavailable` is
 * true when the core has no trigger store wired yet and answered the
 * upsert_trigger command with `not_implemented` (contracts/ipc.md section 5g).
 * In every build today `unavailable` is the outcome: the two are kept as
 * separate booleans (rather than one "ok") so the shell can tell an honest
 * "scheduling is not available yet" apart from a real, retryable failure, and
 * so the success path is ready to render truthfully the day the store lands.
 */
export interface ScheduleOutcome {
  name: string;
  title: string;
  scheduled: boolean;
  unavailable: boolean;
}

export interface Library {
  getSnapshot(): LibrarySnapshot;
  subscribe(fn: (snap: LibrarySnapshot) => void): () => void;
  /**
   * Resolves once the initial load has settled: the real list_workflows load
   * (contracts/ipc.md section 5c) when a CommandClient is present, or
   * immediately in dev/Demo where the seeded ./mockRegistry.ts is the source.
   * Lets callers (and tests) await the first real snapshot deterministically
   * rather than racing the async command.
   */
  ready: Promise<void>;
  /**
   * Starts a saved workflow directly (the palette teach flow is a
   * different path entirely; this always uses the saved-workflow run
   * mode). No-op for an unknown name. With a CommandClient present this issues
   * the start_replay command (contracts/ipc.md section 5b, "run_saved_workflow")
   * and lets the core's echoed run.* events update last-run and minutes-saved;
   * in dev/Demo it synthesizes those same events on the bus with no backend.
   */
  run(name: string): void;
  /**
   * Asks the core to create a recurring trigger for this workflow via the
   * upsert_trigger command (contracts/ipc.md section 5e). Resolves with the
   * honest outcome and also reports it through onScheduleResolved. The core has
   * no trigger store yet, so this resolves `unavailable` in every build today;
   * it never fabricates a schedule or touches the bus. Resolves undefined for
   * an unknown name.
   */
  schedule(name: string): Promise<ScheduleOutcome | undefined>;
  /**
   * The plain-English Explain view for a saved workflow. With a CommandClient
   * present this issues explain_workflow (contracts/ipc.md section 5c), the
   * core running the very same `@operant/sdk/render` used locally below; in
   * dev/Demo it renders locally from the seeded manifest and steps. Async
   * because the real path is a command round-trip. Resolves undefined for an
   * unknown name.
   */
  explain(name: string): Promise<RenderedWorkflow | undefined>;
  /**
   * design.md section 3, Library: "Drag to reorder." Moves `name` to just
   * before `beforeName` in display order, or to the end when beforeName is
   * null. No-op when name is not a known card; falls back to the end when
   * beforeName no longer is (the card it was dropped relative to was
   * removed in the meantime). Order is this screen's own display state, not
   * ./mockRegistry.ts's (a real registry has no notion of a person's
   * preferred card order), so it survives a workflow.installed/compiled
   * update but not a fresh createLibrary call.
   */
  reorder(name: string, beforeName: string | null): void;
  /** design.md section 3, Library: "Search filters live." Empty string clears the filter. */
  setSearchQuery(query: string): void;
  dispose(): void;
}

export interface CreateLibraryOptions {
  registry?: MockRegistry;
  now?: () => number;
  /**
   * The scheduler command surface (contracts/ipc.md section 5e). Defaults to
   * the honest not-yet-wired implementation, which answers every scheduler
   * command with `not_implemented` because the core has no trigger store yet
   * (see ../scheduler/commands.ts).
   */
  scheduler?: SchedulerCommands;
  /** Reports the outcome of a Schedule press once the upsert_trigger command resolves. */
  onScheduleResolved?: (outcome: ScheduleOutcome) => void;
  /**
   * The request/response bridge (contracts/ipc.md). When present, the library
   * loads its real saved workflows via list_workflows and routes Run/Explain
   * through start_replay/explain_workflow. Omitted in dev/Demo (and the current
   * webview until the real transport lands), where the library falls back to the
   * seeded ./mockRegistry.ts and synthesizes runs on the bus itself. Scheduling
   * goes through the separate `scheduler` seam above, not this client.
   */
  client?: CommandClient;
}

function minutesSavedFor(runtime: WorkflowRuntime | undefined): number {
  if (!runtime || runtime.exploreMs === null || runtime.replayCount === 0) return 0;
  const avgReplayMs = runtime.replayMsTotal / runtime.replayCount;
  const savedMsPerRun = Math.max(0, runtime.exploreMs - avgReplayMs);
  const minutes = (savedMsPerRun * runtime.replayCount) / 60000;
  return Math.round(minutes * 10) / 10;
}

function formatWhen(atMs: number, nowMs: number): string {
  const diffMs = Math.max(0, nowMs - atMs);
  const minutes = Math.round(diffMs / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.round(hours / 24);
  return `${days} day${days === 1 ? "" : "s"} ago`;
}

export function createLibrary(bus: BusClient, opts: CreateLibraryOptions = {}): Library {
  const registry = opts.registry ?? createMockRegistry();
  const now = opts.now ?? (() => Date.now());
  const client = opts.client;
  const scheduler = opts.scheduler ?? createUnavailableSchedulerCommands();
  const runtimes = new Map<string, WorkflowRuntime>();
  const pendingRuns = new Map<string, PendingRun>();
  const listeners = new Set<(snap: LibrarySnapshot) => void>();

  function runtimeOf(name: string): WorkflowRuntime {
    let r = runtimes.get(name);
    if (!r) {
      r = { lastRunAt: null, lastRunOutcome: null, exploreMs: null, replayMsTotal: 0, replayCount: 0 };
      runtimes.set(name, r);
    }
    return r;
  }

  // Display order for the card grid (design.md section 3, Library: "Drag to
  // reorder"). Deliberately separate from registry.list()'s own insertion
  // order: a person's preferred layout is this screen's display concern,
  // not something a real workflow registry would have any notion of
  // (./mockRegistry.ts's own header comment: it mirrors "manifests stored
  // locally," not a person's card arrangement). reconcileOrder keeps this
  // in sync whenever the registry changes: newly seen names are appended at
  // the end, names no longer in the registry are dropped.
  let order: string[] = registry.list().map((r) => r.manifest.name);
  let searchQuery = "";

  function reconcileOrder(): void {
    order = order.filter((name) => registry.get(name) !== undefined);
    for (const record of registry.list()) {
      if (!order.includes(record.manifest.name)) order.push(record.manifest.name);
    }
  }

  function matchesQuery(card: LibraryCard, query: string): boolean {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return card.title.toLowerCase().includes(q) || card.name.toLowerCase().includes(q);
  }

  function cardFor(record: MockWorkflowRecord): LibraryCard {
    const runtime = runtimes.get(record.manifest.name);
    const lastRunLabel = runtime?.lastRunAt
      ? workflowLibraryStrings.lastRun(formatWhen(runtime.lastRunAt, now()))
      : libraryStrings.neverRun;
    const minutesSaved = minutesSavedFor(runtime);
    const glyph = assignGlyph(record.manifest.name, record.manifest.description);
    const lastRunStatus: LastRunStatus = runtime?.lastRunOutcome ?? "pending";
    return {
      name: record.manifest.name,
      title: record.manifest.description || record.manifest.name,
      summary: record.manifest.description,
      lastRunLabel,
      minutesSaved,
      minutesSavedLabel: workflowLibraryStrings.minutesSaved(minutesSaved),
      runLabel: workflowLibraryStrings.run,
      scheduleLabel: workflowLibraryStrings.schedule,
      explainLabel: workflowLibraryStrings.explain,
      glyphLetter: glyph.letter,
      glyphHueRotationDeg: glyph.hueRotationDeg,
      lastRunStatus,
    };
  }

  function snapshot(): LibrarySnapshot {
    reconcileOrder();
    const allCards = order.map((name) => registry.get(name)).filter((r): r is MockWorkflowRecord => r !== undefined).map(cardFor);
    const query = searchQuery.trim();
    const cards = query ? allCards.filter((c) => matchesQuery(c, query)) : allCards;
    // Typing a search that matches nothing must never read as "you have no
    // workflows" (workflowLibraryStrings.empty): only say that when the
    // library truly has none, regardless of the search box.
    const noSearchMatches = query.length > 0 && cards.length === 0 && allCards.length > 0;
    return {
      title: workflowLibraryStrings.title,
      cards,
      empty: cards.length === 0,
      emptyLabel: noSearchMatches ? workflowLibraryStrings.noMatches : workflowLibraryStrings.empty,
      emptyActionLabel: allCards.length === 0 ? commonStrings.teachFirstWorkflow : null,
      searchQuery,
      searchLabel: workflowLibraryStrings.searchLabel,
      searchPlaceholder: workflowLibraryStrings.searchPlaceholder,
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function handleBus(event: BusEvent): void {
    switch (event.topic) {
      case "workflow.installed": {
        registry.upsert(event.payload.name, {
          publisher: event.payload.publisher,
          signed: event.payload.signed,
          dryRunOnly: event.payload.dry_run_only,
        });
        return;
      }
      case "workflow.compiled": {
        registry.upsert(event.payload.name, {});
        return;
      }
      case "run.started": {
        if (!event.payload.workflow_name) return;
        pendingRuns.set(event.payload.run_id, {
          workflowName: event.payload.workflow_name,
          mode: event.payload.mode,
          startedAt: now(),
        });
        return;
      }
      case "run.completed": {
        const pending = pendingRuns.get(event.payload.run_id);
        if (!pending) return;
        pendingRuns.delete(event.payload.run_id);
        const runtime = runtimeOf(pending.workflowName);
        runtime.lastRunAt = now();
        // design.md section 3, Library: "a last-run dot." Set unconditionally
        // (explore and replay both count as "the last run"), same as
        // lastRunAt just above.
        runtime.lastRunOutcome = event.payload.outcome === "ok" ? "ok" : "failed";
        if (pending.mode === RUN_MODE_REPLAY) {
          runtime.replayMsTotal += event.payload.wall_ms;
          runtime.replayCount += 1;
        } else {
          runtime.exploreMs = event.payload.wall_ms;
        }
        emit();
        return;
      }
      default:
        return;
    }
  }

  // workflow.installed/compiled reach a snapshot update through
  // registry.upsert's own notify -> the registry.subscribe below; run.*
  // events emit directly since they only touch the runtimes map, which the
  // registry never observes.
  const unsubscribeBus = bus.subscribe("*", handleBus);
  const unsubscribeRegistry = registry.subscribe(() => emit());

  function run(name: string): void {
    const record = registry.get(name);
    if (!record) return;
    if (client) {
      // Real bridge: start_replay (contracts/ipc.md section 5b, the command
      // docs/specs/ipc-bridge.md section 2 names run_saved_workflow). The core
      // wraps the deterministic Replayer in synthetic run.started (mode replay,
      // workflow_name) / run.step.* / run.completed events and echoes them back
      // on the bus (section 3b); handleBus above already consumes those to move
      // last-run and minutes-saved. We synthesize nothing here, precisely so
      // the figures reflect a real replay and not a fabricated one.
      const path = record.path ?? record.manifest.dsl.path;
      void client.request("start_replay", { path });
      return;
    }
    // dev/Demo: this shell has no backend to actually replay the saved workflow
    // (ui/src/bus/mockClient.ts's own canned demo is the palette's, not the
    // library's); complete right away so the library's lastRun and minutes-saved
    // figures update the same way a real replay's run.completed would. The run
    // viewer still shows the start/finish like any other run on the bus.
    const runId = `library-${name}-${now()}`;
    bus.publish("run.started", { run_id: runId, goal: record.manifest.description, mode: RUN_MODE_REPLAY, workflow_name: name });
    bus.publish("run.completed", { run_id: runId, outcome: "ok", steps: record.steps.length, wall_ms: 400 });
  }

  async function schedule(name: string): Promise<ScheduleOutcome | undefined> {
    const record = registry.get(name);
    if (!record) return undefined;
    const title = record.manifest.description || name;
    // Wire the Schedule press to the real upsert_trigger command
    // (contracts/ipc.md section 5e). There is no schedule-authoring UI yet to
    // collect a real cron/file/window/email spec, so the request carries an
    // empty spec: the core answers `not_implemented` before it ever inspects
    // args (it has no trigger store, section 5g), so no fabricated spec is
    // stored anywhere. Building the spec-authoring surface is follow-up work
    // tracked in docs/roadmap/scheduler-live.md.
    const res = await scheduler.upsertTrigger({
      kind: TRIGGER_KIND_CRON,
      workflow_name: name,
      spec: "",
      enabled: true,
    });
    const outcome: ScheduleOutcome = {
      name,
      title,
      scheduled: res.ok,
      unavailable: isNotImplemented(res),
    };
    opts.onScheduleResolved?.(outcome);
    return outcome;
  }

  async function explain(name: string): Promise<RenderedWorkflow | undefined> {
    const record = registry.get(name);
    if (!record) return undefined;
    if (client) {
      // Real bridge: explain_workflow (contracts/ipc.md section 5c) runs the
      // exact same `@operant/sdk/render` renderer this file uses locally, only
      // in the core. Its result is {title, summary, grant, inputs, steps}; fill
      // `name` from the record so the resolved shape matches the local render.
      const path = record.path ?? record.manifest.dsl.path;
      const res = await client.request<Omit<RenderedWorkflow, "name">>("explain_workflow", { path });
      if (!res.ok) return undefined;
      return { name: record.manifest.name, ...res.result };
    }
    // dev/Demo: render locally. renderWorkflow's steps parameter is a mutable
    // array; record.steps is deliberately readonly (mockRegistry.ts), so pass a
    // shallow copy rather than loosen the stored type.
    return renderWorkflow(record.manifest, [...record.steps]);
  }

  function reorder(name: string, beforeName: string | null): void {
    if (name === beforeName) return;
    reconcileOrder();
    const idx = order.indexOf(name);
    if (idx === -1) return;
    order.splice(idx, 1);
    const beforeIdx = beforeName === null ? -1 : order.indexOf(beforeName);
    if (beforeIdx === -1) {
      order.push(name);
    } else {
      order.splice(beforeIdx, 0, name);
    }
    emit();
  }

  function setSearchQuery(query: string): void {
    if (query === searchQuery) return;
    searchQuery = query;
    emit();
  }

  // At mount, with a client present, load the real saved-workflow list
  // (contracts/ipc.md section 5c: list_workflows) and replace the seeded demo
  // records with it, so every card shows a real manifest (name, plain summary,
  // steps, capabilities). registry.replaceAll notifies the subscription wired
  // above, which emits the fresh snapshot; reconcileOrder then folds the real
  // names into the display order. A failed list leaves whatever the registry
  // already holds rather than clearing to a false "no workflows yet".
  async function loadWorkflows(c: CommandClient): Promise<void> {
    const res = await c.request<MockWorkflowRecord[]>("list_workflows");
    if (!res.ok) return;
    registry.replaceAll(res.result);
  }

  const ready: Promise<void> = client ? loadWorkflows(client) : Promise.resolve();

  return {
    ready,
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    run,
    schedule,
    explain,
    reorder,
    setSearchQuery,
    dispose() {
      unsubscribeBus();
      unsubscribeRegistry();
      listeners.clear();
    },
  };
}
