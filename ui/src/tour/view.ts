// DOM rendering for tour callouts and contextual hints. Pure DOM, no bus:
// same split as ui/src/library/view.ts and ui/src/runViewer/view.ts
// (callbacks in, elements out).

import type { TourStep, TourSnapshot } from "./state.ts";
import { tourStrings } from "./strings.ts";
// Per-module stylesheet (same pattern as ui/src/wizard/view.ts's own
// "./wizard.css" import): keeps this surface's CSS out of the shared
// ui/src/styles/base.css, which every other lane in the campaign also
// appends to.
import "./tour.css";

export interface TourCalloutOptions {
  onDismiss?: () => void;
}

export interface ContextualHintOptions {
  onRetire?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

interface CalloutContent {
  title: string;
  message: string;
}

function calloutContentFor(step: TourStep): CalloutContent | null {
  switch (step) {
    case "dashboard":
      return { title: tourStrings.dashboardTitle, message: tourStrings.dashboardMessage };
    case "library":
      return { title: tourStrings.libraryTitle, message: tourStrings.libraryMessage };
    case "runs":
      return { title: tourStrings.runsTitle, message: tourStrings.runsMessage };
    case "settings":
      return { title: tourStrings.settingsTitle, message: tourStrings.settingsMessage };
    case "done":
      return null;
  }
}

/**
 * Render a tour callout for the current step. Returns null if the tour is done.
 * Callouts are dismissible and advance to the next step when dismissed.
 */
export function renderTourCallout(snapshot: TourSnapshot, opts: TourCalloutOptions = {}): HTMLElement | null {
  const content = calloutContentFor(snapshot.step);
  if (!content) return null;

  const root = el("div", "op-tour-callout");
  root.setAttribute("role", "status");
  root.setAttribute("aria-live", "polite");

  const title = el("h3", "op-tour-callout__title", content.title);
  root.append(title);

  const message = el("p", "op-tour-callout__message", content.message);
  root.append(message);

  const close = el("button", "op-button op-button--small", "Got it");
  close.type = "button";
  close.addEventListener("click", () => opts.onDismiss?.());
  root.append(close);

  return root;
}

/**
 * Render a contextual hint for a specific control. Hints are small one-line
 * tips that retire (disappear) after the user first succeeds at the action
 * they describe.
 */
export function renderContextualHint(hintId: string, hintText: string, isRetired: boolean, opts: ContextualHintOptions = {}): HTMLElement | null {
  if (isRetired) return null;

  const root = el("div", "op-hint");
  root.dataset.hintId = hintId;
  root.setAttribute("role", "status");
  root.setAttribute("aria-live", "polite");

  const text = el("span", "op-hint__text", hintText);
  root.append(text);

  const close = el("button", "op-hint__close", "×");
  close.type = "button";
  close.setAttribute("aria-label", "Dismiss hint");
  close.addEventListener("click", () => opts.onRetire?.());
  root.append(close);

  return root;
}

/**
 * Mount a tour callout into a container. Clears the container first.
 */
export function mountTourCallout(container: HTMLElement, snapshot: TourSnapshot, opts: TourCalloutOptions = {}): void {
  container.textContent = "";
  const callout = renderTourCallout(snapshot, opts);
  if (callout) container.append(callout);
}

/**
 * Mount a contextual hint into a container. Clears the container first.
 */
export function mountContextualHint(container: HTMLElement, hintId: string, hintText: string, isRetired: boolean, opts: ContextualHintOptions = {}): void {
  container.textContent = "";
  const hint = renderContextualHint(hintId, hintText, isRetired, opts);
  if (hint) container.append(hint);
}
