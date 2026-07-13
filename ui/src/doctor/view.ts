// DOM mount for the "Check my setup" surface (docs/specs/ui.md: "doctor check").
// Pure DOM, no bus: same split as ui/src/library/view.ts (callbacks in, elements out).

import "./doctor.css";
import type { DoctorCard, DoctorSnapshot } from "./state.ts";

export interface DoctorMountOptions {
  onFix?: (findingId: string) => void;
  /** Dismiss the screen. When given, a Close button is rendered in the header, so the modal this mounts into is dismissible from the keyboard, not only by clicking the backdrop. */
  onClose?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function cardEl(card: DoctorCard, opts: DoctorMountOptions): HTMLElement {
  // The severity modifier lets ui/src/doctor/doctor.css tint a problem card
  // (error/warn) apart from a healthy one (info), so the eye lands on what
  // actually needs attention before reading a word.
  const root = el("article", `op-doctor-card op-doctor-card--${card.severity}`);
  root.setAttribute("aria-label", card.what);

  root.append(el("h3", "op-doctor-card__what", card.what));
  root.append(el("p", "op-doctor-card__why", card.why));
  root.append(el("p", "op-doctor-card__action", card.action));

  if (card.fixCommand) {
    const fix = el("button", "op-button op-button--primary", card.fixLabel);
    fix.type = "button";
    if (opts.onFix) fix.addEventListener("click", () => opts.onFix?.(card.findingId));
    root.append(fix);
  }

  return root;
}

/** Mount the doctor findings into `container`. Clears the container first so it can be re-mounted whenever findings change. */
export function mountDoctor(container: HTMLElement, snapshot: DoctorSnapshot, opts: DoctorMountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("section", "op-doctor");
  root.setAttribute("aria-labelledby", "op-doctor-heading");

  const header = el("div", "op-doctor__header");
  const heading = el("h2", "op-panel__title", snapshot.title);
  heading.id = "op-doctor-heading";
  header.append(heading);
  if (opts.onClose) {
    const close = el("button", "op-button op-doctor__close", "Close");
    close.type = "button";
    close.addEventListener("click", () => opts.onClose?.());
    header.append(close);
  }
  root.append(header);

  if (snapshot.empty) {
    root.append(el("p", "op-empty", "Your setup looks good. No issues found."));
  } else {
    const grid = el("div", "op-doctor__grid");
    for (const card of snapshot.cards) grid.append(cardEl(card, opts));
    root.append(grid);
  }

  container.append(root);
  return root;
}
