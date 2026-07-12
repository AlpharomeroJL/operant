// X8 app-accessibility bar: the full first-timer path (PRD NFR-7's install,
// wizard, demo mode, teach a starter task, save it as a workflow, run it,
// schedule it; this packet's brief shorthands it as palette to teach to
// save-as-workflow to run) driven keyboard-only, no pointer events, start to
// finish, against the real shell
// (ui/src/main.ts, imported here exactly as index.html loads it) rather than
// hand-built fixtures. Uses ui/src/styles/keyboardSim.ts to simulate Tab,
// Enter, Space, and Escape the way a real browser would apply their default
// actions, which jsdom does not do on its own.
//
// This is an integration test spanning every screen this lane owns
// (palette, wizard, library, run viewer) plus ui/src/main.ts's own static
// skeleton and wiring (read-only: main.ts is outside this lane's owned
// paths, so nothing here modifies it). Lives in ui/src/__tests__/ next to
// ./palette-run-viewer.test.ts, the existing cross-module integration test
// covering the same two modules a different way.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { pressTab, pressActivate } from "../styles/keyboardSim.ts";

async function waitUntil(predicate: () => boolean, timeoutMs = 5000, intervalMs = 20): Promise<void> {
  const start = Date.now();
  while (!predicate()) {
    if (Date.now() - start > timeoutMs) throw new Error("waitUntil: timed out");
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

/** Presses Tab up to `maxSteps` times until `predicate` matches the newly focused element, keyboard-only (no direct .focus() calls standing in for navigation). */
function tabUntil(doc: Document, predicate: (el: Element) => boolean, maxSteps = 60): HTMLElement {
  for (let i = 0; i < maxSteps; i++) {
    pressTab(doc, {});
    const active = doc.activeElement;
    if (active instanceof HTMLElement && predicate(active)) return active;
  }
  throw new Error(`tabUntil: no matching element reached within ${maxSteps} Tab presses`);
}

test("keyboard-only, no pointer events: palette screen -> teach -> save as workflow -> run, to completion", async () => {
  const env = createDomEnv();
  try {
    const mainModule = await import("../main.ts");
    void mainModule; // imported for its side effect: mounting the real app into #app, same as index.html's <script> tag.

    const doc = env.document;

    // 1. Landing: the wizard opens over the Run screen (which owns the
    // command palette) for a first-timer, and moves focus into itself
    // automatically (ui/src/wizard/view.ts's focusOnSectionChange) rather
    // than leaving a keyboard user to hunt for it.
    const dialog = doc.querySelector('[role="dialog"]');
    assert.ok(dialog, "the onboarding wizard must be open for a first-timer");
    assert.ok(dialog.contains(doc.activeElement), "focus must start inside the wizard dialog");
    assert.ok(doc.querySelector(".op-palette__input"), "the command palette must already be on screen, behind the wizard");

    // 2. welcome -> setup_path.
    let active = tabUntil(doc, (el) => el.textContent === "Continue");
    pressActivate(doc, active, "Enter");
    await waitUntil(() => doc.querySelector('[role="dialog"] h2')?.textContent !== "Welcome to Operant");

    // 3. setup_path -> mic_check, via the real (non-demo) sign-in path: only
    // the real path publishes workflow.compiled at the end (guided_task's
    // demo mode does not), and this test needs the save-as-workflow step to
    // be real.
    active = tabUntil(doc, (el) => el.textContent === "Sign in with ChatGPT");
    pressActivate(doc, active, "Enter");
    await waitUntil(() => doc.querySelector('[role="dialog"] h2')?.textContent === "Let's check your microphone");

    // 4. mic_check -> guided_task (teach): Skip for now needs no microphone.
    active = tabUntil(doc, (el) => el.textContent === "Skip for now");
    pressActivate(doc, active, "Enter");
    await waitUntil(() => doc.querySelector('[role="dialog"] h2')?.textContent?.includes("Teaching") || Boolean(doc.querySelector(".op-step-list li")));

    // The guided task streams steps on a timer against the fixture invoice
    // form (ui/src/wizard/guidedTask.ts); wait for it to finish the same way
    // a person watching the screen would, then Save as workflow (compile).
    await waitUntil(() => doc.querySelector("button")?.textContent !== undefined && Boolean(findByText(doc, "Save as workflow")), 8000);
    active = tabUntil(doc, (el) => el.textContent === "Save as workflow");
    pressActivate(doc, active, "Enter");
    await waitUntil(() => doc.querySelector('[role="dialog"] h2')?.textContent === "Want this to run by itself?");

    // 5. schedule: Space selects an option (a radio, not a button), then
    // Save this schedule finishes the wizard.
    active = tabUntil(doc, (el) => el instanceof HTMLInputElement && el.type === "radio");
    pressActivate(doc, active, " ");
    await waitUntil(() => !((findByText(doc, "Save this schedule") as HTMLButtonElement | null)?.disabled ?? true));
    active = tabUntil(doc, (el) => el.textContent === "Save this schedule");
    pressActivate(doc, active, "Enter");
    await waitUntil(() => doc.querySelector('[role="dialog"]') === null || (doc.getElementById("op-wizard-backdrop") as HTMLElement)?.hidden === true);

    // 6. The wizard is done; the Run screen (with the palette) is what is
    // left. Tab across the nav to Library, where the just-compiled workflow
    // now has a card (run: the last leg of the first-timer path).
    active = tabUntil(doc, (el) => el.textContent === "Library" && el.id === "op-nav-library");
    pressActivate(doc, active, "Enter");
    assert.equal((doc.getElementById("op-screen-library") as HTMLElement).hidden, false, "activating the Library nav button must show the library screen");

    active = tabUntil(doc, (el) => el.getAttribute("data-op-focus-key") === "library-run-first-task");
    pressActivate(doc, active, "Enter");

    const card = doc.querySelector('article[aria-label="first-task"]');
    assert.ok(card, "the compiled workflow's card must be on screen");
    await waitUntil(() => !(card!.textContent ?? "").includes("Not run yet"));
    assert.ok((card!.textContent ?? "").includes("minute"), "running the workflow must update its minutes-saved figure");
  } finally {
    env.cleanup();
  }
});

function findByText(doc: Document, text: string): Element | null {
  for (const el of Array.from(doc.querySelectorAll("button"))) {
    if (el.textContent === text) return el;
  }
  return null;
}
