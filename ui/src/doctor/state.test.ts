import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import type { DoctorFindingPayload } from "../bus/types.ts";
import { createDoctor } from "./state.ts";

test("doctor renders findings as cards: what, why, action, and fix button if automatable", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  const finding: DoctorFindingPayload = {
    finding_id: "disk_free",
    severity: "error",
    what: "Your computer ran low on free disk space.",
    why: "Operant and the apps it controls need free space to save files safely.",
    action: "Free up some disk space, then try again.",
    fix_command: "operant doctor --fix disk_free",
  };

  bus.publish("doctor.finding", finding);

  const snap = doctor.getSnapshot();
  assert.equal(snap.empty, false);
  assert.equal(snap.cards.length, 1);

  const [card] = snap.cards;
  assert.equal(card.findingId, "disk_free");
  assert.equal(card.severity, "error");
  assert.equal(card.what, "Your computer ran low on free disk space.");
  assert.equal(card.why, "Operant and the apps it controls need free space to save files safely.");
  assert.equal(card.action, "Free up some disk space, then try again.");
  assert.equal(card.fixCommand, "operant doctor --fix disk_free");
  assert.equal(card.fixLabel, "Fix it");

  doctor.dispose();
});

test("seeded broken state: disk low finding yields the right card with fix command", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "error",
    what: "Your computer ran low on free disk space.",
    why: "Operant and the apps it controls need free space to save files safely.",
    action: "Free up some disk space, then try again.",
    fix_command: "operant doctor --fix disk_free",
  });

  const snap = doctor.getSnapshot();
  assert.equal(snap.cards.length, 1);
  const card = snap.cards[0];
  assert.equal(card.findingId, "disk_free");
  assert.equal(card.severity, "error");
  assert.equal(card.fixCommand, "operant doctor --fix disk_free");

  doctor.dispose();
});

test("seeded broken state: model unreachable finding yields the right card with fix command", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  bus.publish("doctor.finding", {
    finding_id: "model_reachable",
    severity: "error",
    what: "Operant could not reach the model it is set up to use.",
    why: "The model may be turned off, or the connection to it is down.",
    action: "Check that the model is running and connected, then try again.",
    fix_command: "operant doctor --fix model_reachable",
  });

  const snap = doctor.getSnapshot();
  assert.equal(snap.cards.length, 1);
  const card = snap.cards[0];
  assert.equal(card.findingId, "model_reachable");
  assert.equal(card.fixCommand, "operant doctor --fix model_reachable");

  doctor.dispose();
});

test("non-automatable findings show action/advice only, no fix button", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  bus.publish("doctor.finding", {
    finding_id: "audio_devices_present",
    severity: "warn",
    what: "Operant could not find a microphone or speakers to use.",
    why: "Voice features need a working microphone and speakers connected to this computer.",
    action: "Connect a microphone and speakers, then try again.",
    // No fix_command
  });

  const snap = doctor.getSnapshot();
  const card = snap.cards[0];
  assert.equal(card.fixCommand, undefined);
  assert.equal(card.action, "Connect a microphone and speakers, then try again.");

  doctor.dispose();
});

test("healthy findings render with info severity and no fix needed", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "info",
    what: "There is enough free disk space.",
    why: "Operant checked the free space on this computer's drive just now.",
    action: "No action needed.",
  });

  const snap = doctor.getSnapshot();
  const card = snap.cards[0];
  assert.equal(card.severity, "info");
  assert.equal(card.fixCommand, undefined);
  assert.equal(card.action, "No action needed.");

  doctor.dispose();
});

test("clicking fix() triggers the onFixRequested callback with the command", () => {
  const bus = createMockBusClient();
  const fixes: Array<{ findingId: string; command: string }> = [];
  const doctor = createDoctor(bus, {
    onFixRequested: (findingId, command) => fixes.push({ findingId, command }),
  });

  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "error",
    what: "Low disk.",
    why: "Why?",
    action: "Free up space.",
    fix_command: "operant doctor --fix disk_free",
  });

  doctor.fix("disk_free");

  assert.deepEqual(fixes, [{ findingId: "disk_free", command: "operant doctor --fix disk_free" }]);

  doctor.dispose();
});

