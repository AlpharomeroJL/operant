// The dev/Demo fallback for the "Check my setup" screen: one canned finding
// per real doctor check, published on the mock bus when there is no core to
// run the real `run_doctor` command (contracts/ipc.md section 5f). The desktop
// app replaces this with the core's real findings, which arrive as the same
// `doctor.finding` events over the bridge, so the screen's render path is
// identical either way.
//
// The check set mirrors crates/doctor/src/checks.rs (the CLI `operant doctor`
// verb): model reachable, disk free, updater reachable, accessibility
// permission, audio devices present, graphics-memory headroom. Every string
// below is copied verbatim from that crate's catalog (crates/doctor/src/
// catalog.rs) and healthy-check copy, so this screen and a real run say the
// same thing the same way, and so this file stays clear of the microcopy
// glossary's internal terms the way the Rust catalog's own test proves it does.
//
// The seeded state is a believable fresh-machine scan: mostly healthy, with one
// automatable problem (low disk, an error with a one-click fix) and one
// advice-only problem (no microphone or speakers, a warning to act on by hand).

import type { DoctorFindingPayload } from "../bus/types.ts";

export const DEMO_DOCTOR_FINDINGS: ReadonlyArray<DoctorFindingPayload> = [
  {
    finding_id: "model_reachable",
    severity: "info",
    what: "The model is reachable.",
    why: "Operant was able to connect to it just now.",
    action: "No action needed.",
  },
  {
    finding_id: "disk_free",
    severity: "error",
    what: "Your computer ran low on free disk space.",
    why: "Operant and the apps it controls need free space to save files safely.",
    action: "Free up some disk space, then try again.",
    fix_command: "operant doctor --fix disk_free",
  },
  {
    finding_id: "updater_reachable",
    severity: "info",
    what: "Operant can check for updates.",
    why: "Operant was able to connect to the update server just now.",
    action: "No action needed.",
  },
  {
    finding_id: "accessibility_permission",
    severity: "info",
    what: "Operant has permission to see and control the screen.",
    why: "Operant checked this computer's permission settings.",
    action: "No action needed.",
  },
  {
    finding_id: "audio_devices_present",
    severity: "warn",
    what: "Operant could not find a microphone or speakers to use.",
    why: "Voice features need a working microphone and speakers connected to this computer.",
    action: "Connect a microphone and speakers, then try again.",
    // Advice-only on purpose: plugging in hardware is not something a
    // one-click fix can do, so no fix_command and so no Fix button.
  },
  {
    finding_id: "vram_headroom",
    severity: "info",
    what: "There is enough graphics memory for the selected model.",
    why: "Operant compared the graphics memory this computer has free against what the model needs.",
    action: "No action needed.",
  },
];

// Healthy replacements, keyed by finding id, for the demo one-click fix: after
// a fix is "applied" (dev/Demo has no real disk to clear, so it just stands in
// for the effect), the screen republishes the finding as healthy so its card
// turns green and loses its Fix button in place, the same transition a real
// fix produces when the core re-checks and republishes.
const DEMO_HEALTHY_FINDINGS: Readonly<Record<string, DoctorFindingPayload>> = {
  disk_free: {
    finding_id: "disk_free",
    severity: "info",
    what: "There is enough free disk space.",
    why: "Operant checked the free space on this computer's drive just now.",
    action: "No action needed.",
  },
};

/** The healthy version of a fixable demo finding, or null when this id has no canned healthy state. */
export function demoHealthyFinding(findingId: string): DoctorFindingPayload | null {
  return DEMO_HEALTHY_FINDINGS[findingId] ?? null;
}
