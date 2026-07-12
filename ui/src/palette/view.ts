// DOM mount for the palette's floating overlay (docs/specs/design.md
// section 3, Palette: "a Raycast-grade centered top-third floating panel on
// Mica"). Pure DOM, no bus and no controller access: same split as
// ui/src/render/workflowView.ts and every other view.ts in ui/src
// (callbacks in, elements out); ui/src/main.ts owns wiring this to
// ./state.ts and toggling the backdrop's `hidden` attribute the way it
// already does for the grant prompt and the wizard (ui/src/main.ts's
// op-grant-backdrop / op-wizard-backdrop).
//
// mountPalette always renders the full dialog, including while
// `snapshot.open` is false: ui/src/__tests__/keyboard-first-timer.test.ts
// already asserts `.op-palette__input` is on screen "behind the wizard" on
// first load, before Ctrl+K has ever been pressed, the same "mount once,
// let the backdrop's hidden attribute gate visibility" contract the wizard
// and grant prompt already use.
//
// Accessibility (X8): this is an editable-combobox-with-listbox-popup
// (WAI-ARIA APG), not a roving-tabindex widget: the single text input holds
// real keyboard focus for as long as the palette is open, and
// aria-activedescendant plus aria-selected communicate which row is
// "selected" without ever moving focus onto the row itself. That keeps
// Escape/Tab/Enter/arrow handling in one place (this file's own keydown
// listener, the same pattern ui/src/wizard/view.ts uses for its Escape
// mapping) and means there is nothing here for
// ui/src/styles/focusPreserve.ts's trapFocus to do: focus never leaves the
// input while the dialog is open, so it can never walk out of it either.
// captureFocus/restoreFocus still apply, the same as every other view.ts
// that rebuilds its DOM on each keystroke (ui/src/library/view.ts's search
// box is the closest sibling).

import type { PaletteSnapshot, PaletteIntent } from "./state.ts";
import type { PaletteGroup, PaletteRow } from "./catalog.ts";
import { highlightSegments } from "./fuzzy.ts";
import { captureFocus, restoreFocus, FOCUS_KEY_ATTR } from "../styles/focusPreserve.ts";

export interface PaletteMountOptions {
  onQueryChange?: (text: string) => void;
  onMoveSelection?: (delta: 1 | -1) => void;
  /** `rowId` is set for a mouse click naming an exact row (./state.ts's PaletteController.commit accepts it for exactly this reason); omitted for a keyboard commit, which always acts on whatever is currently selected. */
  onCommit?: (intent: PaletteIntent, rowId?: string) => void;
  onClose?: () => void;
}

