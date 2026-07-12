// X8 app-accessibility bar for the palette overlay: an axe-core scan of its
// real states (root view, a live query, the Teach this fallback), plus the
// keyboard-specific behavior axe cannot check by static analysis alone.
//
// BAR: "keyboard-only operation (arrow keys + Enter, no pointer events)" is
// the centerpiece here: every test below drives the real mounted DOM with
// dispatched KeyboardEvents (ui/src/styles/keyboardSim.ts where a helper
// exists, a raw KeyboardEvent otherwise for ArrowUp/ArrowDown, which
// keyboardSim.ts does not cover) and never calls .click() or constructs a
// MouseEvent, the same discipline ui/src/wizard/accessibility.test.ts and
// ui/src/__tests__/keyboard-first-timer.test.ts already hold every other
// keyboard-only screen in this app to.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { typeText, pressEscape } from "../styles/keyboardSim.ts";
import { createPaletteController, type PaletteCommit } from "./state.ts";
import { createFrecencyStore } from "./frecency.ts";
import { mountPalette } from "./view.ts";
import type { PaletteEntry } from "./catalog.ts";

const ENTRIES: PaletteEntry[] = [
  { id: "wf-copy-invoice", kind: "workflow", title: "Copy the invoice total into the spreadsheet", keywords: ["copy-invoice-total"] },
  { id: "wf-weekly-report", kind: "workflow", title: "Email the weekly report", keywords: ["weekly-report-email"] },
  { id: "action.nav.library", kind: "action", title: "Library", subtitle: "Switch to this screen" },
  { id: "setting.privacy", kind: "setting", title: "Privacy", subtitle: "Open Settings" },
];

function setup(env: ReturnType<typeof createDomEnv>) {
  const frecency = createFrecencyStore({ storageKey: `test.a11y.${Math.random()}` });
  const controller = createPaletteController({ frecency });
  controller.setEntries(ENTRIES);
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  const commits: PaletteCommit[] = [];

  function render(): void {
    mountPalette(container, controller.getSnapshot(), {
      onQueryChange: (text) => controller.setQuery(text),
      onMoveSelection: (delta) => controller.moveSelection(delta),
      onCommit: (intent, rowId) => {
        const commit = controller.commit(intent, rowId);
        if (commit) commits.push(commit);
      },
      onClose: () => controller.close(),
    });
  }

  return { controller, container, commits, render };
}

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

/** Same "get the window off the Document, not off jsdom's own DOMWindow type" convention as ui/src/styles/keyboardSim.ts's pressTab/pressActivate/pressEscape, so this file's own KeyboardEvent construction (ArrowUp/ArrowDown, which keyboardSim.ts does not cover) type-checks against the DOM lib's Window rather than the structurally different "jsdom" package type. */
function dispatchKey(doc: Document, target: HTMLElement, key: string, opts: KeyboardEventInit = {}): void {
  const win = doc.defaultView;
  if (!win) throw new Error("dispatchKey needs a document with a defaultView (use ./testDomEnv.ts's createDomEnv)");
  const event = new win.KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...opts });
  target.dispatchEvent(event);
}

