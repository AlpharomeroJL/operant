# ADR 0003: Target-app selection for the GUI teach

Status: Accepted (2026-07-15)
Supersedes: none. Amends the frozen IPC surface of ADR 0002 additively.

## Context

When a user types a goal in Operant's command palette, the foreground window is
Operant itself. So "automate the foreground app" is the wrong target. The shipped
build carried this defect twice:

- `readForegroundWindowProcess` in `ui/src/bus/commands.ts` is a hardcoded stub
  returning `"explorer.exe"` (line 94), wired into both the real and mock command
  paths (`ui/src/main.ts:320-321`).
- Because that stub was the source, every GUI teach sent `window_process:
  "explorer.exe"`, so the core faithfully perceived the desktop shell (Program
  Manager) and the compiled precondition gate encoded the desktop, which then failed
  when an app-driven replay ran against the real target.

The recon that grounded this ADR established one important fact: perception is
already bound to `window_process` per snapshot. `ExploreLoop` calls
`self.perceiver.snapshot(&self.window_process)` (explore/mod.rs:164,247,338), and
`UiaPerceiver` resolves the target window fresh each call
(`capture` -> `find_window_by_process`, perception-uia/src/uia/mod.rs:112). So the
core does NOT need a perception change; the defect is purely that the UI supplied the
wrong `window_process`.

## Decision

Explicit pick, with a smart default. The palette teach flow gains a target-selection
step so the teach binds to the app the user means, not to Operant.

1. New core command `list_windows` (additive; see contract below) returns the open
   top-level windows the core can perceive, in z-order (topmost first), excluding
   Operant's own window. It reuses the existing enumerator and accessibility gate in
   `crates/perception-uia/src/uia/window.rs` (`enum_proc`, `process_image_name`,
   `window_title`, `deny_if_inaccessible`), so a window the core cannot perceive is
   never offered.
2. The palette teach flow presents those windows (title plus process) plus a "the app
   I switch to next" option (switch-to-next capture), pre-selected to the first
   non-Operant window (the last-active app, since z-order top is the last foreground).
   The chosen window's process becomes `start_explore`'s existing `window_process`
   argument. The `readForegroundWindowProcess` stub is removed as the real source; it
   remains only as the Demo/off-Tauri default.
3. No core perception change is required: perception, the compiled precondition gate,
   and replay already bind to `window_process`, so sending the chosen process is
   sufficient. The perceived window's title is captured into the snapshot and baked
   into the compiled gate, so the workflow encodes the specific window's identity.

## Reserved: `title_pattern`

`start_explore` gains an OPTIONAL `title_pattern` argument, reserved for disambiguating
two windows of the same process (for example two Notepad windows). It is documented
and accepted but not yet consulted by perception, which resolves by process and picks
the topmost match. When a real same-process multi-window case needs it, perception is
extended to prefer the title-matching window (a `find_window_by_process_and_title`
refinement); until then the field is honestly reserved, never faked. The truth gate
and the A1 bar use one window per process, so process-targeting is exact for them.

## Alternatives considered

- Switch-to-next only: after submit, capture the next foreground window. Lower
  friction but racy (a double alt-tab picks the wrong app) and it hides the target
  from the user at commit time. Kept as one option inside the picker, not the whole
  mechanism.
- Last-active non-Operant only: bind to the most recent non-Operant foreground window
  with no confirmation. Zero friction but surprising and unauditable. Kept as the
  picker's pre-selected default, always confirmable.

## Contract change (additive; amends ADR 0002's frozen surface)

Per `contracts/ipc.md` section 9 rule 2, new commands and new optional args are
additive and do not bump `pv`. Recorded here per project convention because the doc is
marked frozen.

- New command `list_windows`: `{} -> { windows: [ { process, title, id } ] }`. `id` is
  the opaque HWND as a hex string, for display and future use. Z-ordered, Operant
  excluded. In a build without `real-uia` it returns an empty list (no real windows to
  offer), and the UI falls back to the switch-to-next option.
- `start_explore` args gain optional `title_pattern` (string). Reserved as above.

## Consequences

- The bar: from the app, teaching a Notepad task with Notepad chosen while Operant has
  focus produces a compiled workflow whose precondition binds Notepad, and app-driven
  replay passes its gate against Notepad.
- No `pv` bump, no fixture break: `window_process` semantics are unchanged; only a new
  command and an optional arg are added.
