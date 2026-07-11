import { RUN_MODE_EXPLORE, GROUNDING_UIA, type BusEvent, type BusTopic, type BusTopicPayloadMap } from "./types.ts";

type Listener = (event: BusEvent) => void;

export interface BusClient {
  /** Subscribe to an exact topic, a dot-prefix namespace ("run" matches "run.*"), or "*" for everything. */
  subscribe(topicPrefix: string, listener: Listener): () => void;
  publish<T extends BusTopic>(topic: T, payload: BusTopicPayloadMap[T]): void;
  close(): void;
}

/**
 * A mocked bus client standing in for the real C1 event bus
 * (contracts/bus_events.md) so the shell renders end to end with no backend
 * process running. The envelope shape (v, seq, ts, topic, payload) matches
 * the contract exactly. Swap for a real transport (a Tauri IPC bridge onto
 * the Rust core's bus) later; BusClient is the seam other lanes wire against.
 */
export function createMockBusClient(): BusClient {
  let seq = 0;
  const listeners = new Set<{ prefix: string; fn: Listener }>();

  function publish<T extends BusTopic>(topic: T, payload: BusTopicPayloadMap[T]): void {
    const envelope = {
      v: 1 as const,
      seq: ++seq,
      ts: new Date().toISOString(),
      topic,
      payload,
    } as BusEvent;

    for (const { prefix, fn } of listeners) {
      if (prefix === "*" || topic === prefix || topic.startsWith(`${prefix}.`)) {
        fn(envelope);
      }
    }
  }

  function subscribe(topicPrefix: string, listener: Listener): () => void {
    const entry = { prefix: topicPrefix, fn: listener };
    listeners.add(entry);
    return () => listeners.delete(entry);
  }

  function close(): void {
    listeners.clear();
  }

  return { subscribe, publish, close };
}

const DEMO_STEPS: ReadonlyArray<{ id: string; sentence: string }> = [
  { id: "s1", sentence: "Open Downloads folder" },
  { id: "s2", sentence: "Find the newest invoice PDF" },
  { id: "s3", sentence: "Copy the total to the clipboard" },
  { id: "s4", sentence: "Paste the total into the spreadsheet" },
];

/** Sentence lookup for the canned demo steps; the real plain-English renderer (C19) replaces this later. */
export const DEMO_STEP_SENTENCES: Readonly<Record<string, string>> = Object.fromEntries(
  DEMO_STEPS.map((s) => [s.id, s.sentence]),
);

/**
 * Feeds a small canned run through a bus client on a timer so the run viewer
 * has something real to stream without a backend. Returns a stop function
 * that cancels any steps not yet published.
 */
export function simulateDemoRun(bus: BusClient, opts: { stepDelayMs?: number } = {}): () => void {
  const delay = opts.stepDelayMs ?? 500;
  const runId = `demo-${Date.now()}`;
  let cancelled = false;
  const timers: ReturnType<typeof setTimeout>[] = [];

  function schedule(fn: () => void, at: number): void {
    timers.push(
      setTimeout(() => {
        if (!cancelled) fn();
      }, at),
    );
  }

  bus.publish("run.started", {
    run_id: runId,
    goal: "Copy the invoice total into the spreadsheet",
    mode: RUN_MODE_EXPLORE,
  });

  DEMO_STEPS.forEach((step, i) => {
    schedule(() => {
      bus.publish("run.step.proposed", {
        run_id: runId,
        step: { v: 1, id: step.id, kind: "click" },
      });
      bus.publish("run.step.gated", {
        run_id: runId,
        step_id: step.id,
        gate_kind: "pre",
        result: "pass",
      });
      bus.publish("run.step.executed", {
        run_id: runId,
        step_id: step.id,
        outcome: "ok",
        ms: 120 + i * 10,
        grounding: GROUNDING_UIA,
      });
    }, (i + 1) * delay);
  });

  schedule(() => {
    bus.publish("run.completed", {
      run_id: runId,
      outcome: "ok",
      steps: DEMO_STEPS.length,
      wall_ms: DEMO_STEPS.length * delay,
    });
  }, (DEMO_STEPS.length + 1) * delay);

  return () => {
    cancelled = true;
    for (const t of timers) clearTimeout(t);
  };
}
