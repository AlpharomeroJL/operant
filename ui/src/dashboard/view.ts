// DOM mount for the Home dashboard (docs/specs/design.md section 3). Pure
// DOM, no bus: same split as ui/src/library/view.ts and
// ui/src/render/workflowView.ts. No control here holds typed or otherwise
// stateful focus (the dashboard is read-only: a hero line, a sparkline, and
// two plain lists, no buttons or inputs), so unlike ui/src/library/view.ts
// this does not need ui/src/styles/focusPreserve.ts's capture/restore
// around its full-container rebuild.

import type { DashboardSnapshot, UpNextRow, RecentRunRow } from "./state.ts";
import { SPARKLINE_WIDTH, SPARKLINE_HEIGHT } from "./state.ts";

export interface DashboardMountOptions {
  /**
   * H1: the empty state's one specific action (docs/specs/design.md section
   * 4's copy rule). ui/src/main.ts wires this to the same command palette
   * ui/src/palette/ already opens from anywhere in the shell, so "Teach your
   * first workflow" here starts the identical teach flow as typing a fresh
   * goal into the palette by hand.
   */
  onTeach?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const SVG_NS = "http://www.w3.org/2000/svg";

function svgEl<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string> = {}): SVGElementTagNameMap[K] {
  const node = document.createElementNS(SVG_NS, tag) as SVGElementTagNameMap[K];
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  return node;
}

/**
 * The hero line plus its sparkline. The sparkline is aria-hidden: it is a
 * trend glyph redundant with the hero line's own words (which already state
 * this week's total) and snapshot.sparklineSummary, its visually-hidden
 * text equivalent, same reasoning as every status dot elsewhere in this app
 * (ui/src/styles/contrast.ts's documented dot exemption).
 */
function heroEl(snapshot: DashboardSnapshot): HTMLElement {
  const hero = el("div", "op-dashboard__hero");
  hero.append(el("p", "op-dashboard__hero-line", snapshot.heroLine));

  const svg = svgEl("svg", {
    class: "op-dashboard__sparkline",
    viewBox: `0 0 ${SPARKLINE_WIDTH} ${SPARKLINE_HEIGHT}`,
    preserveAspectRatio: "none",
    "aria-hidden": "true",
  });
  const points = snapshot.sparklinePoints.map((p) => `${p.x},${p.y}`).join(" ");
  svg.append(svgEl("polyline", { points, class: "op-dashboard__sparkline-line", fill: "none" }));
  hero.append(svg);
  hero.append(el("span", "op-visually-hidden", snapshot.sparklineSummary));
  return hero;
}

function upNextRowEl(row: UpNextRow): HTMLElement {
  const li = el("li", "op-dashboard__row");
  li.append(el("span", "op-dashboard__row-title", row.title));
  li.append(el("span", "op-dashboard__row-when", row.whenLabel));
  return li;
}

/** Compact row: status dot (with its own visually-hidden text equivalent), name, one-line outcome, relative time (design.md section 3). */
function recentRunRowEl(row: RecentRunRow): HTMLElement {
  const li = el("li", "op-dashboard__row");
  const dot = el("span", "op-status__dot");
  dot.dataset.state = row.status;
  dot.setAttribute("aria-hidden", "true");
  li.append(dot, el("span", "op-visually-hidden", row.statusLabel));
  li.append(el("span", "op-dashboard__row-title", row.title));
  li.append(el("span", "op-dashboard__row-outcome", row.outcomeLabel));
  li.append(el("span", "op-dashboard__row-when", row.whenLabel));
  return li;
}

function listOrEmpty<T>(items: readonly T[], rowEl: (item: T) => HTMLElement, emptyLabel: string): HTMLElement {
  if (items.length === 0) return el("p", "op-empty", emptyLabel);
  const ul = el("ul", "op-dashboard__list");
  for (const item of items) ul.append(rowEl(item));
  return ul;
}

function sectionEl(headingId: string, headingText: string, body: HTMLElement): HTMLElement {
  const section = el("section", "op-dashboard__section");
  section.setAttribute("aria-labelledby", headingId);
  const heading = el("h3", "op-dashboard__section-title", headingText);
  heading.id = headingId;
  section.append(heading, body);
  return section;
}

/**
 * H1: the empty state's one specific action (docs/specs/design.md section 3's
 * Wizard finish screen copy, reused here: "a single amber 'Teach your first
 * workflow' button"; section 4's copy rule, "Empty states invite one specific
 * action"). `.op-button--primary` is the app's one existing amber-fill
 * button style (ui/src/styles/base.css, painted from the signal/accent
 * token, ui/src/theme/tokens.ts) -- the same class Library's own primary Run
 * button already uses, so this is not a new color, just its first use as a
 * standalone call to action rather than a per-card row button.
 */
function emptyActionButton(label: string, onTeach?: () => void): HTMLButtonElement {
  const button = el("button", "op-button op-button--primary", label);
  button.type = "button";
  button.addEventListener("click", () => onTeach?.());
  return button;
}

/** Mount the dashboard into `container`, clearing it first (same rebuild-from-scratch pattern as every other screen under ui/src). */
export function mountDashboard(container: HTMLElement, snapshot: DashboardSnapshot, opts: DashboardMountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("section", "op-dashboard");
  root.setAttribute("aria-labelledby", "op-dashboard-heading");

  const heading = el("h2", "op-panel__title", snapshot.title);
  heading.id = "op-dashboard-heading";
  root.append(heading, heroEl(snapshot));

  if (snapshot.empty) {
    const empty = el("div", "op-empty-state");
    empty.append(el("p", "op-empty", snapshot.emptyLabel), emptyActionButton(snapshot.emptyActionLabel, opts.onTeach));
    root.append(empty);
  } else {
    // When scheduling itself is not wired (list_triggers answered
    // not_implemented), say so honestly rather than showing the weaker
    // "nothing scheduled yet," which would imply a working-but-empty scheduler.
    const upNextBody = snapshot.upNextUnavailable
      ? el("p", "op-empty", snapshot.upNextUnavailableLabel)
      : listOrEmpty(snapshot.upNext, upNextRowEl, snapshot.upNextEmptyLabel);
    root.append(sectionEl("op-dashboard-upnext-heading", snapshot.upNextTitle, upNextBody));
    root.append(
      sectionEl("op-dashboard-recent-heading", snapshot.recentRunsTitle, listOrEmpty(snapshot.recentRuns, recentRunRowEl, snapshot.recentRunsEmptyLabel)),
    );
  }

  container.append(root);
  return root;
}
