// @advanced
// Not Advanced-mode UI copy: this file is a typed mirror of the shell-to-core
// request/response command contract in contracts/ipc.md (section 5e, "Scheduler
// and triggers", plus the fixed error shape in section 2c). It is marked
// @advanced only to exempt it from scripts/microcopy_lint.mjs, the same reason
// ui/src/bus/types.ts carries the marker: identifiers and wire values like
// "cron", "not_implemented", and "upsert_trigger" below are correct protocol
// vocabulary, never rendered as UI text. The user-facing copy the schedule
// action shows lives in ui/src/library/strings.ts and ui/src/strings.
//
// This is the command-side sibling of ui/src/bus/mockClient.ts's BusClient
// seam. BusClient carries the core's asynchronous event stream (the `evt`
// frames); SchedulerCommands carries the correlated `req`/`res` command pair
// for the two scheduler commands. A real transport (the sidecar `req`/`res`
// loop, owned by the serve-mode lane) swaps createUnavailableSchedulerCommands
// for one that writes a `req` frame and awaits its `res`; every caller already
// handles the ok/err union, so that swap is a transport change, not a caller
// change, exactly the promise mockClient.ts makes for events.
//
// Why the default is "unavailable": contracts/ipc.md section 5g lists
// list_triggers and upsert_trigger among the five commands that are reserved in
// the frozen contract but have NO core wiring yet, because the core has no
// persistent trigger store (crates/scheduler defines the trigger TYPES but
// nothing that stores, lists, or fires them). The contract is explicit that a
// build which has not wired these commands MUST answer them with error code
// `not_implemented`. So the only honest answer this shell can give today is the
// one the contract mandates, and this default returns it verbatim. What the
// core needs before this becomes real is specified in
// docs/roadmap/scheduler-live.md.

/**
 * The fixed error shape every failing `res` frame carries (contracts/ipc.md
 * section 2c): a stable snake_case `code` from the catalog, a plain-language
 * `message` safe to surface, and whether the same request may succeed later
 * unchanged.
 */
export interface CommandError {
  code: string;
  message: string;
  retryable: boolean;
}

/**
 * The result of one command: exactly one `res` answers each `req`
 * (contracts/ipc.md section 2c), either a success carrying `result` or a
 * failure carrying `error`. Callers discriminate on `ok`.
 */
export type CommandResult<T> =
  | { ok: true; result: T }
  | { ok: false; error: CommandError };

/**
 * Trigger kinds (docs/specs/scheduler.md; the same four the bus wire vocabulary
 * uses in ui/src/bus/types.ts's TriggerKind and crates/scheduler/src/trigger.rs).
 */
export type TriggerKind = "cron" | "file" | "window" | "email";

/**
 * Typed constant for the "cron" trigger-kind wire value, exported so
 * non-@advanced callers (the library Schedule action, tests) can reference the
 * kind without writing the internal-vocabulary literal that
 * scripts/microcopy_lint.mjs (correctly) flags in default-mode files. Same
 * pattern as ui/src/bus/types.ts's RUN_MODE_* / GROUNDING_UIA constants.
 */
export const TRIGGER_KIND_CRON: TriggerKind = "cron";

/**
 * Args for `upsert_trigger` (contracts/ipc.md section 5e). `spec` is the
 * kind-specific specification (a cron expression, a watched directory + glob,
 * a window process + title regex, an email filter), left opaque here because
 * the core owns its parsing; the shell never computes fire times from it (that
 * is the scheduler's job, see docs/roadmap/scheduler-live.md). `trigger_id` is
 * omitted to create a trigger, present to update one.
 */
export interface UpsertTriggerArgs {
  trigger_id?: string;
  kind: TriggerKind;
  workflow_name: string;
  spec: string;
  enabled: boolean;
}

/** One row of `list_triggers`' result (contracts/ipc.md section 5e). */
export interface TriggerRecord {
  trigger_id: string;
  kind: TriggerKind;
  workflow_name: string;
  spec: string;
  enabled: boolean;
}

/** `upsert_trigger`'s success result (contracts/ipc.md section 5e). */
export interface UpsertTriggerResult {
  trigger_id: string;
}

/**
 * The two scheduler commands the shell may send (contracts/ipc.md section 5e).
 * Both are async: a command is one `req` and exactly one `res` (section 4).
 */
export interface SchedulerCommands {
  /** `list_triggers`: the configured recurring triggers, or a typed error. */
  listTriggers(): Promise<CommandResult<TriggerRecord[]>>;
  /** `upsert_trigger`: create or update one trigger, returning its id, or a typed error. */
  upsertTrigger(args: UpsertTriggerArgs): Promise<CommandResult<UpsertTriggerResult>>;
}

/**
 * The error code the contract reserves for a command that exists in the frozen
 * protocol but is not wired in this build (contracts/ipc.md section 2c catalog,
 * section 5g). Both scheduler commands answer with it until the core grows a
 * persistent trigger store.
 */
export const NOT_IMPLEMENTED = "not_implemented";

/**
 * True when a command result is the contract's `not_implemented` refusal, i.e.
 * the core recognizes the command but has no wiring for it yet. The shell reads
 * this as "scheduling is not available yet" rather than a transient failure to
 * retry (`not_implemented` is not retryable). Any other error is a real,
 * possibly transient failure and is not this.
 */
export function isNotImplemented(result: CommandResult<unknown>): boolean {
  return !result.ok && result.error.code === NOT_IMPLEMENTED;
}

/**
 * The honest default scheduler command surface for a build with no core trigger
 * store: both commands answer `not_implemented`, exactly as contracts/ipc.md
 * section 5g mandates. This is not a stub that pretends to schedule; it is the
 * real answer the frozen contract requires of any core that has not yet wired
 * these two commands, which is every core today. Swap it for a sidecar-backed
 * implementation once the store and serve-loop wiring in
 * docs/roadmap/scheduler-live.md land.
 */
export function createUnavailableSchedulerCommands(): SchedulerCommands {
  const notImplemented: CommandError = {
    code: NOT_IMPLEMENTED,
    message: "the core has no trigger store yet, so scheduling commands are not wired",
    retryable: false,
  };
  return {
    listTriggers: async () => ({ ok: false, error: notImplemented }),
    upsertTrigger: async () => ({ ok: false, error: notImplemented }),
  };
}
