// @advanced
// Capability-handshake logic for the shell-to-core boot (contracts/ipc.md
// section 3), kept free of any DOM or Tauri import so it is unit-testable on
// its own. The field names below (real_uia, real_input, ...) are the frozen
// wire capability object; @advanced exempts them from scripts/microcopy_lint.mjs
// the same way ui/src/bus/types.ts is exempt, since they are protocol
// vocabulary. The human text a person reads lives in ./coreGateView.ts.

/** The section 3 capability object, byte-shape per contracts/ipc.md. */
export interface CoreCapabilities {
  real_uia: boolean;
  real_input: boolean;
  real_vision: boolean;
  mock_planner_only: boolean;
  transport_kind: string;
  version: string;
  git_sha: string;
}

/**
 * Real automation requires BOTH live perception and real input. This mirrors
 * the CLI E4 rule (a real run needs both features; either alone silently
 * degrades to mock) and is the structural guarantee that a demo build can
 * never present itself as a product (contracts/ipc.md section 3).
 */
export function canAutomate(caps: CoreCapabilities): boolean {
  return caps.real_uia === true && caps.real_input === true;
}

export interface MissingCapability {
  /** The contract field reported false, named verbatim so the failure is legible. */
  field: "real_uia" | "real_input";
}

/** Each automation capability the core reported false, in contract order. */
export function missingCapabilities(caps: CoreCapabilities): MissingCapability[] {
  const missing: MissingCapability[] = [];
  if (!caps.real_uia) missing.push({ field: "real_uia" });
  if (!caps.real_input) missing.push({ field: "real_input" });
  return missing;
}

export type CoreConnection =
  | { kind: "real"; capabilities: CoreCapabilities }
  | { kind: "blocked"; capabilities: CoreCapabilities; missing: MissingCapability[] }
  | { kind: "error"; message: string };

export interface CoreHandshake {
  /** invoke("core_ready"): resolves once the core child is up and has sent its ready frame. */
  ready: () => Promise<unknown>;
  /** invoke("core_capabilities"): the section 3 capability object. */
  capabilities: () => Promise<CoreCapabilities>;
}

/**
 * Runs the handshake and decides how the shell may render:
 * - "real": the core can automate; the caller builds the real-work UI.
 * - "blocked": the core cannot automate; the caller shows the blocking screen
 *   naming each missing capability.
 * - "error": the handshake itself failed; the caller shows an error state and
 *   MUST NOT fall back to canned data. A failed real connection is never
 *   silently swapped for the demo (contracts/ipc.md section 3).
 */
export async function handshakeCore(handshake: CoreHandshake): Promise<CoreConnection> {
  let caps: CoreCapabilities;
  try {
    await handshake.ready();
    caps = await handshake.capabilities();
  } catch (err) {
    return { kind: "error", message: errorMessage(err) };
  }
  if (canAutomate(caps)) return { kind: "real", capabilities: caps };
  return { kind: "blocked", capabilities: caps, missing: missingCapabilities(caps) };
}

/** Best-effort technical detail from a thrown handshake failure; may be empty. */
function errorMessage(err: unknown): string {
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string") return err;
  return "";
}
