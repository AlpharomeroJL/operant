// "Recents ranked by frecency" (docs/specs/design.md section 3, Palette): a
// blend of how often and how recently a workflow, quick action, or settings
// entry was picked from the palette. Pure scoring function plus a small
// localStorage-backed store, the same try/catch-with-in-memory-fallback
// pattern as ui/src/state/mode.ts (a sandboxed webview with no storage just
// starts every entry back at zero instead of throwing).

export interface FrecencyEntry {
  id: string;
  count: number;
  lastUsedAt: number;
}

const HOUR_MS = 3_600_000;
const DAY_MS = 24 * HOUR_MS;

/**
 * A places-style bucketed recency weight (the same shape Firefox's frecency
 * uses): a pick in the last four hours counts far more than one from last
 * month, but never drops to literally zero, so a single older favorite can
 * still outrank something never picked at all.
 */
function recencyWeight(ageMs: number): number {
  if (ageMs < 4 * HOUR_MS) return 100;
  if (ageMs < DAY_MS) return 70;
  if (ageMs < 3 * DAY_MS) return 50;
  if (ageMs < 7 * DAY_MS) return 30;
  if (ageMs < 14 * DAY_MS) return 10;
  return 1;
}

/**
 * `entry.count` (how many times it has been picked) times the bucketed
 * recency weight of its last pick. Two entries picked equally recently rank
 * by count; two entries picked equally often rank by recency; an entry
 * never picked (count 0) always scores 0, so it never outranks a real
 * recent pick. Exported (not just used internally by ./frecency.ts's own
 * store) so ui/src/palette/catalog.ts can blend it with a fuzzy match score
 * without reaching into the store's private map.
 */
export function frecencyScore(entry: Pick<FrecencyEntry, "count" | "lastUsedAt">, nowMs: number): number {
  if (entry.count <= 0) return 0;
  const ageMs = Math.max(0, nowMs - entry.lastUsedAt);
  return entry.count * recencyWeight(ageMs);
}

export interface FrecencyStore {
  /** Records one pick of `id` right now: bumps its count and refreshes lastUsedAt. */
  record(id: string): void;
  countOf(id: string): number;
  scoreOf(id: string): number;
  /** Every entry ever recorded, highest frecency score first. */
  all(): FrecencyEntry[];
  subscribe(fn: (entries: FrecencyEntry[]) => void): () => void;
}

export interface CreateFrecencyStoreOptions {
  now?: () => number;
  /** Distinct storage key per store instance, so tests never share ui/src/state/mode.ts-style persisted state with each other or with the real app. */
  storageKey?: string;
}

const DEFAULT_STORAGE_KEY = "operant.palette.frecency";

function readStored(key: string): Record<string, FrecencyEntry> {
  try {
    if (typeof localStorage === "undefined") return {};
    const raw = localStorage.getItem(key);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? (parsed as Record<string, FrecencyEntry>) : {};
  } catch {
    return {};
  }
}

function writeStored(key: string, entries: Record<string, FrecencyEntry>): void {
  try {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(key, JSON.stringify(entries));
  } catch {
    // Storage unavailable (private mode, sandboxed webview): frecency still
    // works in memory for the life of this session.
  }
}

export function createFrecencyStore(opts: CreateFrecencyStoreOptions = {}): FrecencyStore {
  const now = opts.now ?? (() => Date.now());
  const storageKey = opts.storageKey ?? DEFAULT_STORAGE_KEY;
  const entries = new Map<string, FrecencyEntry>(Object.entries(readStored(storageKey)));
  const listeners = new Set<(entries: FrecencyEntry[]) => void>();

  function all(): FrecencyEntry[] {
    return Array.from(entries.values()).sort((a, b) => frecencyScore(b, now()) - frecencyScore(a, now()));
  }

  function persist(): void {
    writeStored(storageKey, Object.fromEntries(entries));
  }

  function emit(): void {
    const snap = all();
    for (const fn of listeners) fn(snap);
  }

  return {
    record(id) {
      const existing = entries.get(id);
      entries.set(id, { id, count: (existing?.count ?? 0) + 1, lastUsedAt: now() });
      persist();
      emit();
    },
    countOf(id) {
      return entries.get(id)?.count ?? 0;
    },
    scoreOf(id) {
      const entry = entries.get(id);
      return entry ? frecencyScore(entry, now()) : 0;
    },
    all,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}
