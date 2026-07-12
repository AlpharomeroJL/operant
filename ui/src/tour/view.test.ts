// H1: DOM tests for the first-run tour, new alongside the re-pointed screen
// map (./state.ts's header comment). Uses a real jsdom document
// (../styles/testDomEnv.ts), the same harness every other view.ts's tests
// under ui/src use, since the callout's dismiss button has to actually
// dispatch a click.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createDomEnv } from "../styles/testDomEnv.ts";
import { createTourStore } from "./state.ts";
import { tourStrings, hintStrings } from "./strings.ts";
import { mountTourCallout, mountContextualHint } from "./view.ts";

test("the callout walks the new nav in order: dashboard, library, runs, settings, then nothing", () => {
  const env = createDomEnv();
  try {
    const tour = createTourStore("dashboard");
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    mountTourCallout(container, tour.getSnapshot());
    assert.equal(container.querySelector(".op-tour-callout__title")?.textContent, tourStrings.dashboardTitle);
    assert.equal(container.querySelector(".op-tour-callout__message")?.textContent, tourStrings.dashboardMessage);

    tour.nextStep();
    mountTourCallout(container, tour.getSnapshot());
    assert.equal(container.querySelector(".op-tour-callout__title")?.textContent, tourStrings.libraryTitle);

    tour.nextStep();
    mountTourCallout(container, tour.getSnapshot());
    assert.equal(container.querySelector(".op-tour-callout__title")?.textContent, tourStrings.runsTitle);

    tour.nextStep();
    mountTourCallout(container, tour.getSnapshot());
    assert.equal(container.querySelector(".op-tour-callout__title")?.textContent, tourStrings.settingsTitle);

    tour.nextStep();
    mountTourCallout(container, tour.getSnapshot());
    assert.equal(container.querySelector(".op-tour-callout"), null, "the tour must render nothing once done");
    assert.equal(tour.getSnapshot().completed, true);

    tour.dispose();
  } finally {
    env.cleanup();
  }
});

test("dismissing a callout (\"Got it\") reports onDismiss, which is what ui/src/main.ts wires to advance the tour", () => {
  const env = createDomEnv();
  try {
    const tour = createTourStore("dashboard");
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    let dismissed = 0;
    mountTourCallout(container, tour.getSnapshot(), { onDismiss: () => dismissed++ });

    const button = container.querySelector<HTMLButtonElement>(".op-tour-callout button");
    assert.ok(button, "the callout must have a dismiss button");
    assert.equal(button.textContent, "Got it");
    button.click();

    assert.equal(dismissed, 1);
    tour.dispose();
  } finally {
    env.cleanup();
  }
});

test("the callout is a polite live region so a screen reader announces each new step", () => {
  const env = createDomEnv();
  try {
    const tour = createTourStore("dashboard");
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    mountTourCallout(container, tour.getSnapshot());
    const callout = container.querySelector(".op-tour-callout");
    assert.equal(callout?.getAttribute("role"), "status");
    assert.equal(callout?.getAttribute("aria-live"), "polite");

    tour.dispose();
  } finally {
    env.cleanup();
  }
});

test("a contextual hint renders its text and retires on close", () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    let retired = 0;
    mountContextualHint(container, "palette-hint", hintStrings.paletteHint, false, { onRetire: () => retired++ });

    const hint = container.querySelector(".op-hint");
    assert.ok(hint, "an unretired hint must render");
    assert.equal(container.querySelector(".op-hint__text")?.textContent, hintStrings.paletteHint);

    container.querySelector<HTMLButtonElement>(".op-hint__close")?.click();
    assert.equal(retired, 1);
  } finally {
    env.cleanup();
  }
});

test("a retired hint renders nothing", () => {
  const env = createDomEnv();
  try {
    const container = env.document.createElement("div");
    env.document.body.appendChild(container);

    mountContextualHint(container, "palette-hint", hintStrings.paletteHint, true);
    assert.equal(container.querySelector(".op-hint"), null);
  } finally {
    env.cleanup();
  }
});
