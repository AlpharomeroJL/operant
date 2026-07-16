// GLASS.md GL3 bar (kill-switch overlay): the overlay must be PRE-MOUNTED and
// HIDDEN, and the panic path must reveal it by a pure attribute toggle, never by
// constructing anything on trigger, so it lands inside the same sub-100ms freeze
// budget the stop meets (GLASS.md section 4, G3). These tests drive the module
// directly (main.ts's DOM glue that wires reveal() to the kill chord and the
// tray panic row is intentionally untested, the same split every other module
// here uses); they prove the pre-mount, the single-toggle reveal, and that the
// panel node is never rebuilt across a reveal.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { mountKillSwitchOverlay } from "./killOverlay.ts";

/** The same backdrop+mount shape main.ts builds in its static skeleton. */
function setup(env: ReturnType<typeof createDomEnv>): { backdrop: HTMLElement; mount: HTMLElement } {
  const backdrop = env.document.createElement("div");
  backdrop.className = "op-modal-backdrop op-kill-backdrop";
  const mount = env.document.createElement("div");
  backdrop.appendChild(mount);
  env.document.body.appendChild(backdrop);
  return { backdrop, mount };
}

test("mounts pre-built but hidden: the panel exists in the DOM while the backdrop is still hidden", () => {
  const env = createDomEnv();
  try {
    const { backdrop, mount } = setup(env);
    const overlay = mountKillSwitchOverlay(backdrop, mount);

    assert.equal(backdrop.hidden, true, "the overlay starts hidden");
    assert.equal(overlay.revealed(), false, "revealed() reflects the hidden backdrop");
    // Pre-mounted: the panic panel is already in the DOM before any trigger, so
    // the reveal never has to construct it.
    assert.ok(mount.querySelector(".op-kill-overlay"), "the panel is built at mount, while still hidden");
    assert.strictEqual(overlay.panel, mount.querySelector(".op-kill-overlay"), "the returned panel is the mounted one");
  } finally {
    env.cleanup();
  }
});

test("the panel wears the heavier-blur overlay glass plus the danger edge (GLASS.md section 4, G3 material)", () => {
  const env = createDomEnv();
  try {
    const { backdrop, mount } = setup(env);
    const overlay = mountKillSwitchOverlay(backdrop, mount);
    const panel = overlay.panel;

    assert.ok(panel.matches(".op-glass"), "it is a glass surface");
    assert.ok(panel.matches(".op-glass--overlay"), "it uses the heavier blurOverlay that severs the surface");
    assert.ok(panel.matches(".op-kill-overlay"), "it carries the danger-edge kill class");
    // Accessible name + alertdialog role so the severing surface announces itself.
    assert.equal(panel.getAttribute("role"), "alertdialog");
    assert.equal(panel.getAttribute("aria-modal"), "true");
    const titleId = panel.getAttribute("aria-labelledby");
    assert.ok(titleId && env.document.getElementById(titleId), "aria-labelledby points at the real title element");
  } finally {
    env.cleanup();
  }
});

test("the panic path reveals it by toggling the pre-mounted element's hidden attribute only (no construction)", () => {
  const env = createDomEnv();
  try {
    const { backdrop, mount } = setup(env);
    const overlay = mountKillSwitchOverlay(backdrop, mount);
    const panelBefore = overlay.panel;

    // reveal() is exactly what main.ts calls on the kill chord / tray panic.
    overlay.reveal();

    assert.equal(backdrop.hidden, false, "reveal shows the overlay");
    assert.equal(overlay.revealed(), true);
    // Same node: the reveal was a pure attribute toggle, not a rebuild, which is
    // what keeps it inside the freeze budget.
    assert.strictEqual(mount.querySelector(".op-kill-overlay"), panelBefore, "reveal must not reconstruct the panel");
  } finally {
    env.cleanup();
  }
});

test("hide() puts it back (e.g. on the core's killswitch.released echo), also by the attribute alone", () => {
  const env = createDomEnv();
  try {
    const { backdrop, mount } = setup(env);
    const overlay = mountKillSwitchOverlay(backdrop, mount);
    overlay.reveal();
    const panelWhileShown = overlay.panel;

    overlay.hide();

    assert.equal(backdrop.hidden, true, "hide re-gates the overlay");
    assert.equal(overlay.revealed(), false);
    assert.strictEqual(overlay.panel, panelWhileShown, "hide does not rebuild the panel either");
  } finally {
    env.cleanup();
  }
});
