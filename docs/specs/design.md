# Operant design system

Status: Binding. This is the single visual and interaction reference for the
Operant desktop app and the docs site. Every color and size in the app derives
from `ui/src/theme/tokens.ts`; a lint forbids raw hex anywhere else. Every screen
below references tokens only. When code and this document disagree, this document
wins until it is amended.

## 1. Direction

Operant is a flight recorder for your computer: it watches once, remembers
exactly, and replays perfectly. The design is instrument calm: quiet, precise,
trustworthy, with one warm signal color that means "recording or active." It is
native to Windows 11 (Mica material, 8px corners, Segoe-adjacent metrics) while
carrying its own identity.

It must not read as:
- a generic AI-tool site (cream background with a terracotta accent),
- a hacker terminal (near-black with acid green),
- an Electron web page inside a frame.

Density is calibrated to Raycast and Things 3: generous line height, tight
information hierarchy, nothing decorative.

## 2. Tokens

Single source: `ui/src/theme/tokens.ts`. Every color and size in the app derives
from these. A lint forbids raw hex outside the tokens file.

### Surfaces

Dark theme (the default):
- bg0 `#101114` (window on Mica)
- bg1 `#17181C` (cards)
- bg2 `#1E2026` (raised)
- hairline `#2A2D34`

Light theme:
- bg0 `#F7F7F5`
- bg1 `#FFFFFF`
- bg2 `#F1F1EE`
- hairline `#E3E3DE`

### Ink

- primary `#ECEDEF` (dark) / `#1B1C1E` (light)
- secondary `#9DA2AB` (dark) / `#5C5F66` (light)
- disabled: 45 percent of secondary

### Signal

The identity color, used only for recording, active state, and the primary call
to action. It is a light on an instrument, never a large fill.
- amber `#E8A13C`
- hover `#F2B355`
- on-signal ink `#1A1204`

### Semantic

- success `#3FB27F`
- danger `#E5484D` (reserved: kill switch, destructive actions)
- info `#5B8DEF`

Replay state uses ink, not color. Replay is the calm default; explore is the
amber exception. This is the visual thesis: determinism looks quiet.

Status-dot fill only, light theme (H2, a11y-and-contrast): amber `#C17A17`,
success `#379B6F`, info `#5387EE`. They are the same hue and saturation as the
values above, darkened until a small dot clears WCAG 1.4.11's 3:1 against bg1/bg2;
every other use of amber/success/info, and both dark-theme values, is unchanged.

### Type

- UI and display face: Instrument Sans (bundled, variable).
- Numeric and step data: IBM Plex Mono, tabular figures for timers, counts, and
  hashes.
- Scale: 12 / 13 / 15 body, 17 section, 22 title, 28 dashboard hero.
- No font ships that is not bundled.

### Space, radius, shadow, motion

- Spacing on a 4px grid.
- Radius: 8 (cards), 6 (controls), full (pills).
- Shadows minimal, one level; the dark theme uses hairlines instead of shadows.
- Motion: 160ms cubic-bezier(0.2, 0, 0, 1) standard.
- One orchestrated signature moment only: when a replay run completes, the
  timeline strip compresses into the workflow card with a brief amber tick.
- `prefers-reduced-motion` disables all nonessential motion. Nothing animates on
  load.

## 3. Screens

Each screen references tokens only.

### Palette

A Raycast-grade centered top-third floating panel on Mica. Single input, fuzzy
match over workflows, quick actions, and settings with match-character
highlighting. Results grouped (Workflows, Actions, Recent). Footer hints show
Enter to run, Ctrl+Enter to dry run, Tab for details. Arrow-key first, zero
mouse required. Recents ranked by frecency. Typing a sentence that matches
nothing offers "Teach this" as the amber primary row.

### Home dashboard

The new default window view. A hero line in plain language, "Operant saved you
3.2 hours this week," with a small sparkline of the last 8 weeks. Below: Up next
(scheduled runs with humane times, "tomorrow at 9"), Recent runs (compact rows:
status dot, name, one-line outcome, relative time), and a quiet empty state that
invites teaching the first workflow.

### Flight recorder

The run viewer, and the signature screen. A horizontal filmstrip of step
thumbnails (redacted screenshots) above a vertical step list. Each step row is
the plain-English sentence, duration in mono, status dot. Live runs append in
real time with the strip auto-following. Scrubbing the strip highlights the row
and vice versa. Explore runs show the amber REC chip; replays show a quiet gray
chip reading "no AI, exact replay." Stop is always visible; Pause reveals the
intervene field. Failed gates render as an inline card in the list, not a modal.

### Library

A card grid. Each workflow gets an auto-assigned duotone glyph and hue (hash of
the name into a fixed 12-hue ramp, overridable), a name, a plain one-liner, a
minutes-saved badge, and a last-run dot. Hover reveals Run, Schedule, Explain.
Drag to reorder. Search filters live.

### Undo screen

New; it closes the placeholder. From any completed run, "Undo this run" opens a
preview list of restorations in plain English with per-item checkmarks.
Irreversible items are grayed with "cannot be undone." Confirm executes with the
same filmstrip treatment in reverse.

### Wizard

The same four engine paths, restyled: a full-window calm layout, one decision
per screen, progress as three quiet dots. The local-model download shows size, a
works-on-this-PC check, and a pause/resume bar. The finish screen is the first
dashboard with a single amber "Teach your first workflow" button.

### Tray

Glyph states: idle outline, amber pulse recording, gray play replaying, red
kill. Menu: the top three frecent workflows as one-click Quick Runs, then Open,
Pause all, and a panic row.

### Settings

Sidebar sections: General, Thinking engines (with probe badges), Voice, Privacy
(the watch-and-suggest toggle and purge), Appearance (dark/light/system and an
accent sync toggle), Advanced. Advanced is where the DSL editor, audit browser,
and MCP config continue to live.

### Toasts

Bottom-right, one line, verb-first ("Saved as workflow", "Run complete, 14
steps"). Amber only when an action is invited.

## 4. Copy rules

These extend the microcopy glossary. Commit new pairs as they are introduced.

- Sentence case everywhere.
- Verbs on buttons say what happens: "Save as workflow", "Run now", "Undo this
  run".
- The word "AI" appears in exactly two places in-app: the wizard engine step and
  the REC chip tooltip ("Operant is using your AI engine to learn this").
- Replay never mentions AI except to say it is not using one.
- Errors: what happened, why, one action, in three short sentences. No apology,
  no exclamation points.
- Empty states invite one specific action.
