// DOM mount for the Undo screen (docs/specs/design.md section 3: "opens a
// preview list of restorations in plain English with per-item checkmarks.
// Irreversible items are grayed with 'cannot be undone.' Confirm executes
// with the same filmstrip treatment in reverse."). Pure DOM, no bus: same
// split as ui/src/grants/view.ts (callbacks in, elements out); ./state.ts
// stays DOM-free so it runs under plain `node --test`.
//
// Two looks, one per sentence above: the preview phase is a plain
// checkmarked list (a checkmark for each restorable item; an irreversible
// one gets no mark and reads grayed, its own sentence already saying
// "cannot be undone", ./mockJournal.ts's previewLine). The done phase (after
// Confirm) swaps the checkmark for the flight recorder's own status dot
// (ui/src/runViewer/view.ts's .op-status__dot, ui/src/styles/base.css),
// green for a restored item and the same quiet idle gray an irreversible one
// already used, so the result reads as the run viewer's own instrument, just
// narrating the run in reverse. The modal card itself reuses
// ui/src/grants/view.ts's .op-grant-prompt chrome as-is.
//
// This screen holds no typed input, so unlike ui/src/runViewer/view.ts and
// ui/src/library/view.ts it does not need ui/src/styles/focusPreserve.ts's
// capture/restore around its rebuild.

import type { UndoScreenSnapshot, UndoItemView, UndoPhase } from "./state.ts";

export interface UndoMountOptions {
  onConfirm?: () => void;
  onClose?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** One restoration row. `phase` picks the leading marker: a checkmark glyph while previewing, the run viewer's own status dot once done. */
function itemEl(item: UndoItemView, phase: UndoPhase): HTMLLIElement {
  const li = el("li", item.irreversible ? "op-undo-item op-undo-item--irreversible" : "op-undo-item");

  let marker: HTMLElement;
  if (phase === "done") {
    marker = el("span", "op-status__dot");
    marker.dataset.state = item.irreversible || !item.applied ? "pending" : "done";
  } else {
    marker = el("span", "op-undo-item__check", item.irreversible ? "" : "✓");
  }
  marker.setAttribute("aria-hidden", "true");

  li.append(marker, el("span", "op-step__sentence", item.text));
  return li;
}

/**
 * Mount the Undo screen into `container`, clearing it first (the same
 * rebuild-from-scratch pattern every screen under ui/src uses). Renders
 * nothing when the screen is closed, so ui/src/main.ts can call this
 * unconditionally on every undo-screen snapshot and separately toggle its
 * modal backdrop's `hidden` off snapshot.phase.
 */
export function mountUndoScreen(container: HTMLElement, snapshot: UndoScreenSnapshot, opts: UndoMountOptions = {}): HTMLElement {
  container.textContent = "";
  if (snapshot.phase === "closed") return container;

  const root = el("section", "op-grant-prompt op-undo-screen");
  root.setAttribute("role", "alertdialog");
  root.setAttribute("aria-labelledby", "op-undo-heading");

  const heading = el("h2", "op-panel__title", snapshot.title);
  heading.id = "op-undo-heading";
  root.append(heading);

  if (!snapshot.hasItems) {
    root.append(el("p", "op-empty", snapshot.emptyLabel));
  } else {
    if (snapshot.phase === "done") {
      root.append(el("p", "op-undo-screen__summary", snapshot.doneSummary));
    }
    const list = el("ul", "op-step-list");
    list.setAttribute("aria-label", snapshot.title);
    for (const item of snapshot.items) list.append(itemEl(item, snapshot.phase));
    root.append(list);
  }

  const actions = el("div", "op-change-card__actions");
  if (snapshot.phase === "preview") {
    if (snapshot.hasItems) {
      const confirm = el("button", "op-button op-button--primary", snapshot.confirmLabel);
      confirm.type = "button";
      if (opts.onConfirm) confirm.addEventListener("click", opts.onConfirm);
      actions.append(confirm);
    }
    const cancel = el("button", "op-button", snapshot.cancelLabel);
    cancel.type = "button";
    if (opts.onClose) cancel.addEventListener("click", opts.onClose);
    actions.append(cancel);
  } else {
    const close = el("button", "op-button", snapshot.closeLabel);
    close.type = "button";
    if (opts.onClose) close.addEventListener("click", opts.onClose);
    actions.append(close);
  }
  root.append(actions);

  container.append(root);
  return root;
}
