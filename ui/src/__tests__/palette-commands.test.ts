// @advanced
// Exempt from scripts/microcopy_lint.mjs (same reason ui/src/bus/realClient.test.ts
// is): a test file, not shipped UI copy, whose assertions name wire-protocol
// vocabulary from contracts/ipc.md ("replay", "run_saved_workflow", ...).
// DOM-driven proof that the command palette drives REAL core commands
// (contracts/ipc.md section 5), the way ui/src/main.ts wires it: a real
// KeyboardEvent into the mounted palette overlay -> the view's keydown ->
// controller.commit -> the same handlePaletteCommit routing main.ts uses ->
// the CoreCommands seam. Enter on the Teach row issues start_explore (goal +
// the foreground window as context), Enter on a workflow row issues
// run_saved_workflow, and Ctrl+Enter issues dry_run; each also streams into the
// flight recorder (ui/src/runViewer) via the mocked bus. Mirrors main.ts's
// wiring here rather than importing it, the same convention
// ./palette-run-viewer.test.ts uses for the same two modules.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY, type RunStartedPayload } from "../bus/types.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { createMockCoreCommands, type CoreCommandName } from "../bus/commands.ts";
import { createPaletteController } from "../palette/state.ts";
import { createFrecencyStore } from "../palette/frecency.ts";
import { mountPalette } from "../palette/view.ts";
import { createRunViewer } from "../runViewer/state.ts";
import type { PaletteEntry } from "../palette/catalog.ts";

interface SeenCommand {
  name: CoreCommandName;
  args: Record<string, unknown>;
}

/** Same window-off-the-Document construction ./palette/accessibility.test.ts uses, so a modifier (Ctrl+Enter) KeyboardEvent type-checks against the DOM lib's Window. */
function dispatchKey(doc: Document, target: HTMLElement, key: string, opts: KeyboardEventInit = {}): void {
  const win = doc.defaultView;
  if (!win) throw new Error("dispatchKey needs a document with a defaultView (use ./testDomEnv.ts's createDomEnv)");
  target.dispatchEvent(new win.KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...opts }));
}

function setup(env: ReturnType<typeof createDomEnv>) {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  const viewer = createRunViewer(bus);
  const commands: SeenCommand[] = [];
  const started: RunStartedPayload[] = [];
  bus.subscribe("run.started", (e) => {
    if (e.topic === "run.started") started.push(e.payload);
  });
  const coreCommands = createMockCoreCommands(bus, {
    registry,
    foregroundWindow: () => "notepad.exe",
    stepDelayMs: 3,
    onCommand: (name, args) => commands.push({ name, args }),
  });

  // The palette's saved-workflow rows come from list_workflows, exactly as
  // main.ts's refreshPaletteEntries builds them.
  const workflowEntries: PaletteEntry[] = coreCommands.listWorkflows().map((workflow) => ({
    id: workflow.id,
    kind: "workflow",
    title: workflow.description || workflow.name,
    subtitle: workflow.description ? workflow.name : undefined,
    keywords: [workflow.name],
  }));

  const controller = createPaletteController({ frecency: createFrecencyStore({ storageKey: `test.cmd.${Math.random()}` }) });
  controller.setEntries(workflowEntries);

  // Cancels the teach path's canned stream so its timers do not outlive the
  // test (main.ts keeps this as its stopDemo handle).
  let teachStop: (() => void) | null = null;

  // The exact routing ui/src/main.ts's handlePaletteCommit does for these
  // kinds (its requestRun grant gate included: a workflow needing a permission
  // opens a prompt in main.ts rather than running, so only no-grant workflows
  // run straight through here).
  function requestRun(name: string): void {
    const record = registry.get(name);
    if (!record) return;
    const caps = record.manifest.capabilities;
    const needsGrant = Boolean((caps.paths && caps.paths.length) || (caps.apps && caps.apps.length) || caps.network);
    if (needsGrant) return;
    coreCommands.runSavedWorkflow(name);
  }
  function handleCommit(intent: "run" | "preview" | "details", rowId?: string): void {
    const commit = controller.commit(intent, rowId);
    if (!commit) return;
    const { row } = commit;
    if (row.kind === "workflow") {
      if (commit.intent === "run") requestRun(row.id);
      else if (commit.intent === "preview") coreCommands.dryRunWorkflow(row.id);
    } else if (row.kind === "teach") {
      teachStop = coreCommands.startExplore(row.subtitle ?? row.title);
    }
  }

  const container = env.document.createElement("div");
  env.document.body.appendChild(container);
  function render(): void {
    mountPalette(container, controller.getSnapshot(), {
      onQueryChange: (text) => controller.setQuery(text),
      onMoveSelection: (delta) => controller.moveSelection(delta),
      onCommit: (intent, rowId) => handleCommit(intent, rowId),
      onClose: () => controller.close(),
    });
  }

  function dispose(): void {
    teachStop?.();
    controller.dispose();
    viewer.dispose();
  }

  return { controller, container, commands, started, viewer, render, dispose };
}

