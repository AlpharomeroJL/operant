// @advanced
// Wire-protocol transport, not UI copy. This is the real BusClient that speaks
// the frozen shell-to-core IPC contract (contracts/ipc.md) over the Tauri
// command surface lane B2 exposes: forwarded core events arrive on the
// operant://bus Tauri event, and UI-originated commands go out through
// invoke("core_call", { cmd, args }). Like ui/src/bus/types.ts this file is
// marked @advanced only to exempt it from scripts/microcopy_lint.mjs: the
// command names ("pause", "undo_run", ...) and bus topics below are wire
// vocabulary, never rendered as UI text.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { matchesTopic, type BusClient } from "./mockClient.ts";
import type { BusEnvelope, BusEvent, BusTopic, BusTopicPayloadMap } from "./types.ts";

// The Tauri event channel the shell forwards every core `evt` frame onto
// (contracts/ipc.md section 2d). Its payload is that frame: the bus Envelope
// under `env`, plus the optional flight-recorder `thumb` beside it.
export const BUS_EVENT_CHANNEL = "operant://bus";

// The single Tauri command carrying request/response traffic: one core_call is
// one `req` frame, and it resolves with the matching `res` result
// (contracts/ipc.md sections 2b/2c).
export const CORE_CALL_COMMAND = "core_call";

/** One forwarded core event, exactly as it rides the operant://bus Tauri event. */
export interface BusEventFrame {
  env: BusEnvelope;
  thumb?: unknown;
}

export type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
export type ListenFn = (
  channel: string,
  handler: (event: { payload: BusEventFrame }) => void,
) => Promise<() => void>;

/**
 * The Tauri surface createRealClient drives, injectable so tests can feed
 * fixture events through a fake `listen` and capture core_call invocations with
 * no live webview. Defaults to the real @tauri-apps/api functions.
 */
export interface TauriBridge {
  invoke: InvokeFn;
  listen: ListenFn;
}

const defaultBridge: TauriBridge = {
  invoke: (cmd, args) => tauriInvoke(cmd, args ?? {}),
  listen: (channel, handler) =>
    tauriListen<BusEventFrame>(channel, (event) => handler({ payload: event.payload })),
};

// A UI publish that maps to a core command: the cmd name and the args object
// the core expects (contracts/ipc.md section 5). A null route means the topic
// is a core-owned event the UI has no business commanding, so publish no-ops
// it: the mock run simulators can never fake a run over a real core, and the
// core remains the only source for its own lifecycle events (it echoes each
// one back on operant://bus).
interface CommandRoute {
  cmd: string;
  args: Record<string, unknown>;
}

function routeCommand(topic: BusTopic, payload: Record<string, unknown>): CommandRoute | null {
  switch (topic) {
    // Run control (section 5b). The shell publishes the observable-outcome
    // topic when a person clicks Pause/Resume/Intervene/Stop or the panic
    // switch; over a real core that publish is the command that causes it.
    case "run.paused":
      return { cmd: "pause", args: { run_id: payload.run_id } };
    case "run.resumed":
      return { cmd: "resume", args: { run_id: payload.run_id } };
    case "run.redirected":
      return { cmd: "redirect", args: { instruction: payload.instruction } };
    case "run.halted":
      return { cmd: "stop", args: { run_id: payload.run_id } };
    case "killswitch.engaged":
      return { cmd: "kill", args: {} };
    // Undo (section 5c). Opening the preview requests it; confirming applies it.
    case "undo.previewed":
      return { cmd: "preview_undo", args: { run_id: payload.run_id } };
    case "undo.applied":
      return { cmd: "undo_run", args: { run_id: payload.run_id } };
    // Settings (section 5f). A changed setting is persisted through the core.
    case "config.changed":
      return { cmd: "set_settings", args: { key: payload.key, value: payload.value } };
    default:
      // Core-owned event topics: run.started, run.step.*, run.completed,
      // workflow.*, gate.*, perception.*, and the rest. The core is their only
      // true source and echoes them back, so publishing one is a no-op here.
      return null;
  }
}

/**
 * A BusClient over the real core. `subscribe` reuses the mock's exact
 * prefix-filter so real and mocked subscriptions deliver identically;
 * `publish` routes UI commands to invoke("core_call", ...) and drops
 * core-owned event topics; `close` detaches the operant://bus listener.
 */
export function createRealClient(bridge: TauriBridge = defaultBridge): BusClient {
  const listeners = new Set<{ prefix: string; fn: (event: BusEvent) => void }>();
  let unlisten: (() => void) | null = null;
  let closed = false;

  function dispatch(env: BusEnvelope): void {
    for (const { prefix, fn } of listeners) {
      if (matchesTopic(prefix, env.topic)) {
        // The wire envelope is the permissive BusEnvelope<string, unknown>; the
        // subscriber callback narrows on env.topic, the same as the mock's.
        fn(env as unknown as BusEvent);
      }
    }
  }

  // Drain the forwarded event stream immediately (contracts/ipc.md section 8a:
  // the shell reads stdout continuously). listen resolves with an unlisten fn;
  // stash it for close(). If close() ran before it resolved, detach at once.
  bridge
    .listen(BUS_EVENT_CHANNEL, (event) => {
      if (!closed && event?.payload?.env) dispatch(event.payload.env);
    })
    .then((fn) => {
      if (closed) {
        fn();
      } else {
        unlisten = fn;
      }
    })
    .catch(() => {
      // A listen that never attaches leaves the client eventless; the shell's
      // supervisor path surfaces a dead pipe, not this transport.
    });

  function subscribe(topicPrefix: string, listener: (event: BusEvent) => void): () => void {
    const entry = { prefix: topicPrefix, fn: listener };
    listeners.add(entry);
    return () => listeners.delete(entry);
  }

  function publish<T extends BusTopic>(topic: T, payload: BusTopicPayloadMap[T]): void {
    const route = routeCommand(topic, payload as unknown as Record<string, unknown>);
    if (!route) return;
    // Fire and forget: a command's observable outcome arrives as an evt on
    // operant://bus (contracts/ipc.md section 4), not through this return value.
    void bridge.invoke(CORE_CALL_COMMAND, { cmd: route.cmd, args: route.args });
  }

  function close(): void {
    closed = true;
    listeners.clear();
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  }

  return { subscribe, publish, close };
}
