// Kill-switch chord (docs/specs/guardian.md: "the panic chord (default
// Ctrl+Alt+Shift+Space, configurable)"). Pure formatting/parsing so the
// Settings screen can show and re-record it under plain `node --test`. The
// real low-level keyboard hook (WH_KEYBOARD_LL on a dedicated thread) is a
// Rust/OS concern in the core process, out of this lane's owned path
// (ui/src only); this only prepares the display string and validates a
// freshly recorded combination before it is saved.

export const DEFAULT_KILL_SWITCH_CHORD = "Ctrl+Alt+Shift+Space";

export interface ChordKeyEvent {
  key: string;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
}

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);
const MODIFIER_PARTS = new Set(["Ctrl", "Alt", "Shift", "Cmd"]);

function mainKeyName(key: string): string {
  if (key === " ") return "Space";
  if (key.length === 1) return key.toUpperCase();
  return key; // already display-ready: "Escape", "F5", "ArrowUp", ...
}

/** A keydown-like event's chord parts in a fixed display order, e.g. ["Ctrl","Alt","Shift","Space"]. */
export function chordPartsFromEvent(event: ChordKeyEvent): string[] {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Cmd");
  if (!MODIFIER_KEYS.has(event.key)) {
    parts.push(mainKeyName(event.key));
  }
  return parts;
}

/** ["Ctrl","Alt","Shift","Space"] -> "Ctrl+Alt+Shift+Space" */
export function formatChord(parts: string[]): string {
  return parts.join("+");
}

/**
 * A recorded chord must carry at least one modifier plus exactly one main
 * key: a bare key (no modifier) would collide with ordinary typing anywhere
 * else in the shell, and the guardian spec's own default is modifier-heavy
 * for the same reason.
 */
export function isUsableChord(parts: string[]): boolean {
  if (parts.length < 2) return false;
  const modifierCount = parts.filter((p) => MODIFIER_PARTS.has(p)).length;
  const mainKeyCount = parts.length - modifierCount;
  return modifierCount >= 1 && mainKeyCount === 1;
}
