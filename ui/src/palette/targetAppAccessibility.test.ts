// X8 app-accessibility bar for the target-app picker overlay: an axe-core scan
// of its real states (loading, the window list, the empty fallback), plus the
// keyboard-only behavior axe cannot check statically. Same discipline as
// ./accessibility.test.ts for the palette: every keyboard test drives the real
// mounted DOM with dispatched KeyboardEvents (arrow keys + Enter + Escape) and
// never constructs a MouseEvent, except the one clearly-labelled pointer test
// at the end.

import { test } from "node:test";
import assert from "node:assert/strict";
import axe from "axe-core";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createTargetAppPicker, type TargetAppConfirm, type TargetWindow } from "./targetApp.ts";
import { mountTargetAppPicker } from "./targetAppView.ts";

const WINDOWS: TargetWindow[] = [
  { process: "chrome.exe", title: "Quarterly report - Chrome", id: "win-1" },
  { process: "notepad.exe", title: "notes.txt - Notepad", id: "win-2" },
  { process: "excel.exe", title: "Budget - Excel", id: "win-3" },
];

function setup(env: ReturnType<typeof createDomEnv>) {
  const picker = createTargetAppPicker();
  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  const confirms: TargetAppConfirm[] = [];
  let cancels = 0;

  function render(): void {
    mountTargetAppPicker(container, picker.getSnapshot(), {
      onMoveSelection: (delta) => picker.moveSelection(delta),
      onConfirm: (rowId) => {
        const result = picker.confirm(rowId);
        if (result) confirms.push(result);
      },
      onCancel: () => {
        cancels++;
        picker.close();
      },
    });
  }

  return { picker, container, confirms, render, cancels: () => cancels };
}

async function assertNoViolations(root: Element, label: string): Promise<void> {
  const results = await axe.run(root, { resultTypes: ["violations"] });
  assert.deepEqual(
    results.violations.map((v) => ({ id: v.id, help: v.help, nodes: v.nodes.map((n) => n.target) })),
    [],
    `axe-core violations on ${label}`,
  );
}

/** Same window-off-the-Document construction ./accessibility.test.ts uses so an ArrowDown/Enter/Escape KeyboardEvent type-checks against the DOM lib's Window. */
function dispatchKey(doc: Document, target: HTMLElement, key: string, opts: KeyboardEventInit = {}): void {
  const win = doc.defaultView;
  if (!win) throw new Error("dispatchKey needs a document with a defaultView (use ./testDomEnv.ts's createDomEnv)");
  target.dispatchEvent(new win.KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...opts }));
}

test("loading state (open, window list not yet resolved): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const { picker, container, render } = setup(env);
    picker.open("copy the invoice total");
    render();
    await assertNoViolations(container, "target-app picker loading");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("window list state (a listbox with a front-app row plus each window): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const { picker, container, render } = setup(env);
    picker.open("copy the invoice total");
    picker.setWindows(WINDOWS);
    render();
    await assertNoViolations(container, "target-app picker window list");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("empty state (no other apps open): no axe violations", async () => {
  const env = createDomEnv();
  try {
    const { picker, container, render } = setup(env);
    picker.open("copy the invoice total");
    picker.setWindows([]);
    render();
    await assertNoViolations(container, "target-app picker empty");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("opening the window list moves keyboard focus onto the listbox on its own, with no manual .focus() from the caller", () => {
  const env = createDomEnv();
  try {
    const { picker, container, render } = setup(env);
    picker.open("copy the invoice total");
    picker.setWindows(WINDOWS);
    render();

    const listbox = container.querySelector<HTMLElement>('[role="listbox"]');
    assert.ok(listbox);
    assert.equal(env.document.activeElement, listbox, "the listbox must take focus so arrow keys work without a click first");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only, no pointer events: ArrowDown twice then Enter confirms the moved-to window, not the front-app default", () => {
  const env = createDomEnv();
  try {
    const { picker, container, confirms, render } = setup(env);
    picker.open("copy the invoice total");
    picker.setWindows(WINDOWS);
    render();

    let listbox = container.querySelector<HTMLElement>('[role="listbox"]')!;
    dispatchKey(env.document, listbox, "ArrowDown"); // front-app -> win-1 (chrome)
    render();
    listbox = container.querySelector<HTMLElement>('[role="listbox"]')!;
    dispatchKey(env.document, listbox, "ArrowDown"); // win-1 -> win-2 (notepad)
    render();
    listbox = container.querySelector<HTMLElement>('[role="listbox"]')!;
    dispatchKey(env.document, listbox, "Enter");

    assert.equal(confirms.length, 1);
    assert.equal(confirms[0].windowProcess, "notepad.exe", "Enter must confirm the arrowed-to window");
    assert.notEqual(confirms[0].windowProcess, WINDOWS[0].process, "and not silently fall back to the front-app default");
    assert.equal(confirms[0].goal, "copy the invoice total", "the goal rides through to the teach run");
    assert.equal(picker.getSnapshot().open, false, "confirming must close the picker");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Enter with the default selection confirms the front app (windows[0])", () => {
  const env = createDomEnv();
  try {
    const { picker, container, confirms, render } = setup(env);
    picker.open("copy the invoice total");
    picker.setWindows(WINDOWS);
    render();

    const listbox = container.querySelector<HTMLElement>('[role="listbox"]')!;
    dispatchKey(env.document, listbox, "Enter");

    assert.equal(confirms.length, 1);
    assert.equal(confirms[0].windowProcess, WINDOWS[0].process, "the pre-selected default targets the app the person was last in");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Escape cancels the picker", () => {
  const env = createDomEnv();
  try {
    const { picker, container, confirms, render, cancels } = setup(env);
    picker.open("copy the invoice total");
    picker.setWindows(WINDOWS);
    render();

    const listbox = container.querySelector<HTMLElement>('[role="listbox"]')!;
    dispatchKey(env.document, listbox, "Escape");

    assert.equal(cancels(), 1, "Escape must trigger the cancel callback");
    assert.equal(confirms.length, 0, "Escape must not start a teach run");
    assert.equal(picker.getSnapshot().open, false, "the picker closes on cancel");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("keyboard-only: Escape while still loading also cancels (the dialog itself holds focus)", () => {
  const env = createDomEnv();
  try {
    const { picker, container, render, cancels } = setup(env);
    picker.open("copy the invoice total");
    render();

    const dialog = container.querySelector<HTMLElement>('[role="dialog"]')!;
    assert.equal(env.document.activeElement, dialog, "while loading, the dialog takes focus so Escape still routes");
    dispatchKey(env.document, dialog, "Escape");
    assert.equal(cancels(), 1);
    picker.dispose();
  } finally {
    env.cleanup();
  }
});

test("pointer: clicking a window row confirms that exact window", () => {
  const env = createDomEnv();
  try {
    const { picker, container, confirms, render } = setup(env);
    picker.open("copy the invoice total");
    picker.setWindows(WINDOWS);
    render();

    const excelRow = container.querySelector<HTMLElement>("#op-target-app-row-win-3");
    assert.ok(excelRow, "each window row must be clickable");
    excelRow!.dispatchEvent(new env.window.MouseEvent("click", { bubbles: true }));

    assert.equal(confirms.length, 1);
    assert.equal(confirms[0].windowProcess, "excel.exe");
    picker.dispose();
  } finally {
    env.cleanup();
  }
});