test("Enter on the Teach row issues start_explore with the goal and the foreground window process, and the flight recorder shows the teach run", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commands, started, viewer, render, dispose } = setup(env);
    controller.open();
    controller.setQuery("archive my old screenshots"); // matches no saved workflow -> Teach row
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();
    assert.equal(controller.getSnapshot().teachRow?.kind, "teach", "an unmatched query must offer the Teach row");

    dispatchKey(env.document, input, "Enter");

    assert.equal(commands.length, 1);
    assert.equal(commands[0].name, "start_explore");
    assert.deepEqual(commands[0].args, { goal: "archive my old screenshots", window_process: "notepad.exe" });

    // The teach run streams into the flight recorder, model on and mode explore
    // (contracts/ipc.md section 5b: start_explore is the model-driven path).
    assert.equal(started[0]?.mode, RUN_MODE_EXPLORE);
    assert.equal(started[0]?.goal, "archive my old screenshots");
    const snap = viewer.getSnapshot();
    assert.equal(snap.runState, "running");
    assert.equal(snap.modelOn, true);
    assert.equal(controller.getSnapshot().open, false, "committing must close the palette");

    dispose();
  } finally {
    env.cleanup();
  }
});

test("Enter on a saved-workflow row issues run_saved_workflow and the run replays into the flight recorder", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commands, started, viewer, render, dispose } = setup(env);
    controller.open();
    controller.setQuery("photos"); // only the no-grant backup-photos workflow
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();
    assert.equal(controller.getSnapshot().rows[0]?.id, "backup-photos");

    dispatchKey(env.document, input, "Enter");

    assert.equal(commands.length, 1);
    assert.equal(commands[0].name, "run_saved_workflow");
    assert.equal(commands[0].args.path, "workflows/backup-photos.ts");

    // The saved-workflow run streams start -> complete into the recorder, mode
    // replay (offline), never the model.
    assert.equal(started[0]?.mode, RUN_MODE_REPLAY);
    assert.equal(started[0]?.workflow_name, "backup-photos");
    const snap = viewer.getSnapshot();
    assert.equal(snap.runState, "done");
    assert.equal(snap.modelOn, false, "a saved-workflow run is offline replay, never the model");

    dispose();
  } finally {
    env.cleanup();
  }
});

test("Ctrl+Enter on a saved-workflow row issues dry_run (preview), never a real run", () => {
  const env = createDomEnv();
  try {
    const { controller, container, commands, started, render, dispose } = setup(env);
    controller.open();
    controller.setQuery("photos");
    render();
    const input = container.querySelector<HTMLInputElement>(".op-palette__input")!;
    input.focus();

    dispatchKey(env.document, input, "Enter", { ctrlKey: true });

    assert.equal(commands.length, 1);
    assert.equal(commands[0].name, "dry_run");
    assert.equal(commands[0].args.path, "workflows/backup-photos.ts");
    assert.equal(started[0]?.mode, "dry", "a preview runs in dry mode, not run_saved_workflow's replay");

    dispose();
  } finally {
    env.cleanup();
  }
});
