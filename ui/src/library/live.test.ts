// B5 (library-live): the library shows REAL saved workflows and its actions
// work through the request/response bridge (contracts/ipc.md), with a graceful
// fall back to the seeded ./mockRegistry.ts in dev/Demo. These tests drive the
// real DOM (ui/src/library/view.ts via createDomEnv) and assert on the rendered
// markup, not just the snapshot object, matching the "verify via DOM" bar.
//
// The fake CommandClient stands in for the shell's real invoke-backed client:
// it records every req and answers each with a canned res (contracts/ipc.md
// section 2c), and for start_replay it also echoes the run.* events back on the
// bus exactly as the core does (section 3b), so the full Run loop is exercised.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createMockBusClient, type BusClient } from "../bus/mockClient.ts";
import { RUN_MODE_REPLAY } from "../bus/types.ts";
import { ERROR_NOT_IMPLEMENTED, type CommandClient, type CommandResult } from "../bus/commandClient.ts";
import { createLibrary } from "./state.ts";
import { createMockRegistry, type MockWorkflowRecord } from "./mockRegistry.ts";
import { libraryStrings } from "./strings.ts";
import { mountLibrary } from "./view.ts";

interface RecordedCall {
  cmd: string;
  args: Record<string, unknown> | undefined;
}

type Handler = (cmd: string, args: Record<string, unknown> | undefined) => CommandResult<unknown>;

function fakeClient(handler: Handler): { client: CommandClient; calls: RecordedCall[] } {
  const calls: RecordedCall[] = [];
  const client: CommandClient = {
    request<T>(cmd: string, args?: Record<string, unknown>): Promise<CommandResult<T>> {
      calls.push({ cmd, args });
      return Promise.resolve(handler(cmd, args) as CommandResult<T>);
    },
  };
  return { client, calls };
}

// Two real-shaped manifests distinct from ./mockRegistry.ts's demo seed, so a
// test can tell "the real list replaced the seed" from "the seed rendered."
// Each carries the compiled `path` the shell's list_workflows DTO provides for
// start_replay / explain_workflow (contracts/ipc.md sections 5b/5c).
function realRecords(): MockWorkflowRecord[] {
  return [
    {
      manifest: {
        v: 1,
        name: "notepad-invoice-note",
        version: "1.0.0",
        description: "Write an invoice note in Notepad and save it",
        step_summary: ["Click the text editor", "Type the invoice note", "Save the file"],
        inputs_schema: { type: "object", properties: {} },
        capabilities: { apps: ["notepad.exe"], paths: [], network: false, risk_ceiling: "write" },
        dsl: { path: "workflows/notepad-invoice-note.ts", hash: "a".repeat(64) },
      },
      steps: [
        { kind: "type", params: { text: "Invoice total 142.50" } },
        { kind: "key", params: { combo: "ctrl+s" } },
      ],
      publisher: "acme",
      signed: true,
      dryRunOnly: false,
      path: "compiled/notepad-invoice-note.json",
    },
    {
      manifest: {
        v: 1,
        name: "reconcile-ledger",
        version: "2.1.0",
        description: "Reconcile the monthly ledger",
        step_summary: ["Open the ledger", "Match the rows"],
        inputs_schema: { type: "object", properties: {} },
        capabilities: { apps: ["excel.exe"], paths: [], network: false, risk_ceiling: "read" },
        dsl: { path: "workflows/reconcile-ledger.ts", hash: "b".repeat(64) },
      },
      steps: [{ kind: "wait" }],
      publisher: "acme",
      signed: true,
      dryRunOnly: false,
      path: "compiled/reconcile-ledger.json",
    },
  ];
}

function cardTitles(container: Element): string[] {
  return [...container.querySelectorAll(".op-library-card__name")].map((n) => n.textContent ?? "");
}

