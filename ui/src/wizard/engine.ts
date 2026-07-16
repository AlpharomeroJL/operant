// A mocked backend configurator standing in for the shell/core IPC surface
// that writes engine config. Same seam pattern as ui/src/bus/mockClient.ts
// (mocks the event bus) and ui/src/settings/mockStore.ts (mocks the config
// store): the webview owns no Tauri invoke surface of its own, so this
// in-process mock simulates what the real command layer does and emits the
// exact config.changed bus events the core echoes back.
//
// It maps to the three IPC commands the engine-config step depends on
// (contracts/ipc.md section 5a/5f):
//   - configure_backend {provider, model, api_key?, endpoint?} -> Config::set,
//     echoes config.changed. Implemented in the contract.
//   - probe_backend {provider, model, endpoint?} -> {reachable, detail}.
//     Flagged NOT-YET-IMPLEMENTED in the contract, so the honest default here
//     is `not_implemented`; the wizard surfaces that as "probe unavailable" and
//     never as a green/connected result.
//   - set_settings {key, value} -> Config::set. configure_backend is the
//     higher-level of the two and is what the wizard uses.
// Swapping this for a real invoke-backed configurator later is a same-shape
// transport change, not a wizard-logic change, the same promise every other
// mock in this codebase makes.
//
// KEY SAFETY (lane B8 constraint, FR: never store/log a raw access key in the
// webview): a raw access key is passed straight through configureBackend and is
// meant to be handed to the shell/core for secure storage. It is NEVER
// published on the bus, NEVER written to a config key (config.changed is a
// durable, audited, logged surface), and NEVER retained or logged here. The
// mock records only whether a key was handed off (a boolean), never its value,
// so a test can prove the webview does not keep or leak it.

import type { BusClient } from "../bus/mockClient.ts";
import type { CoreCall } from "../boot/realSeams.ts";

/** Provider identifiers written to Config (the dotted `model.provider` key). */
export type BackendProvider = "chatgpt" | "claude" | "local";

export interface ConfigureBackendArgs {
  provider: BackendProvider;
  model: string;
  /**
   * Present only on the access-key path. Handed to the shell/core for secure
   * storage; it is deliberately not part of the config.changed keys below and
   * is never persisted or logged in the webview.
   */
  apiKey?: string;
  /** Present only on the local-model path (a localhost endpoint). */
  endpoint?: string;
}

/**
 * The honest probe outcome. `probe_backend` is flagged NOT-YET-IMPLEMENTED in
 * contracts/ipc.md, so the default outcome is `not_implemented`. `checking` and
 * `idle` are wizard-side transient states the configurator itself never
 * returns.
 */
export type ProbeState = "idle" | "checking" | "reachable" | "unreachable" | "not_implemented" | "unavailable";

export interface ProbeResult {
  /** One of the resolvable outcomes: reachable, unreachable, not_implemented, unavailable. */
  state: Exclude<ProbeState, "idle" | "checking">;
  /** A short, plain-language sentence, safe to surface (contracts/ipc.md section 5a). */
  detail: string;
}

export interface BackendConfigurator {
  /**
   * Writes provider/model config (echoing config.changed for the non-secret
   * dotted keys) and hands any access key to the shell/core out of band.
   * Resolves once the write is accepted.
   */
  configureBackend(args: ConfigureBackendArgs): Promise<void>;
  /**
   * Calls probe_backend. Honest by contract: a not-yet-implemented core answers
   * `not_implemented` and the wizard must not paint a green probe.
   */
  probeBackend(args: { provider: BackendProvider; model: string; endpoint?: string }): Promise<ProbeResult>;
}

/** What the mock captured about the last configure call. The raw key is never here. */
export interface ConfiguredSummary {
  provider: BackendProvider;
  model: string;
  endpoint?: string;
  /** Whether an access key was handed off. The raw value is never stored. */
  hadApiKey: boolean;
}

export interface MockBackendConfiguratorOptions {
  /** Override the probe outcome for tests. Defaults to the honest not-yet-implemented state. */
  probe?: ProbeResult;
}

export interface MockBackendConfigurator extends BackendConfigurator {
  /** The last accepted configuration, for test assertions. Never carries the raw key. */
  getConfigured(): ConfiguredSummary | null;
}

/**
 * The honest default probe outcome: the contract flags probe_backend as
 * NOT-YET-IMPLEMENTED, so a build that has not wired it answers with the
 * `not_implemented` error code. The wizard renders this as "probe unavailable",
 * never as a reachable/connected result.
 */
export const NOT_IMPLEMENTED_PROBE: ProbeResult = {
  state: "not_implemented",
  detail: "This build cannot check the connection yet.",
};

/**
 * A fresh mock per call, isolated from every other instance (same construction
 * pattern as createMockSettingsStore), optionally wired to a bus so the
 * non-secret config.changed events reach the same audit sink and Settings
 * screen a real core's echo would.
 */
