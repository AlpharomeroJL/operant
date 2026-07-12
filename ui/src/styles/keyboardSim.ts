// Keyboard-only interaction simulation for tests (X8 app-accessibility).
// jsdom (this project's test DOM, see ./testDomEnv.ts) has no rendering
// engine, so it never performs a real browser's *default actions* for a key
// press: pressing Tab does not move focus, and pressing Enter/Space on a
// focused button does not click it. A real browser does both without any
// JavaScript involved. This module is the test-only stand-in for that
// default-action behavior, so a test can drive the app by dispatching real
// KeyboardEvents and asking "what would the browser do next" instead of
// reaching for element.focus()/element.click() directly (which would just
// prove the DOM API works, not that the screen is actually operable by
// keyboard).
//
// Any app code that wants non-default Tab/Enter behavior (ui/src/wizard/view.ts's
// focus trap, ui/src/wizard/view.ts's Escape-to-cancel) does so exactly as a
// real browser page would: an event listener that calls
// event.preventDefault(). This module dispatches a real, cancelable
// KeyboardEvent first and only falls back to its own generic default action
// when nothing called preventDefault, so app-level interception is honored
// rather than bypassed.

function isVisible(el: Element): boolean {
  let node: Element | null = el;
  while (node) {
    if (node instanceof HTMLElement && node.hidden) return false;
    node = node.parentElement;
  }
  return true;
}

function isDisabled(el: Element): boolean {
  return "disabled" in el && Boolean((el as unknown as { disabled: boolean }).disabled);
}

/** Every element a Tab key press could land on, in DOM order, restricted to what a real browser would actually consider focusable and visible. */
export function focusableElements(doc: Document): HTMLElement[] {
  const selector = ['a[href]', 'button', 'input', 'select', 'textarea', '[tabindex]'].join(",");
  return Array.from(doc.querySelectorAll<HTMLElement>(selector)).filter((el) => {
    if (isDisabled(el)) return false;
    if (el.tabIndex < 0) return false;
    if (el instanceof HTMLInputElement && el.type === "hidden") return false;
    return isVisible(el);
  });
}

/**
 * Simulates pressing Tab (or Shift+Tab). Dispatches a real, cancelable
 * `keydown` on the currently focused element (or `document.body` if
 * nothing is focused) so an app-level focus trap gets first say, exactly as
 * happens in a real browser; only moves focus itself (to the next/previous
 * focusable element in document order, wrapping at the ends) when nothing
 * intercepted the event.
 */
export function pressTab(doc: Document, opts: { shift?: boolean } = {}): void {
  const win = doc.defaultView;
  if (!win) throw new Error("pressTab needs a document with a defaultView (use ./testDomEnv.ts's createDomEnv)");
  const active = (doc.activeElement as HTMLElement | null) ?? doc.body;
  const event = new win.KeyboardEvent("keydown", { key: "Tab", shiftKey: Boolean(opts.shift), bubbles: true, cancelable: true });
  const notPrevented = active.dispatchEvent(event);
  if (!notPrevented) return; // an app-level listener (e.g. a modal's focus trap) already moved focus

  const focusables = focusableElements(doc);
  if (focusables.length === 0) return;
  const currentIndex = focusables.indexOf(active);
  let nextIndex: number;
  if (currentIndex === -1) {
    nextIndex = opts.shift ? focusables.length - 1 : 0;
  } else {
    nextIndex = opts.shift ? currentIndex - 1 : currentIndex + 1;
  }
  if (nextIndex < 0) nextIndex = focusables.length - 1;
  if (nextIndex >= focusables.length) nextIndex = 0;
  focusables[nextIndex].focus();
}

/**
 * Simulates pressing Enter or Space on `el` (which must already be
 * focused, the same precondition a real key press has). Dispatches a real
 * `keydown` first; if nothing intercepts it, applies the same default
 * action a browser gives a focused control: Enter or Space activates a
 * button, Enter in a text field submits its form (native <form> behavior),
 * and Space toggles a checkbox/radio.
 */
export function pressActivate(doc: Document, el: HTMLElement, key: "Enter" | " " = "Enter"): void {
  const win = doc.defaultView;
  if (!win) throw new Error("pressActivate needs a document with a defaultView");
  const event = new win.KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  const notPrevented = el.dispatchEvent(event);
  if (!notPrevented) return;

  const isButtonLike = el instanceof HTMLButtonElement || el.getAttribute("role") === "button";
  if (isButtonLike) {
    el.click();
    return;
  }
  if (el instanceof HTMLInputElement && (el.type === "radio" || el.type === "checkbox") && key === " ") {
    el.checked = true;
    el.dispatchEvent(new win.Event("change", { bubbles: true }));
    return;
  }
  if (el instanceof HTMLInputElement && key === "Enter") {
    const form = el.closest("form");
    form?.dispatchEvent(new win.Event("submit", { bubbles: true, cancelable: true }));
  }
}

/** Simulates pressing Escape on `el` (or `document` if omitted): a real, cancelable, bubbling `keydown`, for an app's own Escape-to-close/cancel listener to handle. */
export function pressEscape(doc: Document, el?: HTMLElement): void {
  const win = doc.defaultView;
  if (!win) throw new Error("pressEscape needs a document with a defaultView");
  const target: HTMLElement = el ?? (doc.activeElement as HTMLElement | null) ?? doc.body;
  const event = new win.KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  target.dispatchEvent(event);
}

/** Types `text` into a focused text input one character at a time, firing a real `input` event per character, the same as a real keyboard would, so any per-keystroke re-render (this lane's focus-preservation fix, ui/src/styles/focusPreserve.ts) is exercised the way it actually happens rather than as one bulk value assignment. */
export function typeText(doc: Document, input: HTMLInputElement, text: string): void {
  const win = doc.defaultView;
  if (!win) throw new Error("typeText needs a document with a defaultView");
  for (const ch of text) {
    input.value += ch;
    input.dispatchEvent(new win.Event("input", { bubbles: true }));
  }
}
