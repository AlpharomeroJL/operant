// @advanced
// Exempt from scripts/microcopy_lint.mjs (same reason ui/src/bus/realSeams.ts and
// ui/src/bus/realClient.test.ts are): asserts against wire vocabulary (the
// contracts/ipc.md section 5 command names), never user-facing UI copy.
//
// Proves the ONE real command layer (ui/src/boot/realSeams.ts) routes every
// seam through B2's core_call under the CONTRACT command names, so a real build
// speaks exactly what B1's serve loop accepts. The load-bearing assertion is the
// name reconciliation: running a saved workflow issues `start_replay`, never the
// UI's old `run_saved_workflow`.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  makeCoreCall,
  createRealCommandClient,
  createRealCoreCommands,
  createRealTeachClient,
  createRealUndoCommands,
  createRealPanicClient,
  createRealScheduler,
} from "./realSeams.ts";
import { createMockRegistry } from "../library/mockRegistry.ts";
import { isNotImplemented, TRIGGER_KIND_CRON } from "../scheduler/commands.ts";

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

/** A spy raw invoke that records every call and answers with `answer`. */
function spyInvoke(answer: (cmd: string) => Promise<unknown> = async () => undefined) {
  const calls: Call[] = [];
  const invoke = async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
    calls.push({ cmd, args: args ?? {} });
    return answer(cmd);
  };
  return { invoke, calls };
}

/** Unwrap the { cmd, args } a core_call invocation carries, so tests assert the inner contract command. */
function inner(call: Call): { cmd: string; args: Record<string, unknown> } {
  assert.equal(call.cmd, "core_call", "every command must ride core_call");
  const payload = call.args as { cmd: string; args: Record<string, unknown> };
  return { cmd: payload.cmd, args: payload.args };
}

test("makeCoreCall wraps every command as core_call { cmd, args }", async () => {
  const spy = spyInvoke();
  const coreCall = makeCoreCall(spy.invoke);
  await coreCall("get_settings", { a: 1 });
  await coreCall("list_runs");
  assert.deepEqual(inner(spy.calls[0]), { cmd: "get_settings", args: { a: 1 } });
  assert.deepEqual(inner(spy.calls[1]), { cmd: "list_runs", args: {} });
});

test("CommandClient.request resolves ok:true on success and ok:false (never throws) on a core no", async () => {
  const okSpy = spyInvoke(async () => [{ id: "w1" }]);
  const okClient = createRealCommandClient(makeCoreCall(okSpy.invoke));
  const ok = await okClient.request("list_workflows");
  assert.deepEqual(ok, { ok: true, result: [{ id: "w1" }] });
  assert.deepEqual(inner(okSpy.calls[0]), { cmd: "list_workflows", args: {} });

  const noSpy = spyInvoke(async () => {
    throw { code: "not_implemented", message: "unwired", retryable: false };
  });
  const noClient = createRealCommandClient(makeCoreCall(noSpy.invoke));
  const no = await noClient.request("upsert_trigger", { x: 1 });
  assert.equal(no.ok, false);
  if (!no.ok) assert.equal(no.error.code, "not_implemented");
});

test("CoreCommands issues the CONTRACT command names: run a saved workflow -> start_replay (not run_saved_workflow)", () => {
  const spy = spyInvoke();
  const registry = createMockRegistry();
  const name = registry.list()[0].manifest.name;
  const path = registry.get(name)!.manifest.dsl.path;
  const core = createRealCoreCommands(makeCoreCall(spy.invoke), { registry });

  // ADR 0003, A1: window_process is the process the target-app picker resolved,
  // passed in by main.ts, not a foreground-window guess.
  core.startExplore("copy the invoice total", "notepad.exe");
  core.dryRunWorkflow(name);
  core.runSavedWorkflow(name);

  assert.deepEqual(inner(spy.calls[0]), { cmd: "start_explore", args: { goal: "copy the invoice total", window_process: "notepad.exe" } });
  assert.deepEqual(inner(spy.calls[1]), { cmd: "dry_run", args: { path } });
  // THE NAME FIX: the UI's run_saved_workflow maps to the frozen contract start_replay.
  assert.deepEqual(inner(spy.calls[2]), { cmd: "start_replay", args: { path } });
  assert.ok(!spy.calls.some((c) => inner(c).cmd === "run_saved_workflow"), "run_saved_workflow must never reach the wire");

  // listWorkflows reads the shared registry synchronously, issuing no command.
  const before = spy.calls.length;
  const rows = core.listWorkflows();
  assert.equal(spy.calls.length, before, "listWorkflows reads the registry, it does not call the core");
  assert.equal(rows[0].id, name);

  // A blank goal starts nothing.
  core.startExplore("   ", "notepad.exe");
  assert.equal(spy.calls.length, before, "a blank goal issues no start_explore");
});

