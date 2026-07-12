// The command palette (C13, FR-O1): a single always-reachable text field that
// starts a run against the bus (contracts/bus_events.md). docs/specs/ui.md:
// "command palette (global hotkey, single text field, submits to explore
// with the current foreground window as context)". Only the logic lives
// here: matching the hotkey, cleaning up the typed text, and starting the
// run. Focusing the input and reading its live value are DOM glue left to
// main.ts, the same split used everywhere else in ui/src (see
// ui/src/bus/mockClient.ts, ui/src/state/mode.ts).

import type { BusClient } from "../bus/mockClient.ts";
import { createMockCoreCommands } from "../bus/commands.ts";

/**
 * True when a keydown event matches the palette's global-hotkey-style
 * shortcut: Ctrl+K on Windows/Linux, Cmd+K on macOS. A real OS-wide hotkey
 * (reachable outside the window entirely) is Tauri/src-tauri territory,
 * outside this lane's owned paths; this is the in-window equivalent that
 * jumps focus to the palette from anywhere in the shell.
 */
export function isGlobalPaletteHotkey(event: { key: string; ctrlKey?: boolean; metaKey?: boolean }): boolean {
  return (event.ctrlKey === true || event.metaKey === true) && event.key.toLowerCase() === "k";
}

/** Trim palette input. Returns null for a blank goal: nothing to submit. */
export function normalizeGoal(raw: string): string | null {
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export interface SubmitGoalOptions {
  stepDelayMs?: number;
}

/**
 * Submit a typed goal from the palette. Issues the start_explore command
 * (contracts/ipc.md section 5b) with the goal and the foreground window as
 * context, the model-driven teach path the palette always starts (never a
 * saved-workflow run, per docs/specs/ui.md). In dev/Demo the mock CoreCommands
 * drives the canned bus stream so the flight recorder still fills; the real
 * shell swaps in a Tauri-backed CoreCommands (ui/src/bus/commands.ts). Returns
 * a stop function the caller should use to cancel a previously started run
 * before starting this one, or null when the goal was blank and nothing was
 * started.
 *
 * ui/src/main.ts drives the palette's teach commit through its own injected
 * CoreCommands directly (so a real foreground window is carried in the real
 * shell); this convenience wrapper keeps the (bus, goal) call shape the run
 * viewer's own scripted tests use.
 */
export function submitGoal(bus: BusClient, rawGoal: string, opts: SubmitGoalOptions = {}): (() => void) | null {
  return createMockCoreCommands(bus, opts).startExplore(rawGoal);
}
