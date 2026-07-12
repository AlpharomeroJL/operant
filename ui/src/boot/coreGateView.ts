// DOM for the three non-app boot states (contracts/ipc.md section 3): the
// blocking screen shown when the core cannot automate, the error screen shown
// when the handshake fails, and the persistent Demo banner. Pure DOM, no bus
// and no Tauri import, same state/view split as ui/src/toasts/view.ts. The
// user-facing copy here uses plain language; the only wire tokens shown are the
// capability field names, which the contract requires be named so the failure
// is legible to whoever built or installed the core.

import "./coreGate.css";
import type { MissingCapability } from "./coreGate.ts";

const bootStrings = {
  demoBanner: "Demo: canned example, not your computer",
  blockedHeading: "This build cannot control your computer",
  blockedBody:
    "It can show you the interface but cannot act on your machine. Reinstall a full release build, or rebuild the core with real input and live perception turned on.",
  blockedListIntro: "What this build is missing:",
  errorHeading: "Could not reach the Operant core",
  errorBody:
    "The background engine did not answer the connection check. Operant will not show you a pretend run in its place. Restart the app to try again.",
  errorDetailLabel: "Technical detail",
};

// One plain sentence per missing capability, each naming its wire field so a
// developer can act on it (contracts/ipc.md section 3: "enumerates each false
// capability by its field name so the failure is legible").
const CAPABILITY_TEXT: Record<MissingCapability["field"], string> = {
  real_uia: "live perception of what is on your screen (real_uia=false)",
  real_input: "real keyboard and mouse control (real_input=false)",
};

/**
 * The persistent Demo banner, prepended once inside the app shell. Idempotent
 * so a remount never stacks a second banner. Non-interactive (role="status"),
 * so it never joins the tab order.
 */
export function mountDemoBanner(root: HTMLElement): HTMLElement {
  const app = root.querySelector<HTMLElement>(".op-app") ?? root;
  const existing = app.querySelector<HTMLElement>(".op-demo-banner");
  if (existing) return existing;

  const banner = document.createElement("div");
  banner.className = "op-demo-banner";
  banner.setAttribute("role", "status");
  banner.textContent = bootStrings.demoBanner;
  app.insertBefore(banner, app.firstChild);
  return banner;
}

/**
 * The blocking screen: replaces any prior content in `root` (so no real-work UI
 * survives behind it) and lists every missing automation capability by field.
 */
export function renderBlockingScreen(root: HTMLElement, missing: MissingCapability[]): HTMLElement {
  const screen = replaceWithScreen(root, "op-boot-screen--blocked");

  appendTitle(screen, bootStrings.blockedHeading);
  appendBody(screen, bootStrings.blockedBody);
  appendBody(screen, bootStrings.blockedListIntro);

  const list = document.createElement("ul");
  list.className = "op-boot-screen__list";
  for (const cap of missing) {
    const item = document.createElement("li");
    item.dataset.capability = cap.field;
    item.textContent = CAPABILITY_TEXT[cap.field];
    list.append(item);
  }
  screen.append(list);
  return screen;
}

/**
 * The error screen: replaces any prior content in `root`. Shows fixed human
 * copy and, when present, the raw failure as secondary technical detail. Never
 * renders any real-work UI or canned data.
 */
export function renderErrorScreen(root: HTMLElement, detail?: string): HTMLElement {
  const screen = replaceWithScreen(root, "op-boot-screen--error");

  appendTitle(screen, bootStrings.errorHeading);
  appendBody(screen, bootStrings.errorBody);

  const trimmed = detail?.trim();
  if (trimmed) {
    const detailEl = document.createElement("p");
    detailEl.className = "op-boot-screen__detail";
    detailEl.textContent = `${bootStrings.errorDetailLabel}: ${trimmed}`;
    screen.append(detailEl);
  }
  return screen;
}

function replaceWithScreen(root: HTMLElement, modifier: string): HTMLElement {
  root.textContent = "";
  const screen = document.createElement("section");
  screen.className = `op-boot-screen ${modifier}`;
  screen.setAttribute("role", "alert");
  screen.setAttribute("aria-live", "assertive");
  root.append(screen);
  return screen;
}

function appendTitle(parent: HTMLElement, text: string): void {
  const heading = document.createElement("h1");
  heading.className = "op-boot-screen__title";
  heading.textContent = text;
  parent.append(heading);
}

function appendBody(parent: HTMLElement, text: string): void {
  const body = document.createElement("p");
  body.className = "op-boot-screen__body";
  body.textContent = text;
  parent.append(body);
}
