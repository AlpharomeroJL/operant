import {
  RUN_MODE_EXPLORE,
  GROUNDING_UIA,
  type ActionIR,
  type BusEvent,
  type BusTopic,
  type BusTopicPayloadMap,
  type EvtSidecar,
  type RunControlCommand,
  type RunMode,
} from "./types.ts";

/**
 * A bus subscriber. The optional second argument is the IPC `evt` frame's
 * non-envelope sidecar (contracts/ipc.md section 2d), today just the
 * flight-recorder `thumb`. It is optional so every existing subscriber that
 * takes only the event keeps working unchanged; only consumers that draw the
 * thumbnail read it.
 */
type Listener = (event: BusEvent, sidecar?: EvtSidecar) => void;

export interface BusClient {
  /** Subscribe to an exact topic, a dot-prefix namespace ("run" matches "run.*"), or "*" for everything. */
  subscribe(topicPrefix: string, listener: Listener): () => void;
  /**
   * Publish a bus event. `sidecar` carries the non-envelope part of an IPC
   * `evt` frame (the flight-recorder `thumb`); it is delivered to subscribers
   * beside the envelope and never folded into the bus payload.
   */
  publish<T extends BusTopic>(topic: T, payload: BusTopicPayloadMap[T], sidecar?: EvtSidecar): void;
  /**
   * Send a run-control command to the core (contracts/ipc.md section 5b:
   * stop/pause/resume/redirect). The command is NOT a bus event; its observable
   * outcome is a `run.*` event the core echoes back. The mock plays the core by
   * echoing that event synchronously, so the run viewer renders the result the
   * same way it will against a live core.
   */
  command(command: RunControlCommand): void;
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

  function publish<T extends BusTopic>(topic: T, payload: BusTopicPayloadMap[T], sidecar?: EvtSidecar): void {
    const envelope = {
      v: 1 as const,
      seq: ++seq,
      ts: new Date().toISOString(),
      topic,
      payload,
    } as BusEvent;

    for (const { prefix, fn } of listeners) {
      if (prefix === "*" || topic === prefix || topic.startsWith(`${prefix}.`)) {
        fn(envelope, sidecar);
      }
    }
  }

  function subscribe(topicPrefix: string, listener: Listener): () => void {
    const entry = { prefix: topicPrefix, fn: listener };
    listeners.add(entry);
    return () => listeners.delete(entry);
  }

  function command(command: RunControlCommand): void {
    // Stand in for the core: a run-control command's observable effect is the
    // run.* event the real core would publish in response (contracts/ipc.md
    // section 5b). Echo it here so the round trip -- command out, event back,
    // handlers render -- is exercised end to end on the mock bus, exactly as it
    // will be against a live core (which echoes over the real transport).
    switch (command.cmd) {
      case "stop":
        // Stop ends the run; a human-driven stop closes it as halted.
        publish("run.halted", { run_id: command.run_id, reason: "human" });
        break;
      case "pause":
        publish("run.paused", { run_id: command.run_id, by: "human" });
        break;
      case "resume":
        publish("run.resumed", { run_id: command.run_id });
        break;
      case "redirect":
        // Redirect captures the correction and then resumes on its own
        // (control.rs): the core echoes run.redirected followed by run.resumed.
        publish("run.redirected", { run_id: command.run_id, instruction: command.instruction });
        publish("run.resumed", { run_id: command.run_id });
        break;
    }
  }

  function close(): void {
    listeners.clear();
  }

  return { subscribe, publish, command, close };
}

const DEFAULT_DEMO_GOAL = "Copy the invoice total into the spreadsheet";

/**
 * Canned demo steps as Action IR fragments (contracts/action_ir.schema.json
 * shape: kind plus target/params), not hand-written sentences. simulateDemoRun
 * hands each one to the real plain-English renderer (U4A, ui/src/runViewer/
 * sdkRender.ts) so the run viewer shows exactly what that renderer produces,
 * the same as it would for a live run. v and id are filled in at publish time.
 */
const DEMO_STEPS: ReadonlyArray<Pick<ActionIR, "kind" | "target" | "params">> = [
  {
    kind: "click",
    target: { selectors: [{ kind: "name_role_path", path: [{ role: "treeitem", name: "Downloads" }] }] },
  },
  {
    kind: "click",
    target: { selectors: [{ kind: "name_role_path", path: [{ role: "listitem", name: "Invoice.pdf" }] }] },
  },
  { kind: "key", params: { combo: "ctrl+c" } },
  { kind: "key", params: { combo: "ctrl+v" } },
];

export interface SimulateDemoRunOptions {
  /** The goal text carried on run.started; defaults to a canned sample goal. */
  goal?: string;
  /** Override the generated run id (tests want a stable id to assert against). */
  runId?: string;
  /**
   * run.step.proposed carries the Action IR and, per contracts/bus_events.md,
   * is only published while teaching, before the checkpoint; any other mode
   * goes straight to gated/executed, the same as a real run would. Defaults
   * to explore (teach).
   */
  mode?: RunMode;
  stepDelayMs?: number;
}

/**
 * Feeds a small canned run through a bus client on a timer so the run viewer
 * has something real to stream without a backend. Returns a stop function
 * that cancels any steps not yet published.
 */
export function simulateDemoRun(bus: BusClient, opts: SimulateDemoRunOptions = {}): () => void {
  const delay = opts.stepDelayMs ?? 500;
  const mode = opts.mode ?? RUN_MODE_EXPLORE;
  const runId = opts.runId ?? `demo-${Date.now()}`;
  const goal = opts.goal ?? DEFAULT_DEMO_GOAL;
  let cancelled = false;
  const timers: ReturnType<typeof setTimeout>[] = [];

  function schedule(fn: () => void, at: number): void {
    timers.push(
      setTimeout(() => {
        if (!cancelled) fn();
      }, at),
    );
  }

  bus.publish("run.started", { run_id: runId, goal, mode });

  DEMO_STEPS.forEach((step, i) => {
    const stepId = `s${i + 1}`;
    schedule(() => {
      if (mode === RUN_MODE_EXPLORE) {
        bus.publish("run.step.proposed", {
          run_id: runId,
          step: { v: 1, id: stepId, ...step },
        });
      }
      bus.publish("run.step.gated", {
        run_id: runId,
        step_id: stepId,
        gate_kind: "pre",
        result: "pass",
      });
      bus.publish("run.step.executed", {
        run_id: runId,
        step_id: stepId,
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