// A flushed macrotask boundary: lets a fire-and-forget command promise (Run's
// start_replay, Schedule's upsert_trigger) settle deterministically before the
// assertions, without leaning on how many microtasks the chain happens to use.
function flush(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

test("real-list path: at mount the library loads its cards from list_workflows, replacing the demo seed", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const { client, calls } = fakeClient((cmd) => {
      if (cmd === "list_workflows") return { ok: true, result: realRecords() };
      throw new Error(`unexpected command ${cmd}`);
    });
    // Start from the full demo seed to prove the real list REPLACES it.
    const library = createLibrary(bus, { registry: createMockRegistry(), client });
    await library.ready;

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountLibrary(container, library.getSnapshot());

    assert.deepEqual(calls.map((c) => c.cmd), ["list_workflows"], "the only load command is list_workflows");
    const titles = cardTitles(container);
    assert.deepEqual(titles, ["Write an invoice note in Notepad and save it", "Reconcile the monthly ledger"], "cards show the real manifests' plain summaries");
    // The seeded demo workflows are gone: the real list is the source now.
    assert.equal(container.querySelector('[data-op-focus-key="library-run-copy-invoice-total"]'), null, "the demo seed must not survive a real list_workflows load");
    assert.ok(container.querySelector('[data-op-focus-key="library-run-notepad-invoice-note"]'), "each real card has its Run button");

    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("dev/Demo fallback: with no client the library renders the seeded mockRegistry cards", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const library = createLibrary(bus, { registry: createMockRegistry() });
    // ready resolves immediately when there is no bridge to load from.
    await library.ready;

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    mountLibrary(container, library.getSnapshot());

    const titles = cardTitles(container);
    assert.ok(titles.includes("Copy the invoice total into the spreadsheet"), "the seeded demo workflow renders as the dev/Demo fallback");
    assert.equal(container.querySelectorAll(".op-library-card").length, 3, "all three seeded demo cards render");

    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("Run routes through start_replay; the core's echoed run.* events (not a faked one) move the last-run dot", async () => {
  const env = createDomEnv();
  try {
    const bus: BusClient = createMockBusClient();
    const byPath = new Map(realRecords().map((r) => [r.path, r.manifest.name]));
    const runStartedTopics: string[] = [];
    bus.subscribe("run.started", () => runStartedTopics.push("run.started"));

    const { client, calls } = fakeClient((cmd, args) => {
      if (cmd === "list_workflows") return { ok: true, result: realRecords() };
      if (cmd === "start_replay") {
        // The core wraps the Replayer in synthetic run.* events and echoes them
        // on the bus (contracts/ipc.md section 3b). Replicate that here.
        const name = byPath.get(String(args?.path));
        bus.publish("run.started", { run_id: "run-live", goal: "replay", mode: RUN_MODE_REPLAY, workflow_name: name });
        bus.publish("run.completed", { run_id: "run-live", outcome: "ok", steps: 2, wall_ms: 400 });
        return { ok: true, result: { run_id: "run-live", steps_executed: 2, pre: ["pass"], post: [] } };
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    const library = createLibrary(bus, { registry: createMockRegistry([]), client, now: () => 1_000_000 });
    await library.ready;

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    function render(): void {
      mountLibrary(container, library.getSnapshot(), { onRun: (name) => library.run(name) });
    }
    render();
    library.subscribe(render);

    const runButton = container.querySelector<HTMLButtonElement>('[data-op-focus-key="library-run-notepad-invoice-note"]');
    assert.ok(runButton, "the real workflow's Run button is on screen");
    runButton.click();
    await flush();

    assert.deepEqual(calls.map((c) => c.cmd), ["list_workflows", "start_replay"], "Run issues start_replay, not a synthetic run");
    assert.deepEqual(calls[1].args, { path: "compiled/notepad-invoice-note.json" }, "start_replay is called with the workflow's compiled path");
    assert.equal(runStartedTopics.length, 1, "exactly one run.started reached the bus: the core's echo, with no second one faked by the library");

    const lastRun = container.querySelector(".op-library-card__last-run");
    assert.ok(lastRun?.textContent?.includes("Last run just now"), "the echoed run.completed advanced the card's last-run label");
    const dot = container.querySelector(".op-library-card .op-status__dot");
    assert.equal(dot?.getAttribute("data-state"), "ok", "the last-run dot flipped to ok from the real run outcome");

    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("Explain routes through explain_workflow and returns the core's rendered view", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const { client, calls } = fakeClient((cmd) => {
      if (cmd === "list_workflows") return { ok: true, result: realRecords() };
      if (cmd === "explain_workflow") {
        return {
          ok: true,
          result: {
            title: "Write an invoice note in Notepad and save it",
            summary: "Write an invoice note in Notepad and save it",
            grant: "This workflow can control Notepad.",
            inputs: [],
            steps: [{ n: 1, kind: "type", parts: [{ t: "text", text: "Type the invoice note" }], sentence: "Type the invoice note", irreversible: false }],
          },
        };
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    const library = createLibrary(bus, { registry: createMockRegistry([]), client });
    await library.ready;

    const view = await library.explain("notepad-invoice-note");

    assert.deepEqual(calls.map((c) => c.cmd), ["list_workflows", "explain_workflow"]);
    assert.deepEqual(calls[1].args, { path: "compiled/notepad-invoice-note.json" }, "explain_workflow is called with the workflow's path");
    assert.ok(view);
    assert.equal(view?.name, "notepad-invoice-note", "name is filled from the record so the shape matches the local render");
    assert.equal(view?.grant, "This workflow can control Notepad.", "the grant prose comes from the core's explain_workflow result");
    assert.equal(view?.steps[0].sentence, "Type the invoice note");

    library.dispose();
  } finally {
    env.cleanup();
  }
});

test("Schedule calls upsert_trigger and surfaces not_implemented honestly, never faking a scheduled trigger", async () => {
  const env = createDomEnv();
  try {
    const bus = createMockBusClient();
    const busTopics: string[] = [];
    bus.subscribe("*", (e) => busTopics.push(e.topic));

    const { client, calls } = fakeClient((cmd) => {
      if (cmd === "list_workflows") return { ok: true, result: realRecords() };
      if (cmd === "upsert_trigger") {
        // contracts/ipc.md section 5e/5g: reserved, not wired in this build.
        return { ok: false, error: { code: ERROR_NOT_IMPLEMENTED, message: "cmd is reserved in this contract but not wired in this build", retryable: false } };
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    let notice: string | null = null;
    const library = createLibrary(bus, {
      registry: createMockRegistry([]),
      client,
      onScheduleRequested: (_name, title) => {
        notice = libraryStrings.scheduleNotice(title);
        render();
      },
    });
    await library.ready;

    const container = env.document.createElement("div");
    env.document.body.appendChild(container);
    function render(): void {
      mountLibrary(container, library.getSnapshot(), { onSchedule: (name) => library.schedule(name) });
      if (notice) {
        const p = env.document.createElement("p");
        p.className = "op-schedule-notice";
        p.textContent = notice;
        container.append(p);
      }
    }
    render();

    const scheduleButton = container.querySelector<HTMLButtonElement>('[data-op-focus-key="library-schedule-notepad-invoice-note"]');
    assert.ok(scheduleButton, "the real workflow's Schedule button is on screen");
    scheduleButton.click();
    await flush();

    assert.deepEqual(calls.map((c) => c.cmd), ["list_workflows", "upsert_trigger"], "Schedule issues upsert_trigger");
    assert.equal(String(calls[1].args?.workflow_name), "notepad-invoice-note", "upsert_trigger names the workflow being scheduled");
    assert.ok(notice, "the not_implemented result is surfaced as an honest notice");
    const noticeEl = container.querySelector(".op-schedule-notice");
    assert.ok(noticeEl?.textContent?.includes("not set up yet"), "the DOM shows scheduling is not available yet");
    // No fake success: nothing on the bus claims a run started or a trigger fired.
    assert.deepEqual(busTopics.filter((t) => t.startsWith("run.") || t.startsWith("trigger.") || t.startsWith("schedule.")), [], "scheduling must not fabricate a run, trigger, or schedule event");

    library.dispose();
  } finally {
    env.cleanup();
  }
});
