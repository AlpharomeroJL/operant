// DOM mount for the tray preview: the glyph trigger button, its click-to-
// open menu (design.md section 3, Tray), and its notifications. Pure DOM, no
// bus: same split as ui/src/render/workflowView.ts. This is an in-page
// preview only: the real OS tray icon, its real menu, and OS notification
// toasts are ui/src-tauri's job (out of this lane's owned path, ui/src
// only). This is what drives them, and what shows while running in a plain
// browser tab during development.

import type { TraySnapshot } from "./state.ts";
import { trayNotificationStrings } from "./strings.ts";

export interface TrayMountOptions {
  onDismissNotification?: (id: string) => void;
  onToggleMenu?: () => void;
  onCloseMenu?: () => void;
  onQuickRun?: (name: string) => void;
  onOpen?: () => void;
  onPauseAll?: () => void;
  onPanic?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function menuItem(className: string, text: string, onClick?: () => void): HTMLButtonElement {
  const button = el("button", `op-tray__menu-item${className ? ` ${className}` : ""}`, text);
  button.type = "button";
  button.setAttribute("role", "menuitem");
  if (onClick) button.addEventListener("click", onClick);
  return button;
}

/** design.md section 3: "Menu: the top three frecent workflows as one-click Quick Runs, then Open, Pause all, and a panic row." */
function buildMenu(snapshot: TraySnapshot, opts: TrayMountOptions): HTMLElement {
  const menu = el("div", "op-tray__menu");
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", snapshot.menuLabel);
  // Escape closes the menu without acting on anything, the same convenience
  // ui/src/palette/view.ts's own keydown listener offers its overlay.
  menu.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      opts.onCloseMenu?.();
    }
  });

  const quickSection = el("div", "op-tray__menu-section");
  quickSection.setAttribute("role", "group");
  quickSection.setAttribute("aria-label", snapshot.quickRunsTitle);
  quickSection.append(el("p", "op-tray__menu-heading", snapshot.quickRunsTitle));
  if (snapshot.quickRuns.length) {
    for (const run of snapshot.quickRuns) {
      quickSection.append(menuItem("op-tray__menu-item--quickrun", run.title, () => opts.onQuickRun?.(run.name)));
    }
  } else {
    quickSection.append(el("p", "op-tray__menu-empty", snapshot.quickRunsEmptyLabel));
  }
  menu.append(quickSection);

  menu.append(el("div", "op-tray__menu-divider"));
  menu.append(menuItem("", snapshot.openLabel, () => opts.onOpen?.()));

  const pauseAllButton = menuItem("", snapshot.pauseAllLabel, () => opts.onPauseAll?.());
  pauseAllButton.disabled = !snapshot.canPauseAll;
  menu.append(pauseAllButton);

  menu.append(el("div", "op-tray__menu-divider"));
  const panicButton = menuItem("op-tray__menu-item--panic", snapshot.panicLabel, () => opts.onPanic?.());
  panicButton.title = snapshot.panicHint;
  menu.append(panicButton);

  return menu;
}

export function mountTray(container: HTMLElement, snapshot: TraySnapshot, opts: TrayMountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("div", "op-tray");

  // The glyph itself is the menu's trigger (docs/ARCHITECTURE.md's C20:
  // "global panic hotkey plus tray button", i.e. the tray icon IS a
  // button). Its accessible name folds in the current state and the
  // saved-time tooltip together, the same two pieces the pre-redesign
  // sibling visually-hidden span used to carry; a native title gives
  // sighted mouse users the same text on hover.
  const accessibleName = `${snapshot.glyphLabel}. ${snapshot.tooltip}`;
  const trigger = el("button", "op-tray__trigger");
  trigger.type = "button";
  trigger.setAttribute("aria-haspopup", "true");
  trigger.setAttribute("aria-expanded", String(snapshot.menuOpen));
  trigger.setAttribute("aria-label", `${accessibleName}. ${snapshot.menuLabel}`);
  trigger.title = accessibleName;
  trigger.addEventListener("click", () => opts.onToggleMenu?.());

  const glyph = el("span", "op-tray__glyph");
  glyph.dataset.state = snapshot.glyph;
  glyph.setAttribute("aria-hidden", "true");
  trigger.append(glyph);
  root.append(trigger);

  if (snapshot.menuOpen) {
    root.append(buildMenu(snapshot, opts));
  }

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
