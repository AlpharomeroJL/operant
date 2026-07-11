// Default/Advanced mode switch. Default mode is the zero-code design bar
// (docs/ARCHITECTURE.md C19): no jargon, no code, no config file. Advanced
// mode is the explicit escape hatch a developer opts into. Nothing in this
// module renders copy; it only tracks and broadcasts which mode is active so
// screens can show or hide their Advanced surfaces (contracts/microcopy_glossary.json
// governs which strings are allowed where; see ui/src/strings and ui/src/advanced).

export type UiMode = "default" | "advanced";

const STORAGE_KEY = "operant.ui.mode";

type Listener = (mode: UiMode) => void;

function readInitialMode(): UiMode {
  try {
    const stored = typeof localStorage !== "undefined" ? localStorage.getItem(STORAGE_KEY) : null;
    return stored === "advanced" ? "advanced" : "default";
  } catch {
    return "default";
  }
}

export function createModeStore(initial: UiMode = readInitialMode()) {
  let mode: UiMode = initial;
  const listeners = new Set<Listener>();

  function get(): UiMode {
    return mode;
  }

  function set(next: UiMode): void {
    if (next === mode) return;
    mode = next;
    try {
      if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, mode);
    } catch {
      // Storage unavailable (private mode, sandboxed webview): mode still
      // works in memory for the life of the session.
    }
    for (const fn of listeners) fn(mode);
  }

  function toggle(): void {
    set(mode === "default" ? "advanced" : "default");
  }

  function subscribe(fn: Listener): () => void {
    listeners.add(fn);
    return () => listeners.delete(fn);
  }

  return { get, set, toggle, subscribe };
}

export type ModeStore = ReturnType<typeof createModeStore>;

// One shared instance for the running app. Tests build isolated instances via
// createModeStore(...) instead of importing this, so they never share state
// or a localStorage key with each other.
export const modeStore = createModeStore();