test("CoreCommands.listWindows issues list_windows and unwraps the core's windows array", async () => {
  const windows = [
    { process: "chrome.exe", title: "Quarterly report - Chrome", id: "win-1" },
    { process: "notepad.exe", title: "notes.txt - Notepad", id: "win-2" },
  ];
  const spy = spyInvoke(async (cmd) => (cmd === "core_call" ? { windows } : undefined));
  const core = createRealCoreCommands(makeCoreCall(spy.invoke), { registry: createMockRegistry() });

  const result = await core.listWindows();
  assert.deepEqual(inner(spy.calls[0]), { cmd: "list_windows", args: {} });
  assert.deepEqual(result, windows);
});

test("TeachClient issues start_explore and compile_run", () => {
  const spy = spyInvoke();
  const teach = createRealTeachClient(makeCoreCall(spy.invoke));
  teach.startExplore({ goal: "draft an email", windowProcess: "outlook.exe" });
  teach.compileRun("run_7");
  assert.deepEqual(inner(spy.calls[0]), { cmd: "start_explore", args: { goal: "draft an email", window_process: "outlook.exe" } });
  assert.deepEqual(inner(spy.calls[1]), { cmd: "compile_run", args: { run_id: "run_7" } });
});

test("UndoCommands issues preview_undo and undo_run", () => {
  const spy = spyInvoke();
  const undo = createRealUndoCommands(makeCoreCall(spy.invoke));
  undo.previewUndo("run_3");
  undo.undoRun("run_3");
  assert.deepEqual(inner(spy.calls[0]), { cmd: "preview_undo", args: { run_id: "run_3" } });
  assert.deepEqual(inner(spy.calls[1]), { cmd: "undo_run", args: { run_id: "run_3" } });
});

test("PanicClient.kill fires BOTH the core kill command (path 1) and core_kill (path 2)", () => {
  const spy = spyInvoke();
  let coreKilled = 0;
  const panic = createRealPanicClient(makeCoreCall(spy.invoke), async () => {
    coreKilled++;
  });
  panic.stop("run_9");
  panic.kill();
  assert.deepEqual(inner(spy.calls[0]), { cmd: "stop", args: { run_id: "run_9" } });
  assert.deepEqual(inner(spy.calls[1]), { cmd: "kill", args: {} });
  assert.equal(coreKilled, 1, "path 2: B2's core_kill hard terminate must also fire");
});

test("SchedulerCommands issue list_triggers/upsert_trigger and surface not_implemented honestly", async () => {
  const spy = spyInvoke(async () => {
    throw { code: "not_implemented", message: "no trigger store yet", retryable: false };
  });
  const scheduler = createRealScheduler(makeCoreCall(spy.invoke));

  const list = await scheduler.listTriggers();
  assert.equal(isNotImplemented(list), true, "list_triggers not_implemented is surfaced honestly, not thrown");
  assert.deepEqual(inner(spy.calls[0]), { cmd: "list_triggers", args: {} });

  const up = await scheduler.upsertTrigger({ kind: TRIGGER_KIND_CRON, workflow_name: "w", spec: "", enabled: true });
  assert.equal(isNotImplemented(up), true);
  assert.deepEqual(inner(spy.calls[1]), { cmd: "upsert_trigger", args: { kind: "cron", workflow_name: "w", spec: "", enabled: true } });
});
