// The kill switch's two-path panic (SAFETY, never-cut). docs/ARCHITECTURE.md's
// C20 guardian set, docs/PRD.md FR-S5, docs/specs/guardian.md. Pure and
// DOM-light (the only DOM touch, releaseHeldModifiers, is guarded so this runs
// under plain `node --test`), the same state-vs-view split as
// ui/src/tray/state.ts.
//
// A5 (docs/specs/ipc-bridge.md section 8b) found the panic control
// "renders but does not stop": ui/src/tray/state.ts's panic() self-published
// killswitch.engaged, painting the tray red without ever telling the core to
// halt. This inverts that: the panic path invokes the two independent stop
// COMMANDS the contract defines (contracts/ipc.md section 5b), and the core
// echoes killswitch.engaged back so the existing tray/runViewer handlers paint
// the halted state off a real stop, not a cosmetic self-publish.
//
// The two paths are deliberately redundant (contracts/ipc.md section 5b's note
// on `kill`):
//   1. `stop`  cooperative freeze the loop checks between steps, then closes
//              the run (ends with run.completed/run.halted).
//   2. `kill`  the unblockable backstop: path 1 sets the in-process freeze
//              (operant_core::safety::set_frozen, blocking WindowsSynthesizer
//              before the next SendInput) AND path 2 hard-terminates the child
//              (ui/src-tauri's core_kill, lane B2), so a loop that ignores the
//              cooperative stop is still killed within budget.
// Either path alone stops input synthesis; both run because a wedged core can
// swallow the cooperative one and only the hard terminate is guaranteed.

import type { BusClient } from "../bus/mockClient.ts";

/**
 * The command seam the panic path drives (contracts/ipc.md section 5b). The
 * real transport (lane B3's ui/src-tauri client) implements this by routing
 * `stop` -> the `stop_run` command and `kill` -> `engage_killswitch` + B2's
 * `core_kill` hard terminate; createBusPanicClient below is the mock/dev
 * implementation that echoes the core's events straight onto the bus so the
 * shell renders end to end with no backend process. Both methods are
 * fire-and-forget: a real `kill` may cut the pipe before its `res` is written
 * (contracts/ipc.md section 5b), so an implementation MUST dispatch without
 * blocking and MUST NOT make the caller await a result that may never arrive.
 */
export interface PanicClient {
  /** contracts/ipc.md `stop`: cooperative freeze plus close the run. Best-effort; the backstop below is the guarantee. */
  stop(runId?: string): void;
  /** contracts/ipc.md `kill`: the panic path. Path 1 in-process freeze AND path 2 hard-terminate; echoes killswitch.engaged. */
  kill(): void;
}

export interface EnginePanicOptions {
  /** The run to close cooperatively, when one is known (the tray/runViewer track the latest). Omitted from a global-chord trigger that has no run in hand; the backstop stops the core regardless. */
  runId?: string;
  /**
   * Releases any modifier keys the trigger left held. The panic chord is
   * modifier-heavy (default Ctrl+Alt+Shift+Space, ui/src/settings/chord.ts), so
   * without this a stuck Ctrl/Alt/Shift in the webview's own key tracking could
   * ride along and misread the next click. The OS-level held-INPUT release (a
   * synthesizer that froze mid-combo) is part of `kill`'s core contract
   * (docs/specs/guardian.md: "release all held modifiers"), out of ui/src'
   * reach; this only resets the webview side. Defaults to releaseHeldModifiers.
   */
  releaseModifiers?: () => void;
}

/**
 * Dispatches synthetic keyup events for every modifier so nothing in the
 * webview keeps treating a chord modifier as held after the panic. Guarded for
 * a non-DOM host (plain `node --test`, where this is a no-op).
 */
export function releaseHeldModifiers(): void {
  if (typeof document === "undefined" || typeof KeyboardEvent === "undefined") return;
  for (const key of ["Control", "Alt", "Shift", "Meta"]) {
    document.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  }
}

/**
 * The panic sequence. Never-cut: nothing a cooperative step does (or throws)
 * can stop the backstop `kill` from firing. Release held modifiers, ask the
 * loop to stop cooperatively, then fire the unblockable hard terminate last so
 * it is the guaranteed final word even if the run ignores the cooperative stop.
 */
export function enginePanic(client: PanicClient, opts: EnginePanicOptions = {}): void {
  const release = opts.releaseModifiers ?? releaseHeldModifiers;
  try {
    release();
  } catch {
    // A modifier-reset hiccup must never delay the stop paths below.
  }
  try {
    client.stop(opts.runId);
  } catch {
    // The cooperative path is best-effort; the backstop below is the guarantee,
    // so a throw here (a wedged or half-wired stop) must not skip the kill.
  }
  // The backstop, always reached, always last. Not wrapped: a synchronous throw
  // here is a real wiring bug worth surfacing loudly, not swallowing into a
  // panic that silently did nothing.
  client.kill();
}

/**
 * The mock/dev PanicClient: it stands in for the core by publishing the exact
 * events a real core would echo, so ui/src/tray/state.ts and
 * ui/src/runViewer/state.ts paint the halted state off the bus with no backend
 * running. `stop` closes the tracked run (the same run.halted, reason human,
 * ui/src/runViewer/state.ts's own stop() publishes); `kill` echoes
 * killswitch.engaged, the contract's `kill` echo. Swap for the real transport
 * client without touching a single call site.
 */
export function createBusPanicClient(bus: BusClient, now: () => number = () => Date.now()): PanicClient {
  return {
    stop(runId?: string): void {
      // No run in hand means nothing to close cooperatively here. The freeze
      // `stop` sets in a real core is an in-process atomic with no bus echo of
      // its own (contracts/ipc.md section 8a), so there is nothing to publish.
      if (!runId) return;
      bus.publish("run.halted", { run_id: runId, reason: "human" });
    },
    kill(): void {
      bus.publish("killswitch.engaged", { at_ms: now() });
    },
  };
}
