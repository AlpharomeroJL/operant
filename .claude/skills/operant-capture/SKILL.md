---
name: operant-capture
description: Produce README screenshots and GIFs from the E2E harness.
---
Drive the built app via e2e/ (Playwright for Tauri and the fixture web app, OS
screenshot fallback for native windows). Record video, convert with ffmpeg two-pass
palettegen: max width 800, under 8 MB each, 12 fps. Required assets:
00-onboarding.gif (wizard: pick local model, progress bar, done), 01-palette.gif
(hotkey, plain goal typed, agent starts), 02-explore.gif (run viewer stepping, model
indicator ON), 03-steps.png (compiled workflow as numbered plain-English steps, Advanced
toggle visible but closed), 04-replay.gif (same task instant, model indicator OFF: the
money shot), 05-gate.png (safety halt on a payment dialog, human-language message),
06-drift.gif (button moved, "Update the workflow?" prompt, approve, green rerun),
07-bench.png (BENCHMARKS.md table), 08-gallery.png (template gallery with plain-language
grants), 09-tray.png, 10-undo.gif (run finishes, "Undo last run", files restored with
narration), 11-timesaved.png (tray showing "Operant saved you 3.2 hours this week"),
12-killswitch.gif (mid-run panic hotkey, everything freezes, tray red). Save to assets/
with alt text. Impossible in CI: placeholder plus TODO in LAUNCH.md, never a broken link.
