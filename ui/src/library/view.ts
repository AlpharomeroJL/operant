// DOM mount for the workflow library (docs/specs/ui.md: "workflow library
// (cards: name, plain summary, last run, minutes saved badge,
// run/schedule/explain buttons)"; docs/specs/design.md section 3's restyle:
// a duotone glyph and hue per card, a last-run dot, hover-revealed actions,
// drag to reorder, and a live search filter). Pure DOM, no bus: same split
// as ui/src/render/workflowView.ts (callbacks in, elements out).
//
// Accessibility (X8): mountLibrary rebuilds the whole grid from scratch on
// every snapshot (a new run's last-run label, a newly installed card, a
// keystroke in the search box, ...), which by default drops keyboard focus
// to <body> the instant a person activates Run/Schedule/Explain on a card,
// or types into the search field, since whatever they just touched is
// destroyed and rebuilt. ui/src/styles/focusPreserve.ts's capture/restore
// carries focus (and, for the search input, the typed text and caret) onto
// the equivalent control in the new grid instead.
//
// Drag to reorder uses the plain HTML5 drag-and-drop events
// (dragstart/dragover/drop), reporting only a name and a before-name to
// ui/src/library/state.ts's reorder(); that function (not this DOM wiring)
// is what ui/src/library/state.test.ts exercises, the same "state owns the
// logic, view owns the DOM" split as everything else in this file.

import type { LibraryCard, LibrarySnapshot } from "./state.ts";
import { captureFocus, restoreFocus, FOCUS_KEY_ATTR } from "../styles/focusPreserve.ts";

