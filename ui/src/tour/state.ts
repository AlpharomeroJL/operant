// First-run tour and contextual hints. The tour walks a first-time user
// through palette -> run viewer -> library with short callouts. Contextual
// hints are small one-line tips attached to specific controls that retire
// (never show again) after the user first succeeds at the action the hint
// describes. Persist "seen/retired" state the same way ui/src/wizard or
// ui/src/settings persist local state.

export type TourStep = "palette" | "runViewer" | "library" | "done";

export interface TourSnapshot {
  step: TourStep;
  completed: boolean;
  retiredHints: Set<string>;
}

interface Listener {
  (snap: TourSnapshot): void;
}

const STORAGE_KEY = "operant.ui.tour";
const RETIRED_HINTS_KEY = "operant.ui.tour.retired-hints";

function readInitialStep(): TourStep {
  try {
    if (typeof localStorage === "undefined") return "palette";
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored || !["palette", "runViewer", "library", "done"].includes(stored)) {
      return "palette";
    }
    return stored as TourStep;
  } catch {
    return "palette";
  }
}

function readRetiredHints(): Set<string> {
  try {
    if (typeof localStorage === "undefined") return new Set();
    const stored = localStorage.getItem(RETIRED_HINTS_KEY);
    if (!stored) return new Set();
    return new Set(JSON.parse(stored) as string[]);
  } catch {
    return new Set();
  }
}

export function createTourStore(initial: TourStep = readInitialStep()) {
  let step: TourStep = initial;
  let retiredHints = readRetiredHints();
  const listeners = new Set<Listener>();

  function getSnapshot(): TourSnapshot {
    return {
      step,
      completed: step === "done",
      retiredHints: new Set(retiredHints),
    };
  }

  function nextStep(): void {
    const steps: TourStep[] = ["palette", "runViewer", "library", "done"];
    const currentIndex = steps.indexOf(step);
    if (currentIndex < steps.length - 1) {
      step = steps[currentIndex + 1];
      persist();
      emit();
    }
  }

  function retireHint(hintId: string): void {
    if (!retiredHints.has(hintId)) {
      retiredHints.add(hintId);
      persistRetiredHints();
      emit();
    }
  }

  function isHintRetired(hintId: string): boolean {
    return retiredHints.has(hintId);
  }

  function reset(): void {
    step = "palette";
    retiredHints.clear();
    persist();
    persistRetiredHints();
    emit();
  }

  function persist(): void {
    try {
      if (typeof localStorage !== "undefined") {
        localStorage.setItem(STORAGE_KEY, step);
      }
    } catch {
      // Storage unavailable (private mode, sandboxed webview): state still
      // works in memory for the life of the session.
    }
  }

  function persistRetiredHints(): void {
    try {
      if (typeof localStorage !== "undefined") {
        localStorage.setItem(RETIRED_HINTS_KEY, JSON.stringify(Array.from(retiredHints)));
      }
    } catch {
      // Storage unavailable
    }
  }

  function emit(): void {
    const snap = getSnapshot();
    for (const fn of listeners) fn(snap);
  }

  function subscribe(fn: Listener): () => void {
    listeners.add(fn);
    return () => listeners.delete(fn);
  }

  function dispose(): void {
    listeners.clear();
  }

  return {
    getSnapshot,
    nextStep,
    retireHint,
    isHintRetired,
    reset,
    subscribe,
    dispose,
  };
}

export type TourStore = ReturnType<typeof createTourStore>;

// One shared instance for the running app. Tests build isolated instances via
// createTourStore(...) instead of importing this, so they never share state
// or a localStorage key with each other.
export const tourStore = createTourStore();
