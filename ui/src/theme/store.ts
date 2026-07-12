// Dark/light/system theme switching (docs/specs/design.md section 3's
// Settings > Appearance: "dark/light/system"). Applying a theme means
// setting <html data-theme="dark|light">, which ui/src/styles/tokens.css's
// explicit [data-theme] overrides win with over the OS's own
// prefers-color-scheme in either direction; every screen reads the same CSS
// custom properties (tokens.css), so one attribute change re-themes the
// whole shell at once, with no per-screen wiring required.
//
// Mirrors ui/src/state/mode.ts's localStorage-with-in-memory-fallback
// pattern (same reasoning: a sandboxed webview with no storage should not
// throw, it should just not persist the choice across launches).
//
// design.md section 2's "nothing animates on load" extends to theme
// switching too: applyToDocument sets the attribute directly with no
// transition of its own; ui/src/styles/base.css never transitions a color
// property, only layout/opacity affordances a person's own action
// triggers (hover, a toggle's pressed state, a progress bar's width), so a
// theme flip repaints instantly rather than crossfading.

export type ThemeMode = "dark" | "light" | "system";
export type ResolvedTheme = "dark" | "light";

const STORAGE_KEY = "operant.ui.theme";
// design.md section 1: dark is the default. "system" (not "dark") is this
// store's own out-of-the-box mode so a person whose OS is already set to
// light is not fought with a dark app on first run; resolve() below still
// lands on dark whenever the OS is dark, or its preference cannot be read
// at all (matchMedia unavailable), which is also ui/scripts/build-tokens.mjs
// generated tokens.css's own fallback (its bare :root block is dark), so the
// two only ever disagree while JS has not yet run.
const DEFAULT_MODE: ThemeMode = "system";

type Listener = (mode: ThemeMode, resolved: ResolvedTheme) => void;

function isThemeMode(value: unknown): value is ThemeMode {
  return value === "dark" || value === "light" || value === "system";
}

function readInitialMode(): ThemeMode {
  try {
    const stored = typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
    return isThemeMode(stored) ? stored : DEFAULT_MODE;
  } catch {
    return DEFAULT_MODE;
  }
}

/** `window.matchMedia`, not the bare global: in the jsdom test harness (ui/src/styles/testDomEnv.ts) only `window` itself is patched onto globalThis, not every one of its methods separately. In a real webview `window` is globalThis anyway, so this is identical there. */
function matchMediaSafe(query: string): MediaQueryList | null {
  try {
    const win = typeof window !== "undefined" ? window : undefined;
    if (!win || typeof win.matchMedia !== "function") return null;
    return win.matchMedia(query);
  } catch {
    // jsdom versions that stub matchMedia as "not implemented" throw; system
    // mode still resolves once below (dark, the safe fallback), it just will
    // not live-update if the OS preference changes mid-session.
    return null;
  }
}

function systemPrefersLight(): boolean {
  return matchMediaSafe("(prefers-color-scheme: light)")?.matches ?? false;
}

function resolve(mode: ThemeMode): ResolvedTheme {
  if (mode === "system") return systemPrefersLight() ? "light" : "dark";
  return mode;
}

function applyToDocument(resolved: ResolvedTheme): void {
  try {
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-theme", resolved);
    }
  } catch {
    // No document (e.g. a non-DOM context importing this module for its
    // mode logic only): nothing to paint, so nothing to do.
  }
}

export function createThemeStore(initial: ThemeMode = readInitialMode()) {
  let mode: ThemeMode = initial;
  const listeners = new Set<Listener>();
  let unwatchSystem: (() => void) | null = null;

  function notify(): void {
    const resolved = resolve(mode);
    applyToDocument(resolved);
    for (const fn of listeners) fn(mode, resolved);
  }

  function watchSystem(): void {
    unwatchSystem?.();
    unwatchSystem = null;
    if (mode !== "system") return;
    const mql = matchMediaSafe("(prefers-color-scheme: light)");
    if (!mql) return;
    try {
      const onChange = () => notify();
      mql.addEventListener("change", onChange);
      unwatchSystem = () => mql.removeEventListener("change", onChange);
    } catch {
      // addEventListener unsupported on this MediaQueryList stub: system
      // mode still resolved once, just will not live-update.
    }
  }

  function get(): ThemeMode {
    return mode;
  }

  function getResolved(): ResolvedTheme {
    return resolve(mode);
  }

  function set(next: ThemeMode): void {
    if (next === mode) return;
    mode = next;
    try {
      if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // Storage unavailable: mode still works in memory for the session.
    }
    watchSystem();
    notify();
  }

  /** Cycle dark -> light -> system -> dark: one compact header control needs only this, not a 3-way picker. */
  function cycle(): void {
    set(mode === "dark" ? "light" : mode === "light" ? "system" : "dark");
  }

  function subscribe(fn: Listener): () => void {
    listeners.add(fn);
    return () => listeners.delete(fn);
  }

  /** Applies the current mode to the document immediately and starts watching the OS if in system mode. Call once on startup. */
  function init(): void {
    watchSystem();
    notify();
  }

  function dispose(): void {
    unwatchSystem?.();
    unwatchSystem = null;
    listeners.clear();
  }

  return { get, getResolved, set, cycle, subscribe, init, dispose };
}

export type ThemeStore = ReturnType<typeof createThemeStore>;

// One shared instance for the running app. Tests build isolated instances via
// createThemeStore(...) instead of importing this, so they never share state,
// a localStorage key, or a document.documentElement mutation with each other.
export const themeStore = createThemeStore();
