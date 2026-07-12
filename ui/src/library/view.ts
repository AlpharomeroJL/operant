// DOM mount for the workflow library (docs/specs/ui.md: "workflow library
// (cards: name, plain summary, last run, minutes saved badge,
// run/schedule/explain buttons)"). Pure DOM, no bus: same split as
// ui/src/render/workflowView.ts (callbacks in, elements out).
//
// Accessibility (X8): mountLibrary rebuilds the whole grid from scratch on
// every snapshot (a new run's last-run label, a newly installed card, ...),
// which by default drops keyboard focus to <body> the instant a person
// activates Run/Schedule/Explain on a card, since the button they just
// pressed is destroyed and rebuilt. ui/src/styles/focusPreserve.ts's
// capture/restore carries focus onto the equivalent button in the new grid
// instead, so pressing Run does not also silently strand a keyboard user's
// focus.

import type { LibraryCard, LibrarySnapshot } from "./state.ts";
import { captureFocus, restoreFocus, FOCUS_KEY_ATTR } from "../styles/focusPreserve.ts";

export interface LibraryMountOptions {
  onRun?: (name: string) => void;
  onSchedule?: (name: string) => void;
  onExplain?: (name: string) => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function actionButton(label: string, className: string, focusKey: string, onClick?: () => void): HTMLButtonElement {
  const b = el("button", className, label);
  b.type = "button";
  b.setAttribute(FOCUS_KEY_ATTR, focusKey);
  if (onClick) b.addEventListener("click", onClick);
  return b;
}

function cardEl(card: LibraryCard, opts: LibraryMountOptions): HTMLElement {
  const root = el("article", "op-library-card");
  root.setAttribute("aria-label", card.title);

  root.append(el("h3", "op-library-card__name", card.title));
  if (card.summary && card.summary !== card.title) {
    root.append(el("p", "op-library-card__summary", card.summary));
  }

  const meta = el("p", "op-library-card__meta");
  meta.append(el("span", "op-library-card__last-run", card.lastRunLabel));
  meta.append(el("span", "op-badge op-library-card__minutes", card.minutesSavedLabel));
  root.append(meta);

  const actions = el("div", "op-library-card__actions");
  const run = actionButton(card.runLabel, "op-button op-button--primary", `library-run-${card.name}`, () => opts.onRun?.(card.name));
  const schedule = actionButton(card.scheduleLabel, "op-button", `library-schedule-${card.name}`, () => opts.onSchedule?.(card.name));
  const explain = actionButton(card.explainLabel, "op-button", `library-explain-${card.name}`, () => opts.onExplain?.(card.name));

  actions.append(run, schedule, explain);
  root.append(actions);
  return root;
}

/**
 * Mount the library into `container`. Clears the container first so it can
 * be re-mounted whenever the snapshot changes (a new run's last-run label, a
 * newly installed card, ...), carrying keyboard focus across that rebuild
 * (see this file's header comment) so pressing Run/Schedule/Explain does not
 * also drop focus to <body>.
 */
export function mountLibrary(container: HTMLElement, snapshot: LibrarySnapshot, opts: LibraryMountOptions = {}): HTMLElement {
  const captured = captureFocus(container);
  container.textContent = "";
  const root = el("section", "op-library");
  root.setAttribute("aria-labelledby", "op-library-heading");

  const heading = el("h2", "op-panel__title", snapshot.title);
  heading.id = "op-library-heading";
  root.append(heading);

  if (snapshot.empty) {
    root.append(el("p", "op-empty", snapshot.emptyLabel));
  } else {
    const grid = el("div", "op-library__grid");
    for (const card of snapshot.cards) grid.append(cardEl(card, opts));
    root.append(grid);
  }

  container.append(root);
  restoreFocus(container, captured);
  return root;
}