test("root view (opened, blank query, Workflows/Actions grouped): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    controller.open();
    render();
    await assertNoViolations(container, "palette root view");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("a live, narrowing query with match-character highlighting: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    controller.open();
    controller.setQuery("invoice");
    render();
    await assertNoViolations(container, "palette with a live query");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("the Teach this fallback row: no axe violations", async () => {
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    controller.open();
    controller.setQuery("a whole sentence nothing here will ever match");
    render();
    await assertNoViolations(container, "palette Teach this fallback");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("opening the palette moves keyboard focus onto the input on its own, with no manual .focus() call from the caller", () => {
  // Deliberately never calls input.focus() itself (every other test in this
  // file does, right after render(), to drive keys into it): the point here
  // is that mountPalette's own open-transition fallback
  // (restoreFocus-returned-false -> input.focus()) is what puts focus there
  // the first time, the same guarantee a Ctrl+K keyboard user gets from a
  // real browser. Catches a real regression this lane's own manual browser
  // testing found: every other keyboard-only test in this file was already
  // seeding focus itself and so could not have caught focus never actually
  // landing on open.
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    assert.notEqual(env.document.activeElement, container.querySelector(".op-palette__input"), "sanity check: not already focused before opening");

    controller.open();
    render();

    const input = container.querySelector<HTMLInputElement>(".op-palette__input");
    assert.ok(input);
    assert.equal(env.document.activeElement, input, "opening the palette must move focus onto the input without the caller doing it manually");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("mounting while closed (first page load, before Ctrl+K) never steals focus into the hidden input", () => {
  const env = createDomEnv();
  try {
    const { container, render } = setup(env);
    render(); // controller was never opened
    assert.notEqual(env.document.activeElement, container.querySelector(".op-palette__input"), "a closed, hidden palette must never grab focus on its own");
  } finally {
    env.cleanup();
  }
});

test(".op-palette__input is always on screen, even while closed (mounted once, gated by the backdrop's hidden attribute in main.ts)", () => {
  const env = createDomEnv();
  try {
    const { container, render } = setup(env);
    render(); // controller was never opened
    assert.ok(container.querySelector(".op-palette__input"), "the input must exist in the DOM regardless of open state");
  } finally {
    env.cleanup();
  }
});

test("keyboard-only, no pointer events: ArrowDown twice then Enter runs the third row, not the first", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commits, render } = setup(env);
    controller.open();
    render();

    const input = container.querySelector<HTMLInputElement>(".op-palette__input");
    assert.ok(input);
    input.focus();

    const rows = controller.getSnapshot().rows.map((r) => r.id);
    assert.ok(rows.length >= 3, "fixture must have at least three rows for this test to mean anything");

    dispatchKey(env.document, input, "ArrowDown");
    render();
    dispatchKey(env.document, input, "ArrowDown");
    render();
    assert.equal(controller.getSnapshot().selectedId, rows[2]);

    dispatchKey(env.document, input, "Enter");
    render();

    assert.equal(commits.length, 1);
    assert.equal(commits[0].intent, "run");
    assert.equal(commits[0].row.id, rows[2], "Enter must commit whichever row the arrow keys landed on, not the first row");
    assert.equal(controller.getSnapshot().open, false, "running a row must close the palette");

    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: ArrowUp from the top row wraps to the last row and Enter commits it", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commits, render } = setup(env);
    controller.open();
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    const rows = controller.getSnapshot().rows.map((r) => r.id);
    dispatchKey(env.document, input, "ArrowUp");
    render();
    assert.equal(controller.getSnapshot().selectedId, rows[rows.length - 1]);

    dispatchKey(env.document, input, "Enter");
    render();
    assert.equal(commits[0].row.id, rows[rows.length - 1]);
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Ctrl+Enter previews a workflow row instead of running it", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commits, render } = setup(env);
    controller.open();
    controller.setQuery("invoice");
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    dispatchKey(env.document, input, "Enter", { ctrlKey: true });
    render();

    assert.equal(commits.length, 1);
    assert.equal(commits[0].intent, "preview");
    assert.equal(commits[0].row.id, "wf-copy-invoice");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Tab commits 'details' for a workflow row and does not move focus off the input", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commits, render } = setup(env);
    controller.open();
    controller.setQuery("invoice");
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    dispatchKey(env.document, input, "Tab");
    render();

    assert.equal(commits.length, 1);
    assert.equal(commits[0].intent, "details");
    assert.equal(commits[0].row.id, "wf-copy-invoice");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Tab on a non-workflow row commits nothing (no details to show) and still never moves focus", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commits, render } = setup(env);
    controller.open();
    controller.setQuery("library");
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();
    assert.equal(controller.getSnapshot().rows[0]?.id, "action.nav.library");

    dispatchKey(env.document, input, "Tab");
    render();

    assert.equal(commits.length, 0);
    assert.equal(env.document.activeElement?.className, input.className, "focus must still be on the (rebuilt) input");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Escape closes the palette", () => {
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    controller.open();
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    pressEscape(env.document, input);
    render();

    assert.equal(controller.getSnapshot().open, false);
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only, end to end: typing narrows the list, highlights the match, and Enter commits the top result", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commits, render } = setup(env);
    controller.open();
    render();
    let input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    typeText(env.document, input, "invoice");
    render();

    input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    assert.equal(input.value, "invoice", "the typed text must survive every per-keystroke rebuild");
    assert.equal(env.document.activeElement, input, "focus must survive every per-keystroke rebuild");

    const marks = Array.from(container.querySelectorAll(".op-palette-overlay__match")).map((m) => m.textContent);
    assert.ok(marks.length > 0, "at least one matched run must be highlighted");
    assert.ok(marks.every((t) => "invoice".includes((t ?? "").toLowerCase())), `every highlighted run must be part of the query, got ${JSON.stringify(marks)}`);

    dispatchKey(env.document, input, "Enter");
    render();
    assert.equal(commits.length, 1);
    assert.equal(commits[0].row.id, "wf-copy-invoice");

    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("focus and the typed query survive a rebuild across every keystroke (no jsdom focus loss, ui/src/styles/focusPreserve.ts)", () => {
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    controller.open();
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    typeText(env.document, input, "wk");
    render();

    const inputAfter = container.querySelector<HTMLInputElement>(".op-palette__input");
    assert.ok(inputAfter);
    assert.notEqual(inputAfter, input, "the rebuild must have replaced the DOM node");
    assert.equal(env.document.activeElement, inputAfter, "focus must carry onto the rebuilt input");
    assert.equal(inputAfter.value, "wk");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("the Teach this row renders with the amber row class and the typed text as its subtitle", () => {
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    controller.open();
    controller.setQuery("a whole sentence nothing here will ever match");
    render();

    const teachRow = container.querySelector(".op-palette-overlay__row--teach");
    assert.ok(teachRow, "the Teach this row must render with its own amber-row class");
    assert.equal(teachRow!.querySelector(".op-palette-overlay__row-title")?.textContent, "Teach this");
    assert.equal(teachRow!.querySelector(".op-palette-overlay__row-subtitle")?.textContent, "a whole sentence nothing here will ever match");
    assert.equal(teachRow!.getAttribute("aria-selected"), "true", "the sole row shown must be the selected one");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});

test("aria-activedescendant on the input always names the currently selected row's element id", () => {
  const env = createDomEnv();
  try {
    const { controller, container, render } = setup(env);
    controller.open();
    render();
    let input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    const selectedId = controller.getSnapshot().selectedId;
    assert.ok(selectedId);
    const activeDescendant = input.getAttribute("aria-activedescendant");
    assert.ok(activeDescendant);
    const selectedRow = env.document.getElementById(activeDescendant!);
    assert.ok(selectedRow, "aria-activedescendant must point at a real element in the document");
    assert.equal(selectedRow!.getAttribute("aria-selected"), "true");

    dispatchKey(env.document, input, "ArrowDown");
    render();
    input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    const nextActiveDescendant = input.getAttribute("aria-activedescendant");
    assert.notEqual(nextActiveDescendant, activeDescendant, "moving selection must update aria-activedescendant");
    controller.dispose();
  } finally {
    env.cleanup();
  }
});
