// The workflow library (docs/specs/ui.md: "workflow library (cards: name,
// plain summary, last run, minutes saved badge, run/schedule/explain
// buttons)"). Turns the mock registry (./mockRegistry.ts) plus workflow.*
// and run.* bus events (contracts/bus_events.md) into the cards the screen
// shows. Pure and DOM-free, same split as ui/src/runViewer/state.ts.

import type { BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY, type BusEvent } from "../bus/types.ts";
import { renderWorkflow, type RenderedWorkflow } from "../../../sdk/ts/src/render/index.js";
import { createMockRegistry, type MockRegistry, type MockWorkflowRecord } from "./mockRegistry.ts";
import { workflowLibraryStrings } from "../strings/default.ts";
import { libraryStrings } from "./strings.ts";

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
}

export interface LibrarySnapshot {
  title: string;
  cards: LibraryCard[];
  empty: boolean;
  emptyLabel: string;
}

interface WorkflowRuntime {
  lastRunAt: number | null;
  exploreMs: number | null;
  replayMsTotal: number;
  replayCount: number;
}

interface PendingRun {
  workflowName: string;
  mode: string;
  startedAt: number;
}

export interface Library {
  getSnapshot(): LibrarySnapshot;
  subscribe(fn: (snap: LibrarySnapshot) => void): () => void;
  /**
   * Starts a saved workflow directly (the palette teach flow is a
   * different path entirely; this always uses the saved-workflow run
   * mode). No-op for an unknown name.
   */
  run(name: string): void;
  /** No bus topic models "create a recurring trigger" yet (contracts/bus_events.md's scheduler topics all assume one already exists); this only reports the intent via onScheduleRequested. */
  schedule(name: string): void;
  explain(name: string): RenderedWorkflow | undefined;
  dispose(): void;
}

export interface CreateLibraryOptions {
  registry?: MockRegistry;
  now?: () => number;
  onScheduleRequested?: (name: string, title: string) => void;
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
  const runtimes = new Map<string, WorkflowRuntime>();
  const pendingRuns = new Map<string, PendingRun>();
  const listeners = new Set<(snap: LibrarySnapshot) => void>();

  function runtimeOf(name: string): WorkflowRuntime {
    let r = runtimes.get(name);
    if (!r) {
      r = { lastRunAt: null, exploreMs: null, replayMsTotal: 0, replayCount: 0 };
      runtimes.set(name, r);
    }
    return r;
  }

  function cardFor(record: MockWorkflowRecord): LibraryCard {
    const runtime = runtimes.get(record.manifest.name);
    const lastRunLabel = runtime?.lastRunAt
      ? workflowLibraryStrings.lastRun(formatWhen(runtime.lastRunAt, now()))
      : libraryStrings.neverRun;
    const minutesSaved = minutesSavedFor(runtime);
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
    };
  }

  function snapshot(): LibrarySnapshot {
    const cards = registry.list().map(cardFor);
    return {
      title: workflowLibraryStrings.title,
      cards,
      empty: cards.length === 0,
      emptyLabel: workflowLibraryStrings.empty,
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
    const runId = `library-${name}-${now()}`;
    bus.publish("run.started", { run_id: runId, goal: record.manifest.description, mode: RUN_MODE_REPLAY, workflow_name: name });
    // This shell has no backend to actually replay the saved workflow
    // (ui/src/bus/mockClient.ts's own canned demo is the palette's, not
    // the library's); complete right away so the library's lastRun and
    // minutes-saved figures update the same way a real replay's
    // run.completed would. The run viewer still shows the start/finish
    // like any other run on the bus.
    bus.publish("run.completed", { run_id: runId, outcome: "ok", steps: record.steps.length, wall_ms: 400 });
  }

  function schedule(name: string): void {
    const record = registry.get(name);
    if (!record) return;
    opts.onScheduleRequested?.(name, record.manifest.description || name);
  }

  function explain(name: string): RenderedWorkflow | undefined {
    const record = registry.get(name);
    if (!record) return undefined;
    // renderWorkflow's steps parameter is a mutable array; record.steps is
    // deliberately readonly (mockRegistry.ts), so pass a shallow copy rather
    // than loosen the stored type.
    return renderWorkflow(record.manifest, [...record.steps]);
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    run,
    schedule,
    explain,
    dispose() {
      unsubscribeBus();
      unsubscribeRegistry();
      listeners.clear();
    },
  };
}
