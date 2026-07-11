// DOM mount for the tray preview and its notifications. Pure DOM, no bus:
// same split as ui/src/render/workflowView.ts. This is an in-page preview
// only: the real OS tray icon and OS notification toasts are
// ui/src-tauri's job (out of this lane's owned path, ui/src only). This is
// what drives them, and what shows while running in a plain browser tab
// during development.

import type { TraySnapshot } from "./state.ts";
import { trayNotificationStrings } from "./strings.ts";

export interface TrayMountOptions {
  onDismissNotification?: (id: string) => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function mountTray(container: HTMLElement, snapshot: TraySnapshot, opts: TrayMountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("div", "op-tray");

  const glyph = el("span", "op-tray__glyph");
  glyph.dataset.state = snapshot.glyph;
  glyph.setAttribute("aria-hidden", "true");
  glyph.title = snapshot.tooltip;
  const label = el("span", "op-visually-hidden", `${snapshot.glyphLabel}. ${snapshot.tooltip}`);
  root.append(glyph, label);

  if (snapshot.notifications.length) {
    const list = el("ul", "op-tray__notifications");
    for (const n of snapshot.notifications) {
      const item = el("li", "op-tray__notification");
      item.append(el("strong", "op-tray__notification-title", n.title));
      item.append(el("span", "op-tray__notification-body", ` ${n.body}`));
      const dismiss = el("button", "op-button", trayNotificationStrings.dismiss);
      dismiss.type = "button";
      const id = n.id;
      if (opts.onDismissNotification) {
        dismiss.addEventListener("click", () => opts.onDismissNotification?.(id));
      }
      item.append(dismiss);
      list.append(item);
    }
    root.append(list);
  }

  container.append(root);
  return root;
}
