// The "Check my setup" surface (docs/specs/ui.md: "doctor check: six checks,
// cards with findings, one-click fixes for automatable problems, advice-only
// for the rest"). Subscribes to the doctor.finding bus topic and renders one
// card per finding. Pure and DOM-free, same split as ui/src/library/state.ts.

import type { BusClient } from "../bus/mockClient.ts";
import type { BusEvent, DoctorFindingPayload } from "../bus/types.ts";
import { doctorStrings } from "../strings/default.ts";

export interface DoctorCard {
  findingId: string;
  severity: string;
  what: string;
  why: string;
  action: string;
  fixCommand?: string;
  fixLabel: string;
}

export interface DoctorSnapshot {
  title: string;
  cards: DoctorCard[];
  empty: boolean;
}

export interface Doctor {
  getSnapshot(): DoctorSnapshot;
  subscribe(fn: (snap: DoctorSnapshot) => void): () => void;
  /**
   * Run the checks now. Clears any prior findings (a fresh scan) and asks the
   * `runChecks` seam to produce this scan's findings. Per contracts/ipc.md
   * (the `run_doctor` command), the findings do not come back from this call:
   * they arrive asynchronously as `doctor.finding` events on the bus, which
   * the subscription above renders. Opening the "Check my setup" screen calls
   * this, so a real scan runs every time it is shown.
   */
  open(): void;
  /**
   * Execute a fix command. Looks up the finding, and when it has a
   * `fix_command`, hands both the finding id and that command to the
   * `onFixRequested` seam. The observable result (the finding turning
   * healthy) arrives back as a fresh `doctor.finding` event, the same path a
   * real one-click fix takes: issue the command, re-render from the event.
   */
  fix(findingId: string): void;
  dispose(): void;
}

export interface CreateDoctorOptions {
  /**
   * Issue the `run_doctor` command (contracts/ipc.md section 5f). Invoked by
   * `open()`. In the desktop app this reaches the real core, which runs every
   * check and publishes each result as a `doctor.finding` event; in dev/Demo
   * mode (the mock bus, no core) main.ts falls back to publishing canned
   * findings so the screen still renders. Either way the findings arrive as
   * events, never as this callback's return, so the render path is identical.
   */
  runChecks?: () => void;
  onFixRequested?: (findingId: string, command: string) => void;
}

export function createDoctor(bus: BusClient, opts: CreateDoctorOptions = {}): Doctor {
  const findings = new Map<string, DoctorCard>();
  const listeners = new Set<(snap: DoctorSnapshot) => void>();

  function cardFor(payload: DoctorFindingPayload): DoctorCard {
    return {
      findingId: payload.finding_id,
      severity: payload.severity,
      what: payload.what,
      why: payload.why,
      action: payload.action,
      fixCommand: payload.fix_command,
      fixLabel: doctorStrings.fixButton,
    };
  }

  function snapshot(): DoctorSnapshot {
    const cards = Array.from(findings.values());
    return {
      title: doctorStrings.title,
      cards,
      empty: cards.length === 0,
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function handleBus(event: BusEvent): void {
    if (event.topic !== "doctor.finding") return;
    const payload = event.payload as DoctorFindingPayload;
    const card = cardFor(payload);
    findings.set(payload.finding_id, card);
    emit();
  }

  const unsubscribe = bus.subscribe("doctor.finding", handleBus);

  function open(): void {
    findings.clear();
    emit();
    opts.runChecks?.();
  }

  function fix(findingId: string): void {
    const card = findings.get(findingId);
    if (!card || !card.fixCommand) return;
    opts.onFixRequested?.(findingId, card.fixCommand);
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    open,
    fix,
    dispose() {
      unsubscribe();
      listeners.clear();
    },
  };
}
