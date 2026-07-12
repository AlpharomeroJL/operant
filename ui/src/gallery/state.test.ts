import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBusClient } from "../bus/mockClient.ts";
import type { BusEvent } from "../bus/types.ts";
import { createGallery } from "./state.ts";
import { loadFixturePublisherKeys, loadFixtureTemplates, PinStore, verifyAndPin } from "./catalog.ts";

test("browsing lists the fixture workflow as a card, not yet installed", () => {
  const bus = createMockBusClient();
  const gallery = createGallery(bus);

  const snap = gallery.getSnapshot();
  assert.equal(snap.empty, false);
  assert.equal(snap.cards.length, 1);
  assert.equal(snap.cards[0].name, "notepad-invoice-note");
  assert.equal(snap.cards[0].title, "Writes a dated invoice note into Notepad and saves it.");
  assert.equal(snap.cards[0].publisher, "operant-fixtures");
  assert.equal(snap.cards[0].installLabel, "Install");
  assert.equal(snap.cards[0].installed, false);

  gallery.dispose();
});

// BAR: installing the fixture workflow end to end in default mode shows the
// embedded plain-English step summary and the grant sentence, in plain
// language, before any approval is given.
test("install() opens a preview with plain-language steps and permissions before approval", () => {
  const bus = createMockBusClient();
  const gallery = createGallery(bus);

  gallery.install("notepad-invoice-note");
  const snap = gallery.getSnapshot();

  assert.ok(snap.preview, "install() must open a preview before doing anything");
  assert.equal(snap.preview?.title, "Writes a dated invoice note into Notepad and saves it.");
  assert.equal(snap.preview?.publisher, "operant-fixtures");
  assert.deepEqual(snap.preview?.stepLines, [
    "Click the text editor",
    "Type the invoice note",
    "Wait for the screen to update",
    "Save the file",
    "Wait for the screen to update",
    "Check that the note was written",
  ]);
  assert.deepEqual(snap.preview?.grant.sentences, ["This workflow can control Notepad."]);
  assert.equal(snap.preview?.grant.status, "pending");
  // First time this publisher has been seen: it previews only until promoted.
  assert.equal(
    snap.preview?.trustNote,
    "This is the first workflow from operant-fixtures. It will only preview its steps until you turn it on yourself.",
  );

  // Nothing has installed yet, and no bus event has fired.
  assert.equal(gallery.getSnapshot().cards[0].installed, false);

  gallery.dispose();
});

test("allow() completes a first-time-publisher install flagged preview-only, and publishes workflow.installed", () => {
  const bus = createMockBusClient();
  const events: BusEvent[] = [];
  bus.subscribe("workflow", (e) => events.push(e));
  const gallery = createGallery(bus);

  gallery.install("notepad-invoice-note");
  gallery.allow();

  const snap = gallery.getSnapshot();
  assert.equal(snap.preview, undefined, "approving closes the preview");
  assert.equal(snap.cards[0].installed, true);
  assert.equal(snap.cards[0].installLabel, "Installed");
  assert.equal(snap.notice, '"Writes a dated invoice note into Notepad and saves it." was added to your workflows.');

  assert.equal(events.length, 1);
  assert.equal(events[0].topic, "workflow.installed");
  assert.deepEqual(events[0].payload, {
    name: "notepad-invoice-note",
    version: "1.0.0",
    publisher: "operant-fixtures",
    signed: true,
    dry_run_only: true,
  });

  gallery.dispose();
});

test("a publisher already pinned installs ready to run, not preview-only", () => {
  const bus = createMockBusClient();
  const events: BusEvent[] = [];
  bus.subscribe("workflow", (e) => events.push(e));

  const pins = new PinStore();
  // Pin the fixture publisher to its real fingerprint before installing, the
  // same as a person who already trusts operant-fixtures from an earlier install.
  const [template] = loadFixtureTemplates();
  pins.observe(template.publisher, template.pubkey_fingerprint);

  const gallery = createGallery(bus, { pins });
  gallery.install("notepad-invoice-note");
  assert.equal(
    gallery.getSnapshot().preview?.trustNote,
    "operant-fixtures is already trusted, so this workflow is ready to run right away.",
  );

  gallery.allow();
  assert.deepEqual((events[0].payload as { dry_run_only: boolean }).dry_run_only, false);

  gallery.dispose();
});

test("deny() cancels the preview and installs nothing", () => {
  const bus = createMockBusClient();
  const events: BusEvent[] = [];
  bus.subscribe("workflow", (e) => events.push(e));
  const gallery = createGallery(bus);

  gallery.install("notepad-invoice-note");
  gallery.deny();

  const snap = gallery.getSnapshot();
  assert.equal(snap.preview, undefined);
  assert.equal(snap.cards[0].installed, false);
  assert.equal(snap.notice, "Not installed.");
  assert.deepEqual(events, []);

  gallery.dispose();
});

test("install() on an unknown workflow name is a no-op", () => {
  const bus = createMockBusClient();
  const gallery = createGallery(bus);
  gallery.install("does-not-exist");
  assert.equal(gallery.getSnapshot().preview, undefined);
  gallery.dispose();
});

test("allow()/deny() with no preview open are no-ops", () => {
  const bus = createMockBusClient();
  const gallery = createGallery(bus);
  assert.doesNotThrow(() => gallery.allow());
  assert.doesNotThrow(() => gallery.deny());
  gallery.dispose();
});

// A signed workflow whose publisher key nobody has: install() must not throw
// out of the state layer, and must give a plain three-part error (what, why,
// one suggested action) instead of a raw exception.
test("a signed workflow with no available publisher key surfaces a plain error, not a crash", () => {
  const bus = createMockBusClient();
  const gallery = createGallery(bus, { publisherKeys: {} });

  gallery.install("notepad-invoice-note");
  const snap = gallery.getSnapshot();
  assert.equal(snap.preview, undefined);
  assert.ok(snap.error);
  assert.equal(snap.error?.title, "This workflow could not be installed.");
  assert.equal(snap.error?.why, "Its details could not be checked.");
  assert.equal(snap.error?.action, "Try installing it again, or check with whoever shared it with you.");

  gallery.dispose();
});

test("dispose stops notifying subscribers", () => {
  const bus = createMockBusClient();
  const gallery = createGallery(bus);
  let notified = 0;
  gallery.subscribe(() => notified++);

  gallery.dispose();
  gallery.install("notepad-invoice-note");

  assert.equal(notified, 0);
});

// catalog.ts's crypto is real, not a stand-in: prove it against the exact
// fixture the Rust registry crate (crates/registry) also tests against.
test("catalog.verifyAndPin verifies the real fixture signature and pins on first use", () => {
  const [template] = loadFixtureTemplates();
  const keys = loadFixturePublisherKeys();
  const pins = new PinStore();

  const first = verifyAndPin(template, keys[template.publisher], pins);
  assert.equal(first, "first_time");
  const second = verifyAndPin(template, keys[template.publisher], pins);
  assert.equal(second, "trusted");
});

test("catalog.verifyAndPin rejects a tampered workflow record", () => {
  const [template] = loadFixtureTemplates();
  const keys = loadFixturePublisherKeys();
  const tampered = { ...template, description: "a different description entirely" };
  assert.throws(() => verifyAndPin(tampered, keys[tampered.publisher], new PinStore()));
});
