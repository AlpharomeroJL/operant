// The zero-code workflow view: mounts a rendered workflow (from @operant/sdk's
// plain-English renderer) as numbered steps with inline, editable parameter
// chips, plus grant prose and the changeable details. Also mounts a drift offer
// card. This module only builds DOM; all wording is either supplied by the
// renderer (already plain and jargon-free) or drawn from ./strings.ts.
//
// The view-model shapes are declared locally so this module has no build-time
// dependency on the SDK package resolution; the SDK's renderWorkflow output
// matches these shapes exactly.

import { workflowViewStrings as S } from "./strings.ts";

export type ViewPart =
  | { t: "text"; text: string }
  | { t: "chip"; param: string; value: string; input?: boolean; editable?: boolean };

export interface ViewStep {
  n: number;
  kind: string;
  parts: ViewPart[];
  sentence: string;
  irreversible: boolean;
}

export interface ViewInput {
  name: string;
  label: string;
  kind: string;
  value: string;
  pattern?: string;
  format?: string;
}

export interface WorkflowView {
  name: string;
  title: string;
  summary: string;
  grant: string;
  inputs: ViewInput[];
  steps: ViewStep[];
}

export interface DriftOfferView {
  headline: string;
  question: string;
  text: string;
  accept: string;
  dismiss: string;
  preview?: string;
}

/** Called whenever a parameter chip or a details field is edited. */
export type EditHandler = (name: string, value: string) => void;

export interface MountOptions {
  onEdit?: EditHandler;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function chipInput(part: Extract<ViewPart, { t: "chip" }>, onEdit?: EditHandler): HTMLElement {
  if (part.editable) {
    const input = el("input", "op-chip op-chip--editable");
    input.type = "text";
    input.value = part.value;
    input.setAttribute("aria-label", part.param);
    input.dataset.param = part.param;
    if (onEdit) {
      input.addEventListener("change", () => onEdit(part.param, input.value));
    }
    return input;
  }
  const span = el("span", "op-chip", part.value);
  span.dataset.param = part.param;
  return span;
}

function stepItem(step: ViewStep, onEdit?: EditHandler): HTMLLIElement {
  const li = el("li", "op-workflow-step");
  li.dataset.kind = step.kind;

  const line = el("span", "op-workflow-step__line");
  for (const part of step.parts) {
    if (part.t === "text") {
      line.append(document.createTextNode(part.text));
    } else {
      line.append(chipInput(part, onEdit));
    }
  }
  li.append(line);

  if (step.irreversible) {
    li.append(el("span", "op-badge op-badge--irreversible", S.irreversibleBadge));
  }
  return li;
}

function detailField(field: ViewInput, onEdit?: EditHandler): HTMLElement {
  const wrap = el("div", "op-field");
  const label = el("label", "op-field__label", field.label);
  const input = el("input", "op-field__input");
  input.type = "text";
  input.value = field.value;
  input.dataset.name = field.name;
  const id = `op-field-${field.name}`;
  input.id = id;
  label.htmlFor = id;
  if (field.pattern) input.pattern = field.pattern;
  if (onEdit) {
    input.addEventListener("change", () => onEdit(field.name, input.value));
  }
  wrap.append(label, input);
  return wrap;
}

/**
 * Mount a rendered workflow into `container`. Returns the created root element.
 * Clears the container first so it can be re-mounted on updates.
 */
export function mountWorkflowView(container: HTMLElement, view: WorkflowView, opts: MountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("section", "op-workflow-view");
  root.setAttribute("aria-label", view.title);

  root.append(el("h2", "op-workflow-view__title", view.title));
  if (view.summary && view.summary !== view.title) {
    root.append(el("p", "op-workflow-view__summary", view.summary));
  }

  // Grant prose.
  const grantSection = el("section", "op-workflow-view__grant");
  grantSection.append(el("h3", "op-panel__title", S.grantHeading));
  grantSection.append(el("p", "op-grant", view.grant));
  root.append(grantSection);

  // Changeable details.
  if (view.inputs.length) {
    const details = el("section", "op-workflow-view__details");
    details.append(el("h3", "op-panel__title", S.detailsHeading));
    for (const field of view.inputs) details.append(detailField(field, opts.onEdit));
    root.append(details);
  }

  // Numbered steps.
  const stepsSection = el("section", "op-workflow-view__steps");
  stepsSection.append(el("h3", "op-panel__title", S.stepsHeading));
  if (view.steps.length) {
    const ol = el("ol", "op-workflow-step-list");
    for (const step of view.steps) ol.append(stepItem(step, opts.onEdit));
    stepsSection.append(ol);
  } else {
    stepsSection.append(el("p", "op-empty", S.emptySteps));
  }
  root.append(stepsSection);

  container.append(root);
  return root;
}

export interface DriftMountOptions {
  onAccept?: () => void;
  onDismiss?: () => void;
}

/** Mount a drift offer card (a plain heads-up plus a yes/no choice). */
export function mountDriftCard(container: HTMLElement, offer: DriftOfferView, opts: DriftMountOptions = {}): HTMLElement {
  container.textContent = "";
  const card = el("section", "op-change-card");
  card.setAttribute("role", "alertdialog");
  card.append(el("p", "op-change-card__headline", offer.headline));
  card.append(el("p", "op-change-card__question", offer.question));
  if (offer.preview) card.append(el("p", "op-change-card__preview", offer.preview));

  const actions = el("div", "op-change-card__actions");
  const accept = el("button", "op-button op-button--primary", offer.accept);
  accept.type = "button";
  if (opts.onAccept) accept.addEventListener("click", opts.onAccept);
  const dismiss = el("button", "op-button", offer.dismiss);
  dismiss.type = "button";
  if (opts.onDismiss) dismiss.addEventListener("click", opts.onDismiss);
  actions.append(accept, dismiss);
  card.append(actions);

  container.append(card);
  return card;
}
