// The request/response half of the shell-to-core bridge (contracts/ipc.md
// section 2b "req" / 2c "res"), the counterpart to ./mockClient.ts's BusClient
// (which carries the one-way "evt" stream, section 2d). One req, exactly one
// res, correlated by id; the res is either a success ({ok:true, result}) or a
// typed refusal ({ok:false, error}). This is a separate seam from BusClient on
// purpose: events and commands are two frame families in the contract, and
// keeping them apart lets a screen subscribe to events without also owning a
// command channel, and vice versa.
//
// ui/src/library/state.ts is the first screen wired against it: with a client
// present it loads its real saved workflows via list_workflows and routes
// Run/Explain/Schedule through start_replay/explain_workflow/upsert_trigger.
// Absent (dev/Demo, and the current webview until the real transport lands) it
// falls back to ui/src/library/mockRegistry.ts. Swap for a real Tauri
// invoke-backed client later; CommandClient is the seam, exactly as BusClient
// is for events (docs/specs/ipc-bridge.md section 1: publish routes
// command-topics to invoke; the value-returning commands need this res channel).

/**
 * The res-frame error shape (contracts/ipc.md section 2c). `code` is a stable
 * snake_case string from that section's catalog (for example `not_implemented`,
 * `refused`, `not_found`); `message` is a plain-language sentence safe to
 * surface to a person; `retryable` says whether the same request may later
 * succeed unchanged.
 */
export interface CommandError {
  code: string;
  message: string;
  retryable: boolean;
}

/** One res frame (contracts/ipc.md section 2c): a success or a typed refusal. */
export type CommandResult<T> =
  | { ok: true; result: T }
  | { ok: false; error: CommandError };

export interface CommandClient {
  /**
   * Send one req (contracts/ipc.md section 2b) and resolve with its single
   * correlated res. A command-level "no" (including `not_implemented` for a
   * contract command not wired in this build) arrives as a resolved
   * `{ok:false}` result, NOT a thrown error; the promise rejects only on a
   * transport fault (a dead pipe, a malformed frame). Callers therefore branch
   * on `res.ok` and never need a try/catch for an expected refusal.
   */
  request<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<CommandResult<T>>;
}

/**
 * contracts/ipc.md section 2c/5g: a command reserved in the frozen contract but
 * not yet wired in this build answers with this error code. The library
 * surfaces it honestly (scheduling is not available yet) rather than faking a
 * success, per the contract's rule that a NOT-YET-IMPLEMENTED command MUST
 * answer `not_implemented`.
 */
export const ERROR_NOT_IMPLEMENTED = "not_implemented";
