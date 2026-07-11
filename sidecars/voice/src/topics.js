// Bus topic strings this sidecar publishes or reads. The "Sidecars and VRAM"
// and "Gates, approvals, escalations" families come straight from
// contracts/bus_events.md; VOICE_INTENT is new (see the comment below).
//
// contracts/bus_events.md versioning rule 3: "New topics may be added freely;
// consumers subscribe by explicit topic or prefix and must not crash on
// unknown topics." This lane's owned path is sidecars/voice only, so it
// cannot edit contracts/bus_events.md itself. VOICE_INTENT is published under
// that rule; whoever owns contracts/ should still add a row for it next time
// that file is touched:
//   | voice.intent | source, text | push-to-talk transcript, routed to the palette |
export const TOPIC = Object.freeze({
  SIDECAR_STARTED: "sidecar.started",
  SIDECAR_HEALTH: "sidecar.health",
  SIDECAR_CRASHED: "sidecar.crashed",
  SIDECAR_RESTARTED: "sidecar.restarted",
  VRAM_REQUEST: "vram.request",
  VRAM_GRANT: "vram.grant",
  VRAM_YIELD: "vram.yield",
  GATE_ESCALATION: "gate.escalation",
  VOICE_INTENT: "voice.intent",
});
