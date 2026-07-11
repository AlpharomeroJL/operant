import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY } from "../bus/types.ts";
import { createLibrary } from "./state.ts";
import { createMockRegistry, type MockWorkflowRecord } from "./mockRegistry.ts";

function oneWorkflowRegistry(): ReturnType<typeof createMockRegistry> {
  const record: MockWorkflowRecord = {
    manifest: {
      v: 1,
      name: "copy-invoice-total",
      version: "1.0.0",
      description: "Copy the invoice total into the spreadsheet",
      step_summary: ["Click things"],
      inputs_schema: { type: "object", properties: {} },
      capabilities: { paths: ["C:\\Users\\demo\\Downloads"], apps: ["chrome.exe"], network: false, risk_ceiling: "write" },
      dsl: { path: "workflows/copy-invoice-total.ts", hash: "0".repeat(64) },
    },
    steps: [{ kind: "key", params: { combo: "ctrl+c" } }],
    signed: true,
    dryRunOnly: false,
  };
  return createMockRegistry([record]);
}

test("library renders cards: name, plain summary, last run, and a minutes-saved badge", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: oneWorkflowRegistry() });

  const snap = library.getSnapshot();
  assert.equal(snap.empty, false);
  assert.equal(snap.cards.length, 1);

  const [card] = snap.cards;
  assert.equal(card.name, "copy-invoice-total");
  assert.equal(card.title, "Copy the invoice total into the spreadsheet");
  assert.equal(card.summary, "Copy the invoice total into the spreadsheet");
  assert.equal(card.lastRunLabel, "Not run yet");
  assert.equal(card.minutesSaved, 0);
  assert.equal(card.minutesSavedLabel, "0 minutes saved");
  assert.equal(card.runLabel, "Run");
  assert.equal(card.scheduleLabel, "Schedule");
  assert.equal(card.explainLabel, "Explain");

  library.dispose();
});

test("an empty registry renders the empty-library message, not zero silent cards", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry([]) });

  const snap = library.getSnapshot();
  assert.equal(snap.empty, true);
  assert.equal(snap.cards.length, 0);
  assert.equal(snap.emptyLabel, "No workflows yet. Teach it something to save your first one.");
});

test("run() starts a saved-workflow run on the bus and updates last run + notifies subscribers", () => {
  const bus = createMockBusClient();
  const topics: string[] = [];
  bus.subscribe("run", (e) => topics.push(e.topic));
  let tick = 1_000_000;
  const library = createLibrary(bus, { registry: oneWorkflowRegistry(), now: () => tick });

  let notified = 0;
  library.subscribe(() => notified++);

  library.run("copy-invoice-total");

  assert.deepEqual(topics, ["run.started", "run.completed"]);
  assert.ok(notified >= 1);

  const card = library.getSnapshot().cards[0];
  assert.equal(card.lastRunLabel, "Last run just now");

  library.dispose();
});

test("run() on an unknown workflow name is a no-op", () => {
  const bus = createMockBusClient();
  const events: string[] = [];
  bus.subscribe("*", (e) => events.push(e.topic));
  const library = createLibrary(bus, { registry: oneWorkflowRegistry() });

  library.run("does-not-exist");

  assert.deepEqual(events, []);
  library.dispose();
});

test("minutes saved compares how long teaching took against how long later runs take", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: oneWorkflowRegistry() });

  // Teaching it took 10 minutes; two later runs each take 1 minute.
  bus.publish("run.started", { run_id: "e1", goal: "teach", mode: RUN_MODE_EXPLORE, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "e1", outcome: "ok", steps: 4, wall_ms: 10 * 60_000 });

  bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "r1", outcome: "ok", steps: 4, wall_ms: 60_000 });

  bus.publish("run.started", { run_id: "r2", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "r2", outcome: "ok", steps: 4, wall_ms: 60_000 });

  // (10 - 1) minutes saved per replay * 2 replays = 18 minutes.
  const card = library.getSnapshot().cards[0];
  assert.equal(card.minutesSaved, 18);
  assert.equal(card.minutesSavedLabel, "18 minutes saved");

  library.dispose();
});

test("a run.completed for an untracked run id is ignored (no workflow_name was ever seen for it)", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: oneWorkflowRegistry() });

  bus.publish("run.completed", { run_id: "mystery", outcome: "ok", steps: 1, wall_ms: 10 });

  const card = library.getSnapshot().cards[0];
  assert.equal(card.lastRunLabel, "Not run yet");
  library.dispose();
});

test("workflow.installed registers a new card that was not in the initial registry", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry([]) });

  assert.equal(library.getSnapshot().empty, true);

  bus.publish("workflow.installed", { name: "new-workflow", version: "1.0.0", signed: true, dry_run_only: false });

  const snap = library.getSnapshot();
  assert.equal(snap.empty, false);
  assert.equal(snap.cards.length, 1);
  assert.equal(snap.cards[0].name, "new-workflow");

  library.dispose();
});

test("explain() renders the full plain-English workflow view via U4A's renderer, grant included", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: oneWorkflowRegistry() });

  const view = library.explain("copy-invoice-total");

  assert.ok(view);
  assert.equal(view?.name, "copy-invoice-total");
  assert.equal(view?.grant, "This workflow can read files in Downloads and control Chrome.");
  assert.equal(view?.steps.length, 1);
  assert.equal(view?.steps[0].sentence, "Copy the selection");

  library.dispose();
});

test("explain() on an unknown workflow returns undefined", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: oneWorkflowRegistry() });
  assert.equal(library.explain("nope"), undefined);
  library.dispose();
});

test("schedule() reports the request with the workflow's plain title, does not touch the bus", () => {
  const bus = createMockBusClient();
  const events: string[] = [];
  bus.subscribe("*", (e) => events.push(e.topic));
  const requests: Array<{ name: string; title: string }> = [];
  const library = createLibrary(bus, {
    registry: oneWorkflowRegistry(),
    onScheduleRequested: (name, title) => requests.push({ name, title }),
  });

  library.schedule("copy-invoice-total");

  assert.deepEqual(requests, [{ name: "copy-invoice-total", title: "Copy the invoice total into the spreadsheet" }]);
  assert.deepEqual(events, [], "scheduling a trigger is not modeled on the bus yet; this must not fake one");

  library.dispose();
});

test("dispose stops listening to both the bus and the registry", () => {
  const bus = createMockBusClient();
  const registry = oneWorkflowRegistry();
  const library = createLibrary(bus, { registry });
  let notified = 0;
  library.subscribe(() => notified++);

  library.dispose();
  registry.upsert("copy-invoice-total", { signed: false });
  bus.publish("workflow.installed", { name: "x", version: "1.0.0", signed: true, dry_run_only: false });

  assert.equal(notified, 0);
});
