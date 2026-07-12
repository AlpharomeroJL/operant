// @advanced
// Marked @advanced (exempt from scripts/microcopy_lint.mjs) for the same reason
// ui/src/boot/coreGate.ts is: this test asserts against the wire capability
// object (real_uia, real_input) and the recorded fixture, never user-facing UI
// copy. The human strings a person reads are in ui/src/boot/coreGateView.ts,
// which stays a scanned default-mode file.
//
// Proves the capability gate (ui/src/boot/coreGate.ts) and its screens
// (ui/src/boot/coreGateView.ts) enforce the structural guarantee in
// contracts/ipc.md section 3: a core that cannot automate is blocked from every
// real-work surface with each missing capability named, a failed handshake
// shows an error state and never a silent demo, and only a fully capable core
// is cleared to build the real UI. The blocked case is driven by the real
// recorded (mock) capability object, not a hand-written one.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createDomEnv } from "../styles/testDomEnv.ts";
import {
  canAutomate,
  handshakeCore,
  missingCapabilities,
  type CoreCapabilities,
} from "./coreGate.ts";
import { mountDemoBanner, renderBlockingScreen, renderErrorScreen } from "./coreGateView.ts";

const here = dirname(fileURLToPath(import.meta.url));
const fixturePath = join(here, "..", "..", "..", "contracts", "fixtures", "ipc", "session-explore-compile-replay-undo.jsonl");

/** The real recorded get_capabilities result from the frozen session fixture. */
function fixtureCapabilities(): CoreCapabilities {
  const lines = readFileSync(fixturePath, "utf8").split(/\r?\n/).filter(Boolean);
  for (const line of lines) {
    const frame = JSON.parse(line) as { t: string; id?: string; result?: CoreCapabilities };
    if (frame.t === "res" && frame.id === "cmd-1" && frame.result) return frame.result;
  }
  throw new Error("no get_capabilities result in the session fixture");
}

const CAPABLE: CoreCapabilities = {
  real_uia: true,
  real_input: true,
  real_vision: true,
  mock_planner_only: false,
  transport_kind: "stdio",
  version: "1.0.0",
  git_sha: "abc123",
};

test("the recorded fixture is the blocking case: neither real_uia nor real_input", () => {
  const caps = fixtureCapabilities();
  assert.equal(caps.real_uia, false);
  assert.equal(caps.real_input, false);
  assert.equal(canAutomate(caps), false);
});

test("canAutomate requires BOTH real_uia and real_input", () => {
  assert.equal(canAutomate(CAPABLE), true);
  assert.equal(canAutomate({ ...CAPABLE, real_uia: false }), false);
  assert.equal(canAutomate({ ...CAPABLE, real_input: false }), false);
});

test("missingCapabilities names each false automation field, in contract order", () => {
  assert.deepEqual(missingCapabilities(CAPABLE), []);
  assert.deepEqual(missingCapabilities({ ...CAPABLE, real_input: false }), [{ field: "real_input" }]);
  assert.deepEqual(missingCapabilities(fixtureCapabilities()), [{ field: "real_uia" }, { field: "real_input" }]);
});

test("handshakeCore clears a fully capable core to build the real UI", async () => {
  const connection = await handshakeCore({
    ready: () => Promise.resolve(),
    capabilities: () => Promise.resolve(CAPABLE),
  });
  assert.equal(connection.kind, "real");
});

test("handshakeCore blocks a core that cannot automate, listing what is missing", async () => {
  const connection = await handshakeCore({
    ready: () => Promise.resolve(),
    capabilities: () => Promise.resolve(fixtureCapabilities()),
  });
  assert.equal(connection.kind, "blocked");
  if (connection.kind === "blocked") {
    assert.deepEqual(connection.missing, [{ field: "real_uia" }, { field: "real_input" }]);
  }
});

test("handshakeCore surfaces a failed connection as an error, never as demo or real", async () => {
  const onReadyThrow = await handshakeCore({
    ready: () => Promise.reject(new Error("core_ready timed out")),
    capabilities: () => Promise.resolve(CAPABLE),
  });
  assert.equal(onReadyThrow.kind, "error");
  if (onReadyThrow.kind === "error") assert.match(onReadyThrow.message, /timed out/);

  const onCapsThrow = await handshakeCore({
    ready: () => Promise.resolve(),
    capabilities: () => Promise.reject(new Error("pipe closed")),
  });
  assert.equal(onCapsThrow.kind, "error", "a mid-handshake failure is an error, not a fallback");
});

test("renderBlockingScreen names each missing capability and leaves no working UI", () => {
  const env = createDomEnv();
  try {
    const root = env.document.getElementById("app")!;
    // Prior real-work UI that must not survive behind the blocking screen.
    root.innerHTML = `<nav class="op-nav" id="op-nav"></nav><main class="op-main"></main>`;

    renderBlockingScreen(root, [{ field: "real_uia" }, { field: "real_input" }]);

    // The working UI is gone.
    assert.equal(root.querySelector(".op-nav"), null, "no navigation may remain");
    assert.equal(root.querySelector(".op-main"), null, "no real-work surface may remain");

    const screen = root.querySelector(".op-boot-screen--blocked");
    assert.ok(screen, "a blocking screen must be shown");
    assert.equal(screen!.getAttribute("role"), "alert");

    // Each missing capability is named by its contract field.
    const items = Array.from(root.querySelectorAll(".op-boot-screen__list li"));
    assert.equal(items.length, 2);
    const fields = items.map((li) => (li as HTMLElement).dataset.capability);
    assert.deepEqual(fields, ["real_uia", "real_input"]);
    const text = root.textContent ?? "";
    assert.match(text, /real_uia/);
    assert.match(text, /real_input/);
  } finally {
    env.cleanup();
  }
});

test("renderErrorScreen shows an error state with detail, never canned data", () => {
  const env = createDomEnv();
  try {
    const root = env.document.getElementById("app")!;
    root.innerHTML = `<nav class="op-nav"></nav>`;

    renderErrorScreen(root, "pipe closed");

    assert.equal(root.querySelector(".op-nav"), null);
    const screen = root.querySelector(".op-boot-screen--error");
    assert.ok(screen, "an error screen must be shown");
    assert.equal(screen!.getAttribute("role"), "alert");
    assert.match(root.textContent ?? "", /pipe closed/, "the technical detail is surfaced");
    // No run rows, no demo banner: an error is not a demo.
    assert.equal(root.querySelector(".op-demo-banner"), null);
  } finally {
    env.cleanup();
  }
});

test("mountDemoBanner labels canned mode honestly, once, without joining the tab order", () => {
  const env = createDomEnv();
  try {
    const root = env.document.getElementById("app")!;
    root.innerHTML = `<div class="op-app"><header></header></div>`;

    const banner = mountDemoBanner(root);
    assert.equal(banner.textContent, "Demo: canned example, not your computer");
    assert.equal(banner.getAttribute("role"), "status");
    assert.equal(banner.getAttribute("tabindex"), null, "the banner must not be focusable");

    // First child of the app shell, above the header.
    const app = root.querySelector(".op-app")!;
    assert.equal(app.firstElementChild, banner);

    // Idempotent: a remount never stacks a second banner.
    mountDemoBanner(root);
    assert.equal(root.querySelectorAll(".op-demo-banner").length, 1);
  } finally {
    env.cleanup();
  }
});
