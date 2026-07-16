// DOM mount for the target-app picker (ui/src/palette/targetApp.ts). Pure DOM,
// no controller access: callbacks in, elements out, the same split as
// ui/src/palette/view.ts and every other view.ts in ui/src. ui/src/main.ts
// wires this to the picker controller and gates the backdrop's `hidden`
// attribute the way it already does for the palette and the grant prompt.
//
// Accessibility: a modal dialog (WAI-ARIA APG) whose one focusable control is a
// listbox that holds keyboard focus for as long as the picker is open;
// aria-activedescendant plus aria-selected communicate which row is selected
// without moving focus onto the row itself, the same aria-activedescendant
// pattern ui/src/palette/view.ts uses so Enter/Escape/arrow handling stays in
// one keydown listener and focus never walks out of the dialog. While the list
// is still loading (or genuinely empty) there is no listbox, so the dialog
// itself takes focus and only Escape is meaningful.

import type { TargetAppSnapshot } from "./targetApp.ts";
import { FOCUS_KEY_ATTR } from "../styles/focusPreserve.ts";

export interface TargetAppMountOptions {
  onMoveSelection?: (delta: 1 | -1) => void;
  /** `rowId` is set for a mouse click naming an exact row; omitted for a keyboard Enter, which acts on the current selection. */
  onConfirm?: (rowId?: string) => void;
  onCancel?: () => void;
}

const DIALOG_FOCUS_KEY = "target-app-dialog";
const LISTBOX_FOCUS_KEY = "target-app-listbox";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** A DOM-id-safe form of a row's own id (a window id could carry characters that are awkward to round-trip through aria-activedescendant/getElementById). */
function rowElementId(rowId: string): string {
  return `op-target-app-row-${rowId.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
}

function rowEl(
  row: TargetAppSnapshot["rows"][number],
  selected: boolean,
  onPick?: (rowId: string) => void,
): HTMLElement {
  const option = el("div", row.frontApp ? "op-target-app__row op-target-app__row--front" : "op-target-app__row");
  option.id = rowElementId(row.id);
  option.setAttribute("role", "option");
  option.setAttribute("aria-selected", String(selected));
  if (selected) option.dataset.selected = "true";

  option.append(el("p", "op-target-app__row-title", row.title));
  if (row.subtitle) option.append(el("p", "op-target-app__row-subtitle", row.subtitle));

  // Keyboard is the guaranteed path; a click is wired for the person reaching
  // for the mouse anyway, the same as the palette's own rows.
  if (onPick) option.addEventListener("click", () => onPick(row.id));
  return option;
}

/**
 * Mount the picker into `container`, rebuilding from scratch on each snapshot
 * (the same pattern every screen in ui/src uses). Puts and keeps keyboard focus
 * on the listbox while it exists (so aria-activedescendant is meaningful), or on
 * the dialog itself while loading, so a keyboard user who just committed the
 * goal never has to click before pressing an arrow key or Escape.
 */
export function mountTargetAppPicker(container: HTMLElement, snapshot: TargetAppSnapshot, opts: TargetAppMountOptions = {}): HTMLElement {
  container.textContent = "";

  const root = el("div", "op-target-app");
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-modal", "true");
  root.setAttribute("aria-label", snapshot.overlayLabel);
  root.tabIndex = -1;
  root.setAttribute(FOCUS_KEY_ATTR, DIALOG_FOCUS_KEY);

  const headingId = "op-target-app-heading";
  const heading = el("h2", "op-target-app__heading", snapshot.heading);
  heading.id = headingId;
  root.append(heading);

  let listbox: HTMLElement | null = null;
  if (snapshot.loading) {
    const status = el("p", "op-target-app__status", snapshot.loadingLabel);
    status.setAttribute("role", "status");
    root.append(status);
  } else if (snapshot.rows.length === 0) {
    const status = el("p", "op-target-app__status", snapshot.emptyLabel);
    status.setAttribute("role", "status");
    root.append(status);
  } else {
    listbox = el("div", "op-target-app__listbox");
    listbox.id = "op-target-app-listbox";
    listbox.setAttribute("role", "listbox");
    listbox.setAttribute("aria-labelledby", headingId);
    listbox.tabIndex = 0;
    listbox.setAttribute(FOCUS_KEY_ATTR, LISTBOX_FOCUS_KEY);
    if (snapshot.selectedId) listbox.setAttribute("aria-activedescendant", rowElementId(snapshot.selectedId));
    const onPick = (rowId: string) => opts.onConfirm?.(rowId);
    for (const row of snapshot.rows) listbox.append(rowEl(row, row.id === snapshot.selectedId, onPick));
    root.append(listbox);
  }

  const footer = el("div", "op-target-app__footer");
  footer.append(
    el("span", "op-target-app__footer-hint", snapshot.confirmHint),
    el("span", "op-target-app__footer-hint", snapshot.cancelHint),
  );
  root.append(footer);

  // One keydown listener on the dialog root, so it fires whether focus is on the
  // listbox or (while loading) the dialog itself. Arrow keys move the selection,
  // Enter confirms the current one, Escape cancels back to the goal.
  root.addEventListener("keydown", (event) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        opts.onMoveSelection?.(1);
        return;
      case "ArrowUp":
        event.preventDefault();
        opts.onMoveSelection?.(-1);
        return;
      case "Enter":
        event.preventDefault();
        opts.onConfirm?.();
        return;
      case "Escape":
        event.preventDefault();
        opts.onCancel?.();
        return;
    }
  });

  container.append(root);

  // Focus the listbox when it exists (so its aria-activedescendant names a real
  // focused-container's active option), otherwise the dialog itself, so Escape
  // still routes while loading. Only ever grabs focus while actually open.
  if (snapshot.open) (listbox ?? root).focus();
  return root;
}
