# P0b: live-engine proof (real Windows desktop)

Date: 2026-07-12. Machine: the author's Windows 11 desktop. Binary:
`operant.exe` built from branch `redesign` with
`--features real-uia,real-input,dev-agent-bridge` (the real UIA perceiver, real
Win32 `SendInput` synthesizer, and the dev-only agent-bridge planner). The
release build never enables `dev-agent-bridge`.

## What was proven

The deterministic, model-free replay engine drives a real Windows desktop, and
a real intelligence (Opus 4.8, via the agent-bridge planner) can teach a task
through the live engine that then replays with no model and no network.

1. **Real UIA perception.** `operant explore` captured the full live element
   tree of Notepad each round (window, tabs, menu bar, editor, status, ~2.5 KB
   digests). Not a fixture.
2. **Model-driven teach, human-as-brain.** Opus 4.8 answered each planner turn
   over the agent-bridge file rendezvous, reading the live digest and proposing
   Action IR. It taught: `ctrl+a` (select existing), `type "OPERANT LIVE PROOF
   7A3F"` (replace), `ctrl+a`, `ctrl+c`. Run outcome Ok, 4 steps recorded. The
   typed text was verified exactly via the clipboard: `OPERANT LIVE PROOF 7A3F`.
3. **Compile.** `operant compile` turned the recorded trajectory into a
   compiled workflow (`notepad-clear-editor-...`, 7 steps incl. inserted waits),
   manifest declaring `capabilities.network = false`.
4. **Model-free replay, 5/5.** `operant run <compiled.json>` replayed on the
   live Notepad five consecutive times; every run reproduced the exact marker
   on the clipboard (`PASS=5/5`). Replay uses `operant-replay`, which links no
   model/orchestrator/network crate by construction (enforced by the test
   `replay_crate_is_backend_free`); the run needs no planner, no bridge, no
   network.
5. **Window-move re-resolution (window level).** With the Notepad window moved
   to (320,260), the same compiled workflow replayed clean, so the engine
   re-finds and drives the window by identity (process + regex title), not a
   stale position.

## Engine fixes this proof required (the engine was a prototype)

Driving a live desktop surfaced real defects, each fixed and committed:

- **Focus by regex title** (P0a): `focus_window` matched `title_pattern` as a
  regex over enumerated windows instead of handing the regex to `FindWindowW`.
- **Foreground lock** (`focus_with_attach_workaround`): a background process
  could not `SetForegroundWindow`. Fixed by dropping the lock timeout, injecting
  a benign input event to earn foreground rights, restore-if-minimized, retry,
  and treating the bool as best-effort with `verify_focus_landed` as the gate.
- **Menu-mode key loss**: the foreground input injection used an Alt tap, which
  opened the target's menu bar and swallowed the next key combo. Switched to a
  Shift tap.
- **Typing fidelity**: a single Unicode burst was dropped/repeated/transposed by
  the modern rich-edit control. `type_text` now settles after focus and paces
  per code unit. 5/5 clean after this.

## Remaining P0b item (deferred, with rationale)

- **Explorer element-click re-resolution (KI-1 element identity).** The Notepad
  proof exercises keys/typing and window-level re-find, not element-coordinate
  re-resolution (keys go to the focused window). The replayer's live element
  re-resolution is covered headlessly by `crates/replay/tests/live_reresolve.rs`.
  A live element-click proof (click a file in Explorer by identity after moving
  the window) is the honest remaining item; it will be exercised live when real
  clicks flow through the app in Phase 2 (B4/B16) and the T2 truth-gate. Live
  element clicking also needs a selector-format pass (the resolver matches a
  full name-role path or a unique automation id; a single-segment path does not
  resolve).

## Verdict

The hard-stop question for P0b was whether the engine can automate a live
desktop at all. It can, reliably and model-free (5/5). The engine required real
hardening to get there, which means "wire the engine for real" includes
engine-reliability work, not only wiring. Proceeding to P1 (freeze the bridge
contract) on that basis, with the Explorer element-click proof carried into
Phase 2 / T2.
