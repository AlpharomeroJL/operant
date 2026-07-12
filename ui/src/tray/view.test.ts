// DOM assertions for ./view.ts. jsdom via ui/src/styles/testDomEnv.ts, the
// same harness ui/src/undo/view.test.ts uses.
//
// The digest tests drive the real createTray(bus) through a
// metrics.week.rolled fixture event (not a hand-typed TraySnapshot), so
// this proves the restyled digest actually renders from fixture metrics
// end to end: bus payload -> state -> view, the same path production takes.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE } from "../bus/types.ts";
import { createTray, type Tray } from "./state.ts";
import { mountTray } from "./view.ts";
import { trayNotificationStrings } from "./strings.ts";

function mount(env: ReturnType<typeof createDomEnv>, tray: Tray) {
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  function render(): HTMLElement {
    return mountTray(container, tray.getSnapshot());
  }
  return { container, render };
}

test("no notifications: renders the glyph but no notification list", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const tray = createTray(bus);
    const { container, render } = mount(env, tray);
    render();

    assert.ok(container.querySelector(".op-tray__glyph"));
    assert.equal(container.querySelector(".op-tray__notifications"), null);

    tray.dispose();
  } finally {
    env.cleanup();
  }
});

test("a weekly digest fixture (metrics.week.rolled, 192 minutes) renders the restyled mono stat, aria-hidden, plus the full sentence for accessibility", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const tray = createTray(bus);
    bus.publish("metrics.week.rolled", { week: "2026-W28", minutes_saved_total: 192 });

    const { container, render } = mount(env, tray);
    render();

    const item = container.querySelector(".op-tray__notification");
    assert.ok(item);
    assert.ok(item!.classList.contains("op-tray__notification--digest"), "the digest must get its own restyled modifier class");

    const figure = item!.querySelector(".op-tray__digest-figure");
    assert.ok(figure, "the digest must render a mono stat figure");
    assert.equal(figure!.getAttribute("aria-hidden"), "true");

    const number = figure!.querySelector(".op-tray__digest-number");
    assert.equal(number?.textContent, "192");

    const unit = figure!.querySelector(".op-tray__digest-unit");
    assert.equal(unit?.textContent?.trim(), trayNotificationStrings.digestUnit);

    const title = item!.querySelector(".op-tray__notification-title");
    assert.equal(title?.textContent, "Your weekly time saved");

    const body = item!.querySelector(".op-tray__notification-body");
    assert.equal(body?.textContent?.trim(), "Saved about 192 minutes this week");

    tray.dispose();
  } finally {
    env.cleanup();
  }
});

test("a halted alert notification does NOT get the digest treatment: no mono stat, no digest modifier class", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const tray = createTray(bus);
    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    bus.publish("run.halted", { run_id: "r1", reason: "killswitch" });

    const { container, render } = mount(env, tray);
    render();

    const item = container.querySelector(".op-tray__notification");
    assert.ok(item);
    assert.equal(item!.classList.contains("op-tray__notification--digest"), false);
    assert.equal(item!.querySelector(".op-tray__digest-figure"), null);
    assert.equal(item!.querySelector(".op-tray__notification-title")?.textContent, "Operant stopped");

    tray.dispose();
  } finally {
    env.cleanup();
  }
});

test("a halted alert and a weekly digest together: only the digest entry gets restyled, each keeps its own figures", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const tray = createTray(bus);
    bus.publish("run.started", { run_id: "r1", goal: "g", mode: RUN_MODE_EXPLORE });
    bus.publish("run.halted", { run_id: "r1", reason: "killswitch" });
    bus.publish("metrics.week.rolled", { week: "2026-W28", minutes_saved_total: 47 });

    const { container, render } = mount(env, tray);
    render();

    const items = Array.from(container.querySelectorAll(".op-tray__notification"));
    assert.equal(items.length, 2);

    const digestItems = items.filter((i) => i.classList.contains("op-tray__notification--digest"));
    assert.equal(digestItems.length, 1);
    assert.equal(digestItems[0].querySelector(".op-tray__digest-number")?.textContent, "47");

    tray.dispose();
  } finally {
    env.cleanup();
  }
});

test("dismissing the digest notification removes it via the same Dismiss button every notification uses", () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const tray = createTray(bus);
    bus.publish("metrics.week.rolled", { week: "2026-W28", minutes_saved_total: 10 });

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    function render(): void {
      mountTray(container, tray.getSnapshot(), {
        onDismissNotification: (id) => tray.dismissNotification(id),
      });
    }
    render();

    const dismissButton = container.querySelector<HTMLButtonElement>(".op-tray__notification button");
    assert.ok(dismissButton);
    dismissButton!.click();
    render();

    assert.equal(container.querySelector(".op-tray__notification"), null);
    tray.dispose();
  } finally {
    env.cleanup();
  }
});