export function createMockBackendConfigurator(bus?: BusClient, opts: MockBackendConfiguratorOptions = {}): MockBackendConfigurator {
  let configured: ConfiguredSummary | null = null;
  const probeResult: ProbeResult = opts.probe ?? NOT_IMPLEMENTED_PROBE;

  async function configureBackend(args: ConfigureBackendArgs): Promise<void> {
    // The real config the core persists via Config::set (dotted keys per
    // docs/specs/ipc-bridge.md section 7). model.planner flips from the mock
    // planner to a real one so start_teach_run assembles a real backend
    // afterward (crates/core/src/config.rs uses exactly this "mock_planner" /
    // "real_planner" vocabulary). The access key is deliberately absent: the
    // shell routes it to secure storage, it never rides config.changed.
    bus?.publish("config.changed", { key: "model.provider", value: args.provider });
    bus?.publish("config.changed", { key: "model.name", value: args.model });
    bus?.publish("config.changed", { key: "model.planner", value: "real_planner" });
    if (args.endpoint) bus?.publish("config.changed", { key: "model.endpoint", value: args.endpoint });

    // Record only the fact of a key handoff, never its value.
    configured = {
      provider: args.provider,
      model: args.model,
      endpoint: args.endpoint,
      hadApiKey: Boolean(args.apiKey && args.apiKey.trim().length > 0),
    };
  }

  async function probeBackend(): Promise<ProbeResult> {
    return probeResult;
  }

  return { configureBackend, probeBackend, getConfigured: () => configured };
}

/**
 * The wizard's engine choice mapped to the core provider id from the
 * model-backend quirk table (contracts/model_backend.md: `openai`, `anthropic`,
 * `ollama`, ...). The wizard speaks in product terms (the ChatGPT card, the
 * Claude card, the on-device option); the core's configure_backend wants the
 * quirk-table id. This is the one place that translation happens. The on-device
 * choice maps to `ollama`, whose base_url the wizard passes as the endpoint and
 * which needs no key.
 */
const CORE_PROVIDER_ID: Record<BackendProvider, string> = {
  chatgpt: "openai",
  claude: "anthropic",
  local: "ollama",
};

/**
 * The real, invoke-backed configurator: the wizard's engine choice becomes a
 * real `configure_backend` command on the core over ui/src/boot/realSeams.ts's
 * coreCall (B2's core_call), which stores model.provider/model/endpoint/api_key
 * in Config so the core's build_planner assembles a real HttpBackend
 * (contracts/ipc.md section 5a). This is the shipped path; only Demo keeps
 * createMockBackendConfigurator. Same seam shape as every other createReal* in
 * ui/src/boot/realSeams.ts.
 *
 * Provider translation (contracts/model_backend.md quirk table): the ChatGPT
 * choice is provider `openai`, Claude is `anthropic`, the on-device choice is
 * `ollama`.
 *
 * KEY SAFETY (same promise the mock makes): a raw access key rides `api_key`
 * straight to the core for secure storage. It is NEVER held, logged, or echoed
 * here; this configurator keeps no state at all.
 */
export function createRealBackendConfigurator(coreCall: CoreCall): BackendConfigurator {
  async function configureBackend(args: ConfigureBackendArgs): Promise<void> {
    const wire: Record<string, unknown> = { provider: CORE_PROVIDER_ID[args.provider], model: args.model };
    // endpoint rides only when present (the on-device path); api_key rides only
    // on the access-key path. Both stay off the wire otherwise so the core sees
    // exactly the keys contracts/ipc.md section 5a names.
    if (args.endpoint) wire.endpoint = args.endpoint;
    if (args.apiKey && args.apiKey.trim().length > 0) wire.api_key = args.apiKey;
    // configure_backend -> {ok:true}; the promise resolves once the core accepts
    // the write (coreCall rejects on ok:false, so a failed write surfaces).
    await coreCall("configure_backend", wire);
  }

  async function probeBackend(args: { provider: BackendProvider; model: string; endpoint?: string }): Promise<ProbeResult> {
    // Honest by contract (the createRealScheduler philosophy): genuinely ask the
    // core, never fake a green. probe_backend is NOT-YET-IMPLEMENTED
    // (contracts/ipc.md section 5a), so a real core answers not_implemented,
    // which arrives as a rejected core_call. Map the outcomes:
    //   {reachable:true}  -> reachable
    //   {reachable:false} -> unreachable
    //   not_implemented   -> not_implemented (the honest "cannot check yet")
    //   any other fault   -> unavailable
    const wire: Record<string, unknown> = { provider: CORE_PROVIDER_ID[args.provider], model: args.model };
    if (args.endpoint) wire.endpoint = args.endpoint;
    try {
      const res = await coreCall<{ reachable?: boolean; detail?: unknown }>("probe_backend", wire);
      const detail = typeof res?.detail === "string" ? res.detail : "";
      return res?.reachable === true ? { state: "reachable", detail } : { state: "unreachable", detail };
    } catch (err) {
      const code = err && typeof err === "object" && typeof (err as { code?: unknown }).code === "string" ? (err as { code: string }).code : "";
      if (code === "not_implemented") return NOT_IMPLEMENTED_PROBE;
      return { state: "unavailable", detail: NOT_IMPLEMENTED_PROBE.detail };
    }
  }

  return { configureBackend, probeBackend };
}