test("clicking fix() on a non-automatable finding is a no-op", () => {
  const bus = createMockBusClient();
  const fixes: Array<{ findingId: string; command: string }> = [];
  const doctor = createDoctor(bus, {
    onFixRequested: (findingId, command) => fixes.push({ findingId, command }),
  });

  bus.publish("doctor.finding", {
    finding_id: "audio_devices_present",
    severity: "warn",
    what: "No audio.",
    why: "Why?",
    action: "Connect audio.",
    // No fix_command
  });

  doctor.fix("audio_devices_present");

  assert.deepEqual(fixes, []);

  doctor.dispose();
});

test("fix() on a finding that does not exist is a no-op", () => {
  const bus = createMockBusClient();
  const fixes: Array<{ findingId: string; command: string }> = [];
  const doctor = createDoctor(bus, {
    onFixRequested: (findingId, command) => fixes.push({ findingId, command }),
  });

  doctor.fix("nonexistent");

  assert.deepEqual(fixes, []);

  doctor.dispose();
});

test("multiple findings accumulate and render as separate cards", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "error",
    what: "Low disk.",
    why: "Why?",
    action: "Free up space.",
    fix_command: "operant doctor --fix disk_free",
  });

  bus.publish("doctor.finding", {
    finding_id: "model_reachable",
    severity: "error",
    what: "Model unreachable.",
    why: "Why?",
    action: "Check model.",
    fix_command: "operant doctor --fix model_reachable",
  });

  bus.publish("doctor.finding", {
    finding_id: "audio_devices_present",
    severity: "warn",
    what: "No audio.",
    why: "Why?",
    action: "Connect audio.",
  });

  const snap = doctor.getSnapshot();
  assert.equal(snap.empty, false);
  assert.equal(snap.cards.length, 3);
  assert.equal(snap.cards[0].findingId, "disk_free");
  assert.equal(snap.cards[1].findingId, "model_reachable");
  assert.equal(snap.cards[2].findingId, "audio_devices_present");

  doctor.dispose();
});

test("an empty findings set renders the empty message", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  const snap = doctor.getSnapshot();
  assert.equal(snap.empty, true);
  assert.equal(snap.cards.length, 0);
  assert.equal(snap.title, "Check my setup");

  doctor.dispose();
});

test("subscriber is notified when a new finding arrives", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  let notified = 0;
  doctor.subscribe(() => notified++);

  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "error",
    what: "Low disk.",
    why: "Why?",
    action: "Free up space.",
    fix_command: "operant doctor --fix disk_free",
  });

  assert.ok(notified >= 1);

  doctor.dispose();
});

test("dispose stops listening to the bus and clears subscribers", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  let notified = 0;
  doctor.subscribe(() => notified++);

  doctor.dispose();

  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "error",
    what: "Low disk.",
    why: "Why?",
    action: "Free up space.",
  });

  assert.equal(notified, 0);
});

test("replacing a finding with the same id updates the card in place", () => {
  const bus = createMockBusClient();
  const doctor = createDoctor(bus);

  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "error",
    what: "Low disk.",
    why: "Why?",
    action: "Free up space.",
    fix_command: "operant doctor --fix disk_free",
  });

  let snap = doctor.getSnapshot();
  assert.equal(snap.cards.length, 1);
  assert.equal(snap.cards[0].severity, "error");

  // Simulate fixing the issue by republishing as healthy
  bus.publish("doctor.finding", {
    finding_id: "disk_free",
    severity: "info",
    what: "There is enough free disk space.",
    why: "Operant checked.",
    action: "No action needed.",
  });

  snap = doctor.getSnapshot();
  assert.equal(snap.cards.length, 1, "Still one card, same id");
  assert.equal(snap.cards[0].severity, "info", "But it transitioned to healthy");
  assert.equal(snap.cards[0].fixCommand, undefined, "No fix command anymore");

  doctor.dispose();
});
