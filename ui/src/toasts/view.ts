// DOM mount for the bottom-right toast (docs/specs/design.md section 3's
// Toasts: "Bottom-right, one line, verb-first... Amber only when an action
// is invited"). Pure DOM, no bus: same split as ui/src/tray/view.ts.
//
// Markup and class names (.op-toast/.op-toast__message/.op-toast__action)
// are exactly what F1 first wrote inline in ui/src/main.ts and
// ui/src/styles/base.css; this module moves the DOM-building logic here
// unchanged rather than duplicating a second set of toast styles.

import type { ToastSnapshot } from "./state.ts";

export interface ToastMountOptions {
  /** Fired when the toast's own action is clicked, with the run it applies
   * to. Never fired for a message-only toast, which renders no action
   * element to click. */
  onAction?: (runId: string) => void;
}

export function mountToast(container: HTMLElement, snapshot: ToastSnapshot, opts: ToastMountOptions = {}): HTMLElement {
  container.textContent = "";
  if (!snapshot.toast) return container;
  const { message, action, runId } = snapshot.toast;

  const toastEl = document.createElement("div");
  toastEl.className = "op-toast";
  // A status region, not an alert: a toast reports something that already
  // happened rather than demanding attention (design.md's Toasts are calm,
  // not modal).
  toastEl.setAttribute("role", "status");

  const text = document.createElement("span");
  text.className = "op-toast__message";
  text.textContent = message;
  toastEl.append(text);

  // Amber only because an action is invited (design.md section 1's
  // one-signal-color rule): a message-only toast renders no button at all,
  // so there is nothing left to paint amber.
  if (action && runId) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "op-toast__action";
    button.textContent = action.label;
    button.addEventListener("click", () => opts.onAction?.(runId));
    toastEl.append(button);
  }

  container.append(toastEl);
  return container;
}