export interface LibraryMountOptions {
  onRun?: (name: string) => void;
  onSchedule?: (name: string) => void;
  onExplain?: (name: string) => void;
  /** design.md section 3, Library: "Drag to reorder." beforeName is null when a card is dropped past the last one (move to the end). */
  onReorder?: (name: string, beforeName: string | null) => void;
  /** design.md section 3, Library: "Search filters live." */
  onSearchChange?: (query: string) => void;
  /** H1: the empty state's one specific action, shown only when the library has zero saved workflows (see LibrarySnapshot.emptyActionLabel). Wired in ui/src/main.ts to the same command palette ui/src/dashboard/view.ts's own empty-state button opens. */
  onTeach?: () => void;
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

/**
 * design.md section 3, Library: "auto-assigned duotone glyph and hue."
 * Purely decorative (aria-hidden): redundant with the card's own visible
 * name right beside it, the same reasoning ui/src/styles/contrast.ts
 * documents for every status dot in this app. The hue is applied as a CSS
 * custom property consumed by a `filter: hue-rotate()` rule in
 * ui/src/styles/base.css, rotating the app's one existing accent color
 * (ui/src/theme/tokens.ts's `signal`) rather than storing a second,
 * independent 12-color palette outside that file (see ./glyph.ts's header
 * comment).
 */
function glyphEl(card: LibraryCard): HTMLElement {
  const glyph = el("span", "op-library-card__glyph");
  glyph.style.setProperty("--op-glyph-rotate", `${card.glyphHueRotationDeg}deg`);
  glyph.setAttribute("aria-hidden", "true");
  glyph.append(el("span", "op-library-card__glyph-letter", card.glyphLetter));
  return glyph;
}

function cardEl(card: LibraryCard, opts: LibraryMountOptions): HTMLElement {
  const root = el("article", "op-library-card");
  root.setAttribute("aria-label", card.title);
  root.draggable = true;
  root.dataset.workflowName = card.name;

  const header = el("div", "op-library-card__header");
  header.append(glyphEl(card), el("h3", "op-library-card__name", card.title));
  root.append(header);

  if (card.summary && card.summary !== card.title) {
    root.append(el("p", "op-library-card__summary", card.summary));
  }

  const meta = el("p", "op-library-card__meta");
  const lastRun = el("span", "op-library-card__last-run");
  // design.md section 3, Library: "a last-run dot." aria-hidden: the visible
  // text right beside it (card.lastRunLabel, "Last run 2 hours ago" / "Not
  // run yet") already says the same thing in words, same redundant-dot
  // pattern as every other status dot in this app.
  const dot = el("span", "op-status__dot");
  dot.dataset.state = card.lastRunStatus;
  dot.setAttribute("aria-hidden", "true");
  lastRun.append(dot, document.createTextNode(` ${card.lastRunLabel}`));
  meta.append(lastRun, el("span", "op-badge op-library-card__minutes", card.minutesSavedLabel));
  root.append(meta);

  const actions = el("div", "op-library-card__actions");
  const run = actionButton(card.runLabel, "op-button op-button--primary", `library-run-${card.name}`, () => opts.onRun?.(card.name));
  const schedule = actionButton(card.scheduleLabel, "op-button", `library-schedule-${card.name}`, () => opts.onSchedule?.(card.name));
  const explain = actionButton(card.explainLabel, "op-button", `library-explain-${card.name}`, () => opts.onExplain?.(card.name));
  actions.append(run, schedule, explain);
  root.append(actions);

  root.addEventListener("dragstart", (event) => {
    root.classList.add("op-library-card--dragging");
    event.dataTransfer?.setData("text/plain", card.name);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  });
  root.addEventListener("dragend", () => {
    root.classList.remove("op-library-card--dragging");
  });
  root.addEventListener("dragover", (event) => {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  });
  root.addEventListener("drop", (event) => {
    event.preventDefault();
    event.stopPropagation(); // the grid's own drop handler (append-to-end) only fires for a drop that missed every card
    const draggedName = event.dataTransfer?.getData("text/plain");
    if (draggedName && draggedName !== card.name) opts.onReorder?.(draggedName, card.name);
  });

  return root;
}

function searchEl(snapshot: LibrarySnapshot, opts: LibraryMountOptions): HTMLElement {
  const wrap = el("div", "op-library__search");
  const label = el("label", "op-visually-hidden", snapshot.searchLabel);
  label.htmlFor = "op-library-search-input";
  const input = el("input", "op-library__search-input");
  input.id = "op-library-search-input";
  input.type = "search";
  input.autocomplete = "off";
  input.placeholder = snapshot.searchPlaceholder;
  input.value = snapshot.searchQuery;
  input.setAttribute(FOCUS_KEY_ATTR, "library-search");
  input.addEventListener("input", () => opts.onSearchChange?.(input.value));
  wrap.append(label, input);
  return wrap;
}

/**
 * Mount the library into `container`. Clears the container first so it can
 * be re-mounted whenever the snapshot changes (a new run's last-run label, a
 * newly installed card, a search keystroke, ...), carrying keyboard focus
 * (and the search field's typed text/caret) across that rebuild (see this
 * file's header comment) so pressing Run/Schedule/Explain, or typing a
 * search, does not also drop focus to <body>.
 */
export function mountLibrary(container: HTMLElement, snapshot: LibrarySnapshot, opts: LibraryMountOptions = {}): HTMLElement {
  const captured = captureFocus(container);
  container.textContent = "";
  const root = el("section", "op-library");
  root.setAttribute("aria-labelledby", "op-library-heading");

  const heading = el("h2", "op-panel__title", snapshot.title);
  heading.id = "op-library-heading";
  root.append(heading, searchEl(snapshot, opts));

  if (snapshot.empty) {
    const empty = el("div", "op-empty-state");
    empty.append(el("p", "op-empty", snapshot.emptyLabel));
    // H1 (docs/specs/design.md section 4: "Empty states invite one specific
    // action"): only when the library is genuinely empty, never for a
    // search that simply matched nothing (LibrarySnapshot.emptyActionLabel's
    // own header comment).
    if (snapshot.emptyActionLabel) {
      const teach = actionButton(snapshot.emptyActionLabel, "op-button op-button--primary", "library-teach", () => opts.onTeach?.());
      empty.append(teach);
    }
    root.append(empty);
  } else {
    const grid = el("div", "op-library__grid");
    for (const card of snapshot.cards) grid.append(cardEl(card, opts));
    grid.addEventListener("dragover", (event) => {
      event.preventDefault();
    });
    grid.addEventListener("drop", (event) => {
      event.preventDefault();
      const draggedName = event.dataTransfer?.getData("text/plain");
      if (draggedName) opts.onReorder?.(draggedName, null);
    });
    root.append(grid);
  }

  container.append(root);
  restoreFocus(container, captured);
  return root;
}
