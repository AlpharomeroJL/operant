// DOM mount for the workflow library (docs/specs/ui.md: "workflow library
// (cards: name, plain summary, last run, minutes saved badge,
// run/schedule/explain buttons)"). Pure DOM, no bus: same split as
// ui/src/render/workflowView.ts (callbacks in, elements out).

import type { LibraryCard, LibrarySnapshot } from "./state.ts";

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
  const run = el("button", "op-button op-button--primary", card.runLabel);
  run.type = "button";
  if (opts.onRun) run.addEventListener("click", () => opts.onRun?.(card.name));

  const schedule = el("button", "op-button", card.scheduleLabel);
  schedule.type = "button";
  if (opts.onSchedule) schedule.addEventListener("click", () => opts.onSchedule?.(card.name));

  const explain = el("button", "op-button", card.explainLabel);
  explain.type = "button";
  if (opts.onExplain) explain.addEventListener("click", () => opts.onExplain?.(card.name));

  actions.append(run, schedule, explain);
  root.append(actions);
  return root;
}

/** Mount the library into `container`. Clears the container first so it can be re-mounted whenever the snapshot changes (a new run's last-run label, a newly installed card, ...). */
export function mountLibrary(container: HTMLElement, snapshot: LibrarySnapshot, opts: LibraryMountOptions = {}): HTMLElement {
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
  return root;
}
