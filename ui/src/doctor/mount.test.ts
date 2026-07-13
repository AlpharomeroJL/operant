// DOM assertions for the "Check my setup" screen, wired the way ui/src/main.ts
// wires it: createDoctor(bus, { runChecks, onFixRequested }) with mountDoctor
// re-run on every snapshot, driven by doctor.finding events over the bus. jsdom
// via ui/src/styles/testDomEnv.ts, the same harness ui/src/tray/view.test.ts
// uses. Verified through the DOM (screenshots are unreliable): what actually
// renders, which cards get a Fix button, and that the one-click fix turns a
// card healthy in place.
//
// The dev/Demo branch is exercised here (no Tauri bridge), so runChecks
// publishes the canned DEMO_DOCTOR_FINDINGS and onFixRequested stands in for
// the fix by republishing the finding healthy, byte-for-byte the fallback
// ui/src/main.ts runs when no core is attached.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient, type BusClient } from "../bus/mockClient.ts";
import { createDoctor } from "./state.ts";
import { mountDoctor } from "./view.ts";
import { DEMO_DOCTOR_FINDINGS, demoHealthyFinding } from "./demoFindings.ts";

function wireDoctor(env: ReturnType<typeof createDomEnv>, bus: BusClient) {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);

  const doctor = createDoctor(bus, {
    runChecks: () => {
      for (const finding of DEMO_DOCTOR_FINDINGS) bus.publish("doctor.finding", finding);
    },
    onFixRequested: (findingId) => {
      bus.publish("doctor.fixed", { finding_id: findingId });
      const healthy = demoHealthyFinding(findingId);
      if (healthy) bus.publish("doctor.finding", healthy);
    },
  });

  function render(): void {
    mountDoctor(container, doctor.getSnapshot(), {
      onFix: (findingId) => doctor.fix(findingId),
      onClose: () => {},
    });
  }
  doctor.subscribe(render);
  render();
  return { container, doctor };
}

test("before a scan: the screen mounts with its title and no finding cards", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const { container, doctor } = wireDoctor(env, bus);

    assert.ok(container.querySelector(".op-doctor"), "the doctor section mounts");
    assert.equal(container.querySelector("#op-doctor-heading")?.textContent, "Check my setup");
    assert.equal(container.querySelectorAll(".op-doctor-card").length, 0);
    assert.ok(container.querySelector(".op-empty"), "the healthy/empty note shows before a scan");

    doctor.dispose();
  } finally {
    env.cleanup();
  }
});

test("open() runs the checks: one card per doctor.finding, rendering the real catalog copy", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const { container, doctor } = wireDoctor(env, bus);

    doctor.open();

    const cards = container.querySelectorAll(".op-doctor-card");
    assert.equal(cards.length, DEMO_DOCTOR_FINDINGS.length);

    const disk = container.querySelector(".op-doctor-card--error");
    assert.ok(disk, "the low-disk finding is tinted as an error");
    assert.equal(
      disk!.querySelector(".op-doctor-card__what")?.textContent,
      "Your computer ran low on free disk space.",
    );
    assert.equal(
      disk!.querySelector(".op-doctor-card__action")?.textContent,
      "Free up some disk space, then try again.",
    );

    doctor.dispose();
  } finally {
    env.cleanup();
  }
});

test("a fixable finding shows a Fix button; an advice-only finding shows none", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const { container, doctor } = wireDoctor(env, bus);
    doctor.open();

    const errorCard = container.querySelector(".op-doctor-card--error");
    assert.ok(errorCard!.querySelector("button"), "the low-disk error offers a one-click fix");

    const warnCard = container.querySelector(".op-doctor-card--warn");
    assert.ok(warnCard, "the no-audio warning renders");
    assert.equal(warnCard!.querySelector("button"), null, "advice-only: no Fix button");

    doctor.dispose();
  } finally {
    env.cleanup();
  }
});

