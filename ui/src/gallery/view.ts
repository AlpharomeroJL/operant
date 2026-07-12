// DOM mount for the template gallery (browse cards, one-click install).
// Pure DOM, no bus: same split as ui/src/library/view.ts. The install
// preview reuses ui/src/grants/view.ts's grant prompt mount for the
// Allow/Deny permission section instead of drawing a second one.

import type { GallerySnapshot, GalleryCard } from "./state.ts";
import { mountGrantPrompt } from "../grants/view.ts";

export interface GalleryMountOptions {
  onInstall?: (name: string) => void;
  onAllow?: () => void;
  onDeny?: () => void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function cardEl(card: GalleryCard, opts: GalleryMountOptions): HTMLElement {
  const root = el("article", "op-gallery-card");
  root.setAttribute("aria-label", card.title);

  root.append(el("h3", "op-gallery-card__name", card.title));
  if (card.summary && card.summary !== card.title) {
    root.append(el("p", "op-gallery-card__summary", card.summary));
  }
  root.append(el("p", "op-gallery-card__publisher", card.publisher));

  const install = el("button", "op-button op-button--primary", card.installLabel);
  install.type = "button";
  install.disabled = card.installed;
  if (opts.onInstall) install.addEventListener("click", () => opts.onInstall?.(card.name));
  root.append(install);

  return root;
}

function previewEl(snapshot: GallerySnapshot, opts: GalleryMountOptions): HTMLElement | undefined {
  const preview = snapshot.preview;
  if (!preview) return undefined;

  const root = el("section", "op-gallery-preview");
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-label", preview.title);

  root.append(el("h3", "op-panel__title", preview.title));
  root.append(el("p", "op-gallery-preview__publisher", preview.publisher));

  if (preview.stepLines.length) {
    root.append(el("h4", "op-panel__subtitle", preview.stepsHeading));
    const ol = el("ol", "op-gallery-preview__steps");
    for (const line of preview.stepLines) ol.append(el("li", "op-gallery-preview__step", line));
    root.append(ol);
  }

  root.append(el("p", "op-gallery-preview__trust", preview.trustNote));

  const grantMount = el("div", "op-gallery-preview__grant");
  mountGrantPrompt(grantMount, preview.grant, { onAllow: opts.onAllow, onDeny: opts.onDeny });
  root.append(grantMount);

  return root;
}

/** Mount the gallery into `container`. Clears the container first so it can be re-mounted on updates (a new card, a preview opening or closing). */
export function mountGallery(container: HTMLElement, snapshot: GallerySnapshot, opts: GalleryMountOptions = {}): HTMLElement {
  container.textContent = "";
  const root = el("section", "op-gallery");
  root.setAttribute("aria-labelledby", "op-gallery-heading");

  const heading = el("h2", "op-panel__title", snapshot.title);
  heading.id = "op-gallery-heading";
  root.append(heading);

  if (snapshot.notice) root.append(el("p", "op-notice", snapshot.notice));

  if (snapshot.error) {
    const errorEl = el("div", "op-error");
    errorEl.append(el("p", "op-error__title", snapshot.error.title));
    errorEl.append(el("p", "op-error__why", snapshot.error.why));
    errorEl.append(el("p", "op-error__action", snapshot.error.action));
    root.append(errorEl);
  }

  if (snapshot.empty) {
    root.append(el("p", "op-empty", snapshot.emptyLabel));
  } else {
    const grid = el("div", "op-gallery__grid");
    for (const card of snapshot.cards) grid.append(cardEl(card, opts));
    root.append(grid);
  }

  const preview = previewEl(snapshot, opts);
  if (preview) root.append(preview);

  container.append(root);
  return root;
}
