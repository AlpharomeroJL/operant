// DOM mount for the grant prompt (docs/specs/ui.md: "grant prompt (sentence
// list plus Allow/Deny)"). Pure DOM, no bus: same split as
// ui/src/render/workflowView.ts (callbacks in, elements out).

import type { GrantPromptSnapshot } from "./state.ts";

export interface GrantPromptMountOptions {
  onAllow?: () => void;
  onDeny?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** Mount the grant prompt into `container`. Clears the container first so it can be re-mounted on updates (a new snapshot after Allow/Deny disables both buttons). */
export function mountGrantPrompt(
  container: HTMLElement,
  snapshot: GrantPromptSnapshot,
  opts: GrantPromptMountOptions = {},
): HTMLElement {
  container.textContent = "";
  const root = el("section", "op-grant-prompt");
  root.setAttribute("role", "alertdialog");
  root.setAttribute("aria-labelledby", "op-grant-prompt-title");

  const title = el("h2", "op-panel__title", snapshot.title);
  title.id = "op-grant-prompt-title";
  root.append(title);

  const list = el("ul", "op-grant-prompt__list");
  for (const sentence of snapshot.sentences) {
    list.append(el("li", "op-grant-prompt__item", sentence));
  }
  root.append(list);

  const actions = el("div", "op-change-card__actions");
  const allow = el("button", "op-button op-button--primary", snapshot.allowLabel);
  allow.type = "button";
  allow.disabled = snapshot.status !== "pending";
  if (opts.onAllow) allow.addEventListener("click", opts.onAllow);

  const deny = el("button", "op-button", snapshot.denyLabel);
  deny.type = "button";
  deny.disabled = snapshot.status !== "pending";
  if (opts.onDeny) deny.addEventListener("click", opts.onDeny);

  actions.append(allow, deny);
  root.append(actions);

  container.append(root);
  return root;
}
