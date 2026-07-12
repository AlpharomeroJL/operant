// Fuzzy subsequence matching with word-boundary bonuses, plus a helper that
// turns matched character indices into highlight segments for rendering
// (docs/specs/design.md section 3, Palette: "fuzzy match over workflows,
// quick actions, and settings with match-character highlighting").
//
// Pure, DOM-free, dependency-free: ui/src/palette/catalog.ts is the only
// caller, and this file's own tests (./fuzzy.test.ts) run under plain
// `node --test`, no jsdom, the same split every module under ui/src/palette
// uses between logic and DOM (see ./state.ts's header comment).

export interface FuzzyMatch {
  /** Higher is a better match. Not normalized to any fixed range; only meaningful relative to another fuzzyMatch call against the same query. */
  score: number;
  /** Indices into `target` (not `query`) that matched a query character, ascending. */
  indices: number[];
}

// A boundary character immediately before a match: the start of a new
// "word" inside a title, a hyphenated workflow slug, or a file-path-like
// string.
const WORD_BOUNDARY_BEFORE = /[\s\-_/]/;

function isUpper(ch: string): boolean {
  return ch !== ch.toLowerCase() && ch === ch.toUpperCase();
}

/**
 * True when `target[index]` starts a new "word": the very first character,
 * right after a space/hyphen/underscore/slash, or a camelCase capital
 * following a lowercase letter. Powers the word-boundary bonus below, the
 * same heuristic popular fuzzy filters (VS Code's, Sublime's) use so an
 * acronym-style match ("ci" -> "Copy Invoice") outranks an equally long
 * match buried mid-word ("ci" inside "specific").
 */
function isWordBoundary(target: string, index: number): boolean {
  if (index === 0) return true;
  const prev = target[index - 1];
  const cur = target[index];
  if (WORD_BOUNDARY_BEFORE.test(prev)) return true;
  if (isUpper(cur) && !isUpper(prev)) return true;
  return false;
}

/**
 * Case-insensitive subsequence match: every character of `query`, in order,
 * appears somewhere in `target` (not necessarily contiguous). Returns null
 * when `query` is not a subsequence of `target` at all. An empty query
 * matches every target trivially, with a zero score and no highlighted
 * indices: ui/src/palette/catalog.ts's "nothing typed yet" root view relies
 * on this rather than special-casing a blank query itself.
 *
 * Scoring rewards, in order: the match itself; an unbroken consecutive run
 * (a real substring match beats the same letters scattered across the
 * string); starting right at a word boundary; starting early in the
 * string; and finishing inside a shorter target (the same letters packed
 * into a tighter match read as more relevant). Greedy left-to-right
 * matching (the first available occurrence of each query character) can in
 * principle miss a better later alignment for a handful of adversarial
 * inputs, but is deterministic and fast, and matches every case
 * ./fuzzy.test.ts asserts; a full dynamic-programming alignment is more
 * than this palette needs.
 */
export function fuzzyMatch(query: string, target: string): FuzzyMatch | null {
  if (query.length === 0) return { score: 0, indices: [] };
  if (target.length === 0) return null;

  const q = query.toLowerCase();
  const t = target.toLowerCase();
  const indices: number[] = [];
  let score = 0;
  let qi = 0;
  let previousMatchIndex = -1;
  let runLength = 0;

  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] !== q[qi]) continue;

    indices.push(ti);
    score += 10; // base credit for the match itself

    if (previousMatchIndex === ti - 1) {
      runLength += 1;
      score += runLength * 5; // consecutive-run bonus, compounding with run length
    } else {
      runLength = 0;
      if (previousMatchIndex >= 0) {
        // Mild gap penalty: a short, deliberate skip (matching two word
        // initials) should not be punished as hard as a wide scatter.
        score -= Math.min(ti - previousMatchIndex - 1, 4);
      }
    }

    if (isWordBoundary(t, ti)) score += 8;
    if (indices.length === 1) score += Math.max(0, 6 - ti); // earlier-in-string bonus, first match only

    previousMatchIndex = ti;
    qi++;
  }

  if (qi < q.length) return null; // query exhausted target without matching every character

  score += Math.max(0, 20 - t.length); // tight-match bonus: same letters, shorter target

  return { score, indices };
}

export interface HighlightSegment {
  text: string;
  matched: boolean;
}

/**
 * Turns `indices` (as returned by fuzzyMatch, ascending, referring to
 * `text`) into contiguous runs for rendering: ui/src/palette/view.ts wraps
 * each `matched: true` segment in the highlight markup (design.md section
 * 3's "match-character highlighting"). Never reorders or mutates `text`.
 */
export function highlightSegments(text: string, indices: readonly number[]): HighlightSegment[] {
  if (indices.length === 0) return text.length > 0 ? [{ text, matched: false }] : [];

  const matchSet = new Set(indices);
  const segments: HighlightSegment[] = [];
  let i = 0;
  while (i < text.length) {
    const matched = matchSet.has(i);
    let j = i;
    while (j < text.length && matchSet.has(j) === matched) j++;
    segments.push({ text: text.slice(i, j), matched });
    i = j;
  }
  return segments;
}
