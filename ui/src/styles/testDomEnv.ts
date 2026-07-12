// Shared jsdom bootstrap for the accessibility tests under ui/src/palette,
// ui/src/wizard, ui/src/library, and ui/src/runViewer: a real DOM (jsdom) so
// axe-core can scan actual markup and keyboard-driving tests can call
// .focus()/.click() on real elements, instead of re-implementing DOM
// semantics under test. `node --test` has no DOM of its own (see
// ./testHooks.mjs for the companion .css-import shim every view.ts needs),
// so every test file that mounts a view.ts calls createDomEnv() first and
// env.cleanup() in a `finally` so globals never leak between tests running
// in the same process.

import { JSDOM } from "jsdom";

const GLOBAL_KEYS = [
  "window",
  "document",
  "navigator",
  "HTMLElement",
  "HTMLInputElement",
  "HTMLButtonElement",
  "HTMLSelectElement",
  "Element",
  "Node",
  "Event",
  "KeyboardEvent",
  "MouseEvent",
  "CustomEvent",
  "customElements",
  "localStorage",
  "Blob",
  "URL",
  "getComputedStyle",
  "DocumentFragment",
] as const;

export interface DomEnv {
  window: InstanceType<typeof JSDOM>["window"];
  document: Document;
  /** Restores whatever ui/src/styles/testDomEnv.ts's globals were before createDomEnv(), so tests can run back to back in one process. */
  cleanup(): void;
}

/** A fresh jsdom document with an empty #app mount point, wired onto globalThis so view.ts's plain `document.createElement` calls work unmodified. */
export function createDomEnv(bodyHtml = '<div id="app"></div>'): DomEnv {
  const dom = new JSDOM(`<!doctype html><html><body>${bodyHtml}</body></html>`, {
    url: "http://localhost/",
    pretendToBeVisual: true,
  });

  const previous = new Map<string, unknown>();
  for (const key of GLOBAL_KEYS) {
    previous.set(key, (globalThis as Record<string, unknown>)[key]);
    const value = (dom.window as unknown as Record<string, unknown>)[key];
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }

  return {
    window: dom.window,
    document: dom.window.document,
    cleanup(): void {
      for (const key of GLOBAL_KEYS) {
        Object.defineProperty(globalThis, key, { value: previous.get(key), configurable: true, writable: true });
      }
    },
  };
}
