// Shared focus-management helpers for every view.ts under ui/src that
// rebuilds its DOM from scratch on each snapshot (`container.textContent =
// ""` then re-append), the pattern documented in ui/src/library/view.ts's
// mountLibrary comment and used by every screen in this app. That pattern
// has a real keyboard-accessibility bug by default: rebuilding destroys the
// currently focused element, and a removed element's focus falls back to
// <body> (verified against ui/src/wizard/view.ts's access-key text field,
// where typing a single character used to lose focus on every keystroke).
// The helpers here let a view.ts opt in to carrying focus (and, for text
// inputs, the caret/selection) across a rebuild, plus the two related
// concerns any modal dialog needs: moving focus into fresh content when the
// screen/section changes, and trapping Tab/Shift+Tab inside the dialog so it
// cannot escape to whatever is behind the modal backdrop.

/** Tag a focusable element with a stable identity so captureFocus/restoreFocus can find its replacement after a rebuild. Unique per container, not per page. */
export const FOCUS_KEY_ATTR = "data-op-focus-key";

export interface CapturedFocus {
  key: string;
  /** The focused text input's live value, captured independently of whatever the next snapshot says. Most views (ui/src/wizard/view.ts's access-key field) already keep typed text in their own state, so the rebuilt input already has the right value and this is redundant; a view with no such state to round-trip through (ui/src/runViewer/view.ts's intervene field, which is deliberately not part of RunViewerSnapshot: a redirect instruction is submit-once, not a thing worth persisting) relies on this to avoid restoreFocus putting the cursor back into a blanked-out field. */
  value: string | null;
  selectionStart: number | null;
  selectionEnd: number | null;
}

function isTextSelectable(el: Element): el is HTMLInputElement {
  if (!(el instanceof HTMLInputElement)) return false;
  // Only text-like input types expose selectionStart/selectionEnd; radio,
  // checkbox, and others throw on access in some engines.
  return ["text", "search", "url", "tel", "password", "email"].includes(el.type);
}

/**
 * Call before tearing down `container`'s DOM. Remembers which of its
 * descendants (tagged with FOCUS_KEY_ATTR) currently has focus, plus its
 * text selection if it is a text input, so restoreFocus can put focus back
 * on the equivalent freshly-built element.
 */
export function captureFocus(container: Element): CapturedFocus | null {
  const active = (container.ownerDocument ?? document).activeElement;
  if (!(active instanceof HTMLElement) || !container.contains(active)) return null;
  const key = active.getAttribute(FOCUS_KEY_ATTR);
  if (!key) return null;
  const selectable = isTextSelectable(active);
  return {
    key,
    value: selectable ? active.value : null,
    selectionStart: selectable ? active.selectionStart : null,
    selectionEnd: selectable ? active.selectionEnd : null,
  };
}

/**
 * Call after rebuilding `container`'s DOM, with whatever captureFocus
 * returned beforehand. No-op when nothing was captured or the key no longer
 * exists (the control it belonged to was removed by the new snapshot).
 * Returns whether focus was actually restored, so a caller can fall back to
 * ./focusOnSectionChange when it was not (nothing was focused before, or the
 * focused control is gone).
 */
export function restoreFocus(container: Element, captured: CapturedFocus | null): boolean {
  if (!captured) return false;
  const next = container.querySelector<HTMLElement>(`[${FOCUS_KEY_ATTR}="${cssEscape(captured.key)}"]`);
  if (!next) return false;
  next.focus();
  if (isTextSelectable(next)) {
    // Only overwrite the freshly-built value when it actually differs (a
    // view whose snapshot already carries the typed text, like the
    // wizard's access-key field, rebuilds with the same value already in
    // place; this is a no-op there and only matters for a field with no
    // such backing state, see CapturedFocus.value above).
    if (captured.value !== null && next.value !== captured.value) next.value = captured.value;
    if (captured.selectionStart !== null && captured.selectionEnd !== null) {
      try {
        next.setSelectionRange(captured.selectionStart, captured.selectionEnd);
      } catch {
        // Some input types throw on setSelectionRange even after the type
        // check above (engine-specific); losing the caret position is a
        // much smaller problem than losing focus entirely, so just keep
        // focus (and the restored value, set above).
      }
    }
  }
  return true;
}

function cssEscape(value: string): string {
  const w = (globalThis as { CSS?: { escape?(v: string): string } }).CSS;
  if (w?.escape) return w.escape(value);
  // Minimal fallback for test environments without window.CSS.escape: the
  // focus keys this codebase uses are always plain identifiers
  // ([a-zA-Z0-9_-]), so a literal match is exact for every real caller.
  return value.replace(/[^a-zA-Z0-9_-]/g, "\\$&");
}

/**
 * Moves focus into `container` when the logical "section" being shown
 * changes (a wizard screen, a workflow's explain view, ...), so a keyboard
 * or screen-reader user lands somewhere meaningful in the new content
 * instead of staying wherever focus was before (often <body>, on first
 * mount). Call on every render; it only acts when `sectionId` differs from
 * the previous call's for this same `container` (tracked in a WeakMap keyed
 * by the container element itself, so independent mounts, and independent
 * tests each creating their own container, never share or leak this state).
 *
 * Focuses the first element matching `focusSelector` inside `container`
 * (default: a heading), falling back to `container` itself if nothing
 * matches. The target must be focusable (a heading needs `tabindex="-1"`);
 * callers own adding that attribute since only they know their markup.
 */
const lastSectionByContainer = new WeakMap<Element, string | null>();

export function focusOnSectionChange(container: HTMLElement, sectionId: string, focusSelector = "[data-op-section-focus]"): void {
  const last = lastSectionByContainer.has(container) ? lastSectionByContainer.get(container)! : null;
  lastSectionByContainer.set(container, sectionId);
  if (last === sectionId) return;
  const target = container.querySelector<HTMLElement>(focusSelector) ?? container;
  target.focus();
}

function focusableElements(root: HTMLElement): HTMLElement[] {
  const selector = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled])",
    "select:not([disabled])",
    "textarea:not([disabled])",
    '[tabindex]:not([tabindex="-1"])',
  ].join(",");
  return Array.from(root.querySelectorAll<HTMLElement>(selector)).filter((el) => !el.hasAttribute("hidden") && el.getClientRects !== undefined);
}

/**
 * Traps Tab/Shift+Tab inside `root` for as long as the returned function has
 * not been called: Tab on the last focusable element wraps to the first,
 * Shift+Tab on the first wraps to the last. Required for any `role="dialog"
 * aria-modal="true"` surface (WAI-ARIA APG modal dialog pattern); without
 * it, Tab walks out of the dialog into whatever the modal backdrop is
 * covering. Call once per mount (not per render) and call the returned
 * cleanup function when the dialog is torn down.
 */
export function trapFocus(root: HTMLElement): () => void {
  function onKeydown(event: KeyboardEvent): void {
    if (event.key !== "Tab") return;
    const focusable = focusableElements(root);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = root.ownerDocument.activeElement;

    if (event.shiftKey) {
      if (active === first || !root.contains(active)) {
        event.preventDefault();
        last.focus();
      }
    } else {
      if (active === last || !root.contains(active)) {
        event.preventDefault();
        first.focus();
      }
    }
  }

  root.addEventListener("keydown", onKeydown);
  return () => root.removeEventListener("keydown", onKeydown);
}
