// Turns the palette's typed query into grouped, ranked rows (docs/specs/
// design.md section 3, Palette: "fuzzy match over workflows, quick actions,
// and settings... Results grouped (Workflows, Actions, Recent)... Recents
// ranked by frecency... Typing a sentence that matches nothing offers
// 'Teach this' as the amber primary row"). Pure and DOM-free: ./state.ts
// wraps this with open/query/selection state, ./view.ts only ever renders
// whatever this produces. Same logic/DOM split as every other screen in
// ui/src (see ui/src/runViewer/state.ts's header comment).

import { fuzzyMatch, type FuzzyMatch } from "./fuzzy.ts";
import type { FrecencyStore } from "./frecency.ts";

/**
 * The three source categories design.md names ("fuzzy match over
 * workflows, quick actions, and settings"). Settings entries fold into the
 * "Actions" display group below: design.md's grouping only ever names
 * three buckets (Workflows, Actions, Recent), not four.
 */
export type PaletteEntryKind = "workflow" | "action" | "setting";

export interface PaletteEntry {
  id: string;
  kind: PaletteEntryKind;
  title: string;
  subtitle?: string;
  /** Extra fuzzy-matchable aliases (a workflow's raw slug, for example). The best-scoring of title/keywords wins; keywords never appear on screen themselves. */
  keywords?: string[];
  /** A per-row tooltip (e.g. reusing the existing "press Enter to..." hint); optional, view-only. */
  hint?: string;
}

export type PaletteRowKind = PaletteEntryKind | "teach";

export interface PaletteRow {
  id: string;
  kind: PaletteRowKind;
  title: string;
  subtitle?: string;
  hint?: string;
  /** Indices into `title` that matched the typed query (./fuzzy.ts's highlightSegments turns these into rendered spans). */
  highlight: FuzzyMatch["indices"];
}

export interface PaletteGroup {
  title: string;
  rows: PaletteRow[];
}

export interface PaletteResults {
  groups: PaletteGroup[];
  /** Every row across every group, top to bottom, in the exact order rendered: the flat list ui/src/palette/state.ts's arrow-key navigation walks. Equal to [teachRow] when teachRow is set. */
  rows: PaletteRow[];
  /** design.md section 3: "Typing a sentence that matches nothing offers 'Teach this'." Set only when the query is non-blank and nothing matched. */
  teachRow: PaletteRow | null;
}

export const TEACH_ROW_ID = "__teach__";

/**
 * The best match score across an entry's title and its keywords (an
 * invisible alias, e.g. a workflow's raw slug, "never appears on screen
 * itself" per PaletteEntry.keywords's own doc comment) together, so a
 * search for either surfaces the entry, plus the title's own match on its
 * own: a keyword can win the *ranking* (it is a real, deliberate alias) but
 * only the title's own indices are ever valid to highlight, since the
 * keyword string itself is never rendered. Returns null when neither the
 * title nor any keyword matched at all.
 */
function scoreEntry(query: string, entry: PaletteEntry): { rankScore: number; titleMatch: FuzzyMatch | null } | null {
  const titleMatch = fuzzyMatch(query, entry.title);
  let rankScore = titleMatch?.score ?? Number.NEGATIVE_INFINITY;
  for (const keyword of entry.keywords ?? []) {
    const m = fuzzyMatch(query, keyword);
    if (m && m.score > rankScore) rankScore = m.score;
  }
  if (rankScore === Number.NEGATIVE_INFINITY) return null;
  return { rankScore, titleMatch };
}

export interface MatchStrings {
  groupWorkflows: string;
  groupActions: string;
  groupRecent: string;
  teachThis: string;
  teachHint?: string;
}

interface Candidate {
  entry: PaletteEntry;
  /** Only set when the entry's own title matched (as opposed to only a keyword alias): the sole source of valid highlight indices, since a keyword string is never what is rendered on screen. */
  titleMatch: FuzzyMatch | null;
  /** The best score across title and keywords, plus the entry's current frecency score: the signal both the Recent group's membership test and every group's internal ranking use. */
  combined: number;
}

function toRow(c: Candidate): PaletteRow {
  return { id: c.entry.id, kind: c.entry.kind, title: c.entry.title, subtitle: c.entry.subtitle, hint: c.entry.hint, highlight: c.titleMatch?.indices ?? [] };
}

function compareByScore(a: Candidate, b: Candidate): number {
  return b.combined - a.combined || a.entry.title.localeCompare(b.entry.title);
}

/**
 * Ranks and groups `entries` against `query`. A blank query matches every
 * entry (./fuzzy.ts's fuzzyMatch("", x) is trivially true for every x), so
 * this same function also produces the palette's "nothing typed yet" root
 * view: no special-casing needed.
 *
 * `frecency` supplies design.md's "Recents ranked by frecency": any entry
 * ever picked before (frecency.countOf(id) > 0) is pulled out of its native
 * Workflows/Actions group into its own Recent group, so it is never shown
 * twice. Every group's internal order is fuzzy score plus frecency score
 * combined, so a strong exact match still beats a stale one-time pick, and
 * (crucially for a blank query, where every fuzzy score is 0) frecency
 * alone decides the Recent group's order.
 *
 * Groups render in the literal order design.md lists them: Workflows,
 * Actions, Recent. A group with no matches is omitted entirely rather than
 * rendered empty.
 */
export function matchEntries(entries: readonly PaletteEntry[], query: string, frecency: FrecencyStore, strings: MatchStrings): PaletteResults {
  const trimmed = query.trim();

  const matched: Candidate[] = [];
  for (const entry of entries) {
    const scored = scoreEntry(trimmed, entry);
    if (!scored) continue;
    matched.push({ entry, titleMatch: scored.titleMatch, combined: scored.rankScore + frecency.scoreOf(entry.id) });
  }

  if (matched.length === 0) {
    if (trimmed.length === 0) return { groups: [], rows: [], teachRow: null };
    const teachRow: PaletteRow = { id: TEACH_ROW_ID, kind: "teach", title: strings.teachThis, subtitle: trimmed, hint: strings.teachHint, highlight: [] };
    return { groups: [], rows: [teachRow], teachRow };
  }

  const recent = matched.filter((c) => frecency.countOf(c.entry.id) > 0).sort(compareByScore);
  const recentIds = new Set(recent.map((c) => c.entry.id));
  const rest = matched.filter((c) => !recentIds.has(c.entry.id));
  const workflows = rest.filter((c) => c.entry.kind === "workflow").sort(compareByScore);
  const actions = rest.filter((c) => c.entry.kind !== "workflow").sort(compareByScore);

  const groups: PaletteGroup[] = [];
  if (workflows.length > 0) groups.push({ title: strings.groupWorkflows, rows: workflows.map(toRow) });
  if (actions.length > 0) groups.push({ title: strings.groupActions, rows: actions.map(toRow) });
  if (recent.length > 0) groups.push({ title: strings.groupRecent, rows: recent.map(toRow) });

  return { groups, rows: groups.flatMap((g) => g.rows), teachRow: null };
}
