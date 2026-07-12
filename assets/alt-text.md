# Asset alt text

Alt text for the thirteen README/LAUNCH.md capture assets in this folder.
Use these verbatim as the `alt` attribute (HTML) or the `[alt text]` part of
a markdown image link. See `.claude/skills/operant-capture/SKILL.md` and
`LAUNCH.md`'s asset ledger (section 6) for what each asset is for.

| File | Real or placeholder | Alt text |
|---|---|---|
| `00-onboarding.gif` | Real | Animated walkthrough of the Operant setup wizard: the welcome screen, choosing to download a free local model with a live progress bar from 0 to done, a guided first task filling out a sample invoice step by step, and picking a schedule, ending with the wizard closed and the main Run screen visible. |
| `01-palette.gif` | Real | Animated demo of the Operant command palette: the goal text field is focused, the text "Copy the invoice total into the spreadsheet" is typed in, and pressing Enter starts a new run. |
| `02-explore.gif` | Real | Animated run viewer stepping through a live teaching run with the model indicator reading "Thinking live": steps to click Downloads, click Invoice.pdf, copy, and paste appear one at a time and turn green. |
| `03-steps.png` | Real | Screenshot of the Explain panel for the "Copy the invoice total into the spreadsheet" workflow, showing what the workflow can do and its four steps numbered in plain English, with the Advanced toggle closed. |
| `04-replay.gif` | Real | Animated run viewer replaying the same copy-invoice-total task with the model indicator reading "Running from memory, no thinking needed": the same four steps as 02-explore.gif complete almost instantly, ending on Done. |
| `05-gate.png` | Real | Screenshot of a safety halt: the run viewer reads "Stopped, needs you" with a blocked step "Click 'Confirm payment'", and the tray icon has turned red with a notification that Operant stopped and needs the user before it can continue. |
| `06-drift.gif` | Real | Animated drift repair loop: a run halts on a step reading "Click 'Save invoice'" because the button was renamed, a card asks "Update the workflow?" showing "Save invoice" to "Store invoice", and after clicking "Update the workflow" the run repeats with every step green, now reading "Click 'Store invoice'". |
| `07-bench.png` | Real | Table image of the Operant benchmark headline from BENCHMARKS.md, comparing compiled replay (near-zero latency, zero model calls) against re-inferring every step (higher latency, dozens of model calls and tokens) across three tasks. |
| `08-gallery.png` | Real | Screenshot of the template gallery showing the notepad-invoice-note workflow card and its install preview: six plain-English steps, a trust note that it is the first workflow from this publisher, and a permission prompt reading "This workflow can control Notepad" with Allow and Deny buttons. |
| `09-tray.png` | Real | Screenshot of the Operant header at rest, showing the app name and an idle gray tray status dot with no active notifications. |
| `10-undo.gif` | Placeholder | Placeholder graphic labeled "Placeholder, not a real capture" stating that this asset will show a finished run, an "Undo last run" action, and the restored files narrated in plain English, and noting this screen does not exist in ui/src yet (see LAUNCH.md's Capture TODOs). |
| `11-timesaved.png` | Real | Screenshot of a tray notification reading "Your weekly time saved: Saved about 192 minutes this week" with a Dismiss button. |
| `12-killswitch.gif` | Real | Animated kill switch demo: a run is mid-step (Click Downloads, Click Invoice.pdf) with the model indicator on, then the panic hotkey fires and the run freezes, the run viewer reads "Stopped, needs you", and the tray icon turns red with an "Operant stopped" notification. |
