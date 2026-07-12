import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import { RUN_MODE_EXPLORE, RUN_MODE_REPLAY } from "../bus/types.ts";
import { createLibrary } from "./state.ts";
import { createMockRegistry, type MockWorkflowRecord } from "./mockRegistry.ts";
import { assignGlyph } from "./glyph.ts";

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

test("each card's glyph is deterministic from its name: matches ./glyph.ts directly and is stable across separate createLibrary calls", () => {
  const bus = createMockBusClient();
  const library1 = createLibrary(bus, { registry: oneWorkflowRegistry() });
  const [card1] = library1.getSnapshot().cards;
  library1.dispose();

  const library2 = createLibrary(createMockBusClient(), { registry: oneWorkflowRegistry() });
  const [card2] = library2.getSnapshot().cards;
  library2.dispose();

  const expected = assignGlyph("copy-invoice-total", "Copy the invoice total into the spreadsheet");
  assert.equal(card1.glyphLetter, expected.letter);
  assert.equal(card1.glyphHueRotationDeg, expected.hueRotationDeg);
  assert.equal(card1.glyphLetter, card2.glyphLetter);
  assert.equal(card1.glyphHueRotationDeg, card2.glyphHueRotationDeg);
});

test("distinct workflows in the default seed land on more than one hue (not a coincidental all-same collision)", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry() });
  const hues = new Set(library.getSnapshot().cards.map((c) => c.glyphHueRotationDeg));
  assert.ok(hues.size > 1);
  library.dispose();
});

test("a workflow with no runs yet shows a pending last-run dot; a completed run flips it to ok or failed", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: oneWorkflowRegistry() });

  assert.equal(library.getSnapshot().cards[0].lastRunStatus, "pending");

  bus.publish("run.started", { run_id: "r1", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "r1", outcome: "failed", steps: 1, wall_ms: 10 });
  assert.equal(library.getSnapshot().cards[0].lastRunStatus, "failed");

  bus.publish("run.started", { run_id: "r2", goal: "run", mode: RUN_MODE_REPLAY, workflow_name: "copy-invoice-total" });
  bus.publish("run.completed", { run_id: "r2", outcome: "ok", steps: 4, wall_ms: 400 });
  assert.equal(library.getSnapshot().cards[0].lastRunStatus, "ok");

  library.dispose();
});

test("live search filters cards by title or name, case-insensitively, without touching the underlying registry", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry() });

  library.setSearchQuery("invoice");
  let snap = library.getSnapshot();
  assert.equal(snap.cards.length, 1);
  assert.equal(snap.cards[0].name, "copy-invoice-total");
  assert.equal(snap.empty, false);

  library.setSearchQuery("BACKUP");
  snap = library.getSnapshot();
  assert.equal(snap.cards.length, 1);
  assert.equal(snap.cards[0].name, "backup-photos");

  library.setSearchQuery("");
  assert.equal(library.getSnapshot().cards.length, 3, "clearing the search restores every card");

  library.dispose();
});

test("a search that matches nothing shows the no-matches message, not the no-workflows-at-all message", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry() });

  library.setSearchQuery("does-not-exist-anywhere");
  const snap = library.getSnapshot();
  assert.equal(snap.cards.length, 0);
  assert.equal(snap.empty, true);
  assert.equal(snap.emptyLabel, "No workflows match your search.");

  library.dispose();
});

test("an empty registry still says 'no workflows yet', not 'no matches', even while a search box happens to hold text", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry([]) });

  library.setSearchQuery("anything");
  const snap = library.getSnapshot();
  assert.equal(snap.empty, true);
  assert.equal(snap.emptyLabel, "No workflows yet. Teach it something to save your first one.");

  library.dispose();
});

test("setSearchQuery notifies subscribers, and is idempotent for an unchanged query", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry() });
  let notified = 0;
  library.subscribe(() => notified++);

  library.setSearchQuery("invoice");
  assert.equal(notified, 1);
  library.setSearchQuery("invoice");
  assert.equal(notified, 1, "setting the same query again must not re-notify");

  library.dispose();
});

test("reorder moves a card to just before another named card; drag to reorder (design.md section 3)", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry() });
  const namesInOrder = () => library.getSnapshot().cards.map((c) => c.name);

  assert.deepEqual(namesInOrder(), ["copy-invoice-total", "weekly-report-email", "backup-photos"]);

  library.reorder("backup-photos", "copy-invoice-total");
  assert.deepEqual(namesInOrder(), ["backup-photos", "copy-invoice-total", "weekly-report-email"]);

  library.dispose();
});

test("reorder with beforeName null moves a card to the end", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry() });

  library.reorder("copy-invoice-total", null);
  assert.deepEqual(
    library.getSnapshot().cards.map((c) => c.name),
    ["weekly-report-email", "backup-photos", "copy-invoice-total"],
  );

  library.dispose();
});

test("reorder is a no-op for an unknown name, and for reordering a card relative to itself", () => {
  const bus = createMockBusClient();
  const library = createLibrary(bus, { registry: createMockRegistry() });
  const before = library.getSnapshot().cards.map((c) => c.name);

  library.reorder("does-not-exist", "copy-invoice-total");
  assert.deepEqual(library.getSnapshot().cards.map((c) => c.name), before);

  library.reorder("copy-invoice-total", "copy-invoice-total");
  assert.deepEqual(library.getSnapshot().cards.map((c) => c.name), before);

  library.dispose();
});

test("a newly installed workflow's card joins the end of the display order, after any drag-to-reorder result", () => {
  const bus = createMockBusClient();
  const registry = createMockRegistry();
  const library = createLibrary(bus, { registry });

  library.reorder("backup-photos", "copy-invoice-total");
  bus.publish("workflow.installed", { name: "brand-new", version: "1.0.0", signed: true, dry_run_only: false });

  assert.deepEqual(library.getSnapshot().cards.map((c) => c.name), ["backup-photos", "copy-invoice-total", "weekly-report-email", "brand-new"]);

  library.dispose();
});
