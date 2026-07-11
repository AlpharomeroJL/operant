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
   * Execute a fix command. Maps the command name to a dispatched event or
   * internal action. For now, a test seam that demonstrates fix_command was
   * parsed; a real implementation would coordinate with the backend.
   */
  fix(findingId: string): void;
  dispose(): void;
}

export interface CreateDoctorOptions {
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
    fix,
    dispose() {
      unsubscribe();
      listeners.clear();
    },
  };
}
