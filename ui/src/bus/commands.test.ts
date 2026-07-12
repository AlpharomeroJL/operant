// Tests the shell-to-core command seam (./commands.ts): the palette issues the
// contract commands (contracts/ipc.md section 5) through it, and in dev/Demo
// the mock drives the same canned bus stream the shell rendered against before.
// Asserts each command's name and args (notably that start_explore carries the
// goal AND the foreground window_process), and that the dev fallback publishes
// the exact run.* events Library/tray/run-viewer already react to. Pure logic,
// no DOM, same split as ./mockClient.test.ts.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "./mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY, type BusEvent } from "./types.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { createMockCoreCommands, DEV_FOREGROUND_WINDOW, type CoreCommandName } from "./commands.ts";

interface SeenCommand {
  name: CoreCommandName;
  args: Record<string, unknown>;
}

test("startExplore issues start_explore carrying the goal and the foreground window process, and streams a teach run", () => {
  const bus = createMockBusClient();
  const commands: SeenCommand[] = [];
  const started: BusEvent[] = [];
  bus.subscribe("run.started", (e) => started.push(e));

  const core = createMockCoreCommands(bus, {
    foregroundWindow: () => "notepad.exe",
    onCommand: (name, args) => commands.push({ name, args }),
    stepDelayMs: 2,
  });

  const stop = core.startExplore("  Find last month's invoices  ");
  assert.ok(stop, "a non-blank goal must start a run");

  assert.equal(commands.length, 1);
  assert.equal(commands[0].name, "start_explore");
  assert.deepEqual(commands[0].args, { goal: "Find last month's invoices", window_process: "notepad.exe" });

  // dev/Demo fallback: the canned teach run streams so the flight recorder fills.
  assert.equal(started.length, 1);
  if (started[0].topic === "run.started") {
    assert.equal(started[0].payload.goal, "Find last month's invoices");
    assert.equal(started[0].payload.mode, RUN_MODE_EXPLORE);
  } else {
    assert.fail("expected a run.started event");
  }

  stop?.();
});

test("a blank goal issues no command and starts nothing", () => {
  const bus = createMockBusClient();
  const commands: CoreCommandName[] = [];
  const topics: string[] = [];
  bus.subscribe("*", (e) => topics.push(e.topic));

  const core = createMockCoreCommands(bus, { onCommand: (name) => commands.push(name) });
  const stop = core.startExplore("   \t\n ");

  assert.equal(stop, null);
  assert.equal(commands.length, 0);
  assert.equal(topics.length, 0);
});

test("start_explore defaults window_process to the dev foreground stub when no provider is injected", () => {
  const bus = createMockBusClient();
  let seen: Record<string, unknown> | null = null;
  const core = createMockCoreCommands(bus, {
    onCommand: (name, args) => {
      if (name === "start_explore") seen = args;
    },
    stepDelayMs: 1,
  });

  const stop = core.startExplore("do a thing");
  assert.ok(seen);
  assert.equal((seen as Record<string, unknown>).window_process, DEV_FOREGROUND_WINDOW);
  stop?.();
});

test("runSavedWorkflow issues run_saved_workflow and replays the saved workflow on the bus (mode replay, never explore)", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  const commands: SeenCommand[] = [];
  const topics: string[] = [];
  const started: BusEvent[] = [];
  bus.subscribe("*", (e) => {
    topics.push(e.topic);
    if (e.topic === "run.started") started.push(e);
  });

  const core = createMockCoreCommands(bus, { registry, onCommand: (name, args) => commands.push({ name, args }) });
  core.runSavedWorkflow("copy-invoice-total");

  assert.equal(commands.length, 1);
  assert.equal(commands[0].name, "run_saved_workflow");
  assert.equal(commands[0].args.path, "workflows/copy-invoice-total.ts");

  // Byte-identical to library.run's old inline pair, so Library/tray bookkeeping is unchanged.
  assert.deepEqual(topics, ["run.started", "run.completed"]);
  if (started[0].topic === "run.started") {
    assert.equal(started[0].payload.mode, RUN_MODE_REPLAY);
    assert.equal(started[0].payload.workflow_name, "copy-invoice-total");
  } else {
    assert.fail("expected a run.started event");
  }
});

test("dryRunWorkflow issues dry_run and previews the saved workflow on the bus (mode dry)", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  const commands: SeenCommand[] = [];
  const topics: string[] = [];
  const started: BusEvent[] = [];
  bus.subscribe("*", (e) => {
    topics.push(e.topic);
    if (e.topic === "run.started") started.push(e);
  });

  const core = createMockCoreCommands(bus, { registry, onCommand: (name, args) => commands.push({ name, args }) });
  core.dryRunWorkflow("copy-invoice-total");

  assert.equal(commands.length, 1);
  assert.equal(commands[0].name, "dry_run");
  assert.equal(commands[0].args.path, "workflows/copy-invoice-total.ts");

  assert.deepEqual(topics, ["run.started", "run.completed"]);
  if (started[0].topic === "run.started") {
    assert.equal(started[0].payload.mode, "dry");
  } else {
    assert.fail("expected a run.started event");
  }
});

test("an unknown workflow name issues no command and starts nothing", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  const commands: CoreCommandName[] = [];
  const topics: string[] = [];
  bus.subscribe("*", (e) => topics.push(e.topic));

  const core = createMockCoreCommands(bus, { registry, onCommand: (name) => commands.push(name) });
  core.runSavedWorkflow("does-not-exist");
  core.dryRunWorkflow("does-not-exist");

  assert.equal(commands.length, 0);
  assert.equal(topics.length, 0);
});

test("listWorkflows returns one summary per saved workflow, and is empty with no registry", () => {
  const registry = createMockRegistry();
  const core = createMockCoreCommands(createMockBusClient(), { registry });

  const list = core.listWorkflows();
  assert.ok(list.length >= 3, "the seeded registry has at least three workflows");
  const invoice = list.find((w) => w.id === "copy-invoice-total");
  assert.deepEqual(invoice, {
    id: "copy-invoice-total",
    name: "copy-invoice-total",
    version: "1.0.0",
    description: "Copy the invoice total into the spreadsheet",
  });

  const noRegistry = createMockCoreCommands(createMockBusClient(), {});
  assert.deepEqual(noRegistry.listWorkflows(), [], "with no registry there are no saved workflows to list");
});