test("fix() hands the finding's exact fix_command to the seam (contracts/ipc.md fix_command)", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    const seen: Array<{ id: string; command: string }> = [];
    const doctor = createDoctor(bus, {
      runChecks: () => {
        for (const finding of DEMO_DOCTOR_FINDINGS) bus.publish("doctor.finding", finding);
      },
      onFixRequested: (id, command) => seen.push({ id, command }),
    });
    doctor.subscribe(() => mountDoctor(container, doctor.getSnapshot(), { onFix: (id) => doctor.fix(id) }));
    mountDoctor(container, doctor.getSnapshot(), { onFix: (id) => doctor.fix(id) });

    doctor.open();
    container.querySelector<HTMLButtonElement>(".op-doctor-card--error button")!.click();

    assert.deepEqual(seen, [{ id: "disk_free", command: "operant doctor --fix disk_free" }]);

    doctor.dispose();
  } finally {
    env.cleanup();
  }
});

test("clicking Fix turns the low-disk card healthy in place (the one-click fix path)", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const { container, doctor } = wireDoctor(env, bus);
    doctor.open();

    assert.equal(container.querySelectorAll(".op-doctor-card").length, DEMO_DOCTOR_FINDINGS.length);
    const fixButton = container.querySelector<HTMLButtonElement>(".op-doctor-card--error button");
    assert.ok(fixButton);
    fixButton!.click();

    // Same card count (the finding updated in place, not removed), but the
    // low-disk card is now healthy: no error tint, no Fix button.
    assert.equal(container.querySelectorAll(".op-doctor-card").length, DEMO_DOCTOR_FINDINGS.length);
    assert.equal(container.querySelector(".op-doctor-card--error"), null, "no error card remains after the fix");

    const disk = Array.from(container.querySelectorAll(".op-doctor-card")).find(
      (c) => c.querySelector(".op-doctor-card__what")?.textContent === "There is enough free disk space.",
    );
    assert.ok(disk, "the low-disk card now reports healthy");
    assert.equal(disk!.querySelector("button"), null, "healthy: the Fix button is gone");

    doctor.dispose();
  } finally {
    env.cleanup();
  }
});

test("open() is a fresh scan: it clears prior findings before the new ones arrive", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    const doctor = createDoctor(bus, {
      runChecks: () =>
        bus.publish("doctor.finding", {
          finding_id: "disk_free",
          severity: "info",
          what: "There is enough free disk space.",
          why: "Operant checked the free space on this computer's drive just now.",
          action: "No action needed.",
        }),
    });
    doctor.subscribe(() => mountDoctor(container, doctor.getSnapshot(), {}));
    mountDoctor(container, doctor.getSnapshot(), {});

    // A stale finding from some earlier scan lingers in the view.
    bus.publish("doctor.finding", {
      finding_id: "model_reachable",
      severity: "error",
      what: "Operant could not reach the model it is set up to use.",
      why: "The model may be turned off, or the connection to it is down.",
      action: "Check that the model is running and connected, then try again.",
      fix_command: "operant doctor --fix model_reachable",
    });
    assert.equal(container.querySelectorAll(".op-doctor-card").length, 1);

    doctor.open();

    // Only the fresh scan's single finding remains; the stale model card is gone.
    const cards = container.querySelectorAll(".op-doctor-card");
    assert.equal(cards.length, 1);
    assert.equal(cards[0].querySelector(".op-doctor-card__what")?.textContent, "There is enough free disk space.");

    doctor.dispose();
  } finally {
    env.cleanup();
  }
});

test("the Close button renders when onClose is given and invokes it", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    const doctor = createDoctor(bus);

    let closed = 0;
    mountDoctor(container, doctor.getSnapshot(), { onClose: () => closed++ });

    const closeBtn = container.querySelector<HTMLButtonElement>(".op-doctor__close");
    assert.ok(closeBtn, "a Close button appears when onClose is provided");
    closeBtn!.click();
    assert.equal(closed, 1);

    doctor.dispose();
  } finally {
    env.cleanup();
  }
});