const INPUT_FOCUS_KEY = "palette-overlay-input";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** A DOM-id-safe form of a row's own id (workflow/action/setting ids can contain characters like ":" or "." that are legal in an HTML id but easier to avoid for a value that also has to round-trip through aria-activedescendant/getElementById lookups). */
function rowElementId(rowId: string): string {
  return `op-palette-row-${rowId.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
}

/** design.md section 3: "match-character highlighting." Unmatched runs render as plain text; matched runs get semibold weight via a <mark> reset to blend into the surrounding ink color (design.md section 1's thesis reserves the one signal color for recording/active/primary-CTA, so highlighting deliberately never spends amber on it). */
function highlightedTitle(title: string, indices: readonly number[]): DocumentFragment {
  const fragment = document.createDocumentFragment();
  for (const segment of highlightSegments(title, indices)) {
    if (segment.matched) {
      fragment.append(el("mark", "op-palette-overlay__match", segment.text));
    } else {
      fragment.append(document.createTextNode(segment.text));
    }
  }
  return fragment;
}

function rowEl(row: PaletteRow, selected: boolean, onPick?: (row: PaletteRow) => void): HTMLElement {
  const option = el("div", row.kind === "teach" ? "op-palette-overlay__row op-palette-overlay__row--teach" : "op-palette-overlay__row");
  option.id = rowElementId(row.id);
  option.setAttribute("role", "option");
  option.setAttribute("aria-selected", String(selected));
  if (selected) option.dataset.selected = "true";
  if (row.hint) option.title = row.hint;

  const title = el("p", "op-palette-overlay__row-title");
  title.append(highlightedTitle(row.title, row.highlight));
  option.append(title);

  if (row.subtitle) {
    option.append(el("p", "op-palette-overlay__row-subtitle", row.subtitle));
  }

  // Keyboard is the guaranteed path (design.md: "zero mouse required"); a
  // click is still wired for the person reaching for the mouse anyway, same
  // as every clickable card elsewhere in ui/src.
  if (onPick) {
    option.addEventListener("click", () => onPick(row));
  }

  return option;
}

function groupEl(group: PaletteGroup, selectedId: string | null, onPick?: (row: PaletteRow) => void): HTMLElement {
  const root = el("div", "op-palette-overlay__group");
  root.setAttribute("role", "group");
  const headingId = `op-palette-group-${group.title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
  root.setAttribute("aria-labelledby", headingId);

  const heading = el("p", "op-palette-overlay__group-title", group.title);
  heading.id = headingId;
  root.append(heading);

  for (const row of group.rows) {
    root.append(rowEl(row, row.id === selectedId, onPick));
  }
  return root;
}

/**
 * Mount the palette overlay into `container`. Clears the container first so
 * it can be re-mounted on every snapshot change (a keystroke, an arrow-key
 * selection move, a fresh registry entry), the same rebuild-from-scratch
 * pattern every screen in ui/src uses. Carries keyboard focus and the typed
 * query's caret across that rebuild (ui/src/styles/focusPreserve.ts) so
 * typing does not lose a keystroke or drop focus back to <body>.
 */
export function mountPalette(container: HTMLElement, snapshot: PaletteSnapshot, opts: PaletteMountOptions = {}): HTMLElement {
  const captured = captureFocus(container);
  container.textContent = "";

  const root = el("div", "op-palette-overlay");
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-modal", "true");
  root.setAttribute("aria-label", snapshot.overlayLabel);

  const inputRow = el("div", "op-palette-overlay__input-row");
  const label = el("label", "op-visually-hidden", snapshot.inputLabel);
  const input = el("input", "op-palette__input");
  input.id = "op-palette-overlay-input";
  input.type = "text";
  input.autocomplete = "off";
  input.placeholder = snapshot.placeholder;
  input.value = snapshot.query;
  input.setAttribute(FOCUS_KEY_ATTR, INPUT_FOCUS_KEY);
  input.setAttribute("role", "combobox");
  input.setAttribute("aria-autocomplete", "list");
  input.setAttribute("aria-expanded", "true");
  input.setAttribute("aria-controls", "op-palette-overlay-listbox");
  if (snapshot.selectedId) input.setAttribute("aria-activedescendant", rowElementId(snapshot.selectedId));
  else input.removeAttribute("aria-activedescendant");
  label.htmlFor = input.id;
  inputRow.append(label, input);
  root.append(inputRow);

  const listbox = el("div", "op-palette-overlay__listbox");
  listbox.id = "op-palette-overlay-listbox";
  listbox.setAttribute("role", "listbox");
  listbox.setAttribute("aria-label", snapshot.overlayLabel);

  // A click always commits "run" on the exact row clicked (design.md's
  // Enter-to-run default), never whichever row arrow keys last highlighted;
  // ./state.ts's commit(intent, rowId) takes the id for exactly this
  // reason. Keyboard remains the guaranteed path (design.md: "zero mouse
  // required"); this is the click affordance for reaching for the mouse anyway.
  const onPick = (row: PaletteRow) => opts.onCommit?.("run", row.id);

  if (snapshot.teachRow) {
    listbox.append(rowEl(snapshot.teachRow, true, onPick));
  } else if (snapshot.groups.length > 0) {
    for (const group of snapshot.groups) listbox.append(groupEl(group, snapshot.selectedId, onPick));
  }
  root.append(listbox);

  const footer = el("div", "op-palette-overlay__footer");
  footer.append(
    el("span", "op-palette-overlay__footer-hint", snapshot.footer.run),
    el("span", "op-palette-overlay__footer-hint", snapshot.footer.preview),
    el("span", "op-palette-overlay__footer-hint", snapshot.footer.details),
  );
  root.append(footer);

  input.addEventListener("input", () => opts.onQueryChange?.(input.value));
  input.addEventListener("keydown", (event) => {
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
        // Ctrl+Enter (or Cmd+Enter, the same dual-modifier convention
        // ./palette.ts's isGlobalPaletteHotkey already uses for Ctrl/Cmd+K)
        // previews a workflow instead of running it for real.
        opts.onCommit?.(event.ctrlKey || event.metaKey ? "preview" : "run");
        return;
      case "Tab":
        // Repurposed for "show details" (design.md's footer hint), not
        // focus movement: there is only ever one focusable control in this
        // dialog (the input itself), so there is nothing for Tab to move
        // focus to or from.
        event.preventDefault();
        opts.onCommit?.("details");
        return;
      case "Escape":
        event.preventDefault();
        opts.onClose?.();
        return;
    }
  });

  container.append(root);
  // Every rebuild while open (a keystroke, an arrow-key move) restores focus
  // onto the equivalent input via captureFocus/restoreFocus, same as every
  // other live-typed field in ui/src (ui/src/library/view.ts's search box is
  // the closest sibling). The one case that leaves nothing to restore is the
  // opening transition itself, when the container was previously empty
  // (mounted but hidden behind op-palette-backdrop): a keyboard user who
  // just pressed Ctrl+K should never have to click into the field before
  // typing, the same guarantee ui/src/wizard/view.ts's focusOnSectionChange
  // gives the wizard's own first screen.
  const restored = restoreFocus(container, captured);
  if (!restored && snapshot.open) input.focus();
  return root;
}
