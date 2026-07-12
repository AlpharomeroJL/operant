# Operant launch demo: narration script

Spoken narration for `launch-demo.mp4`, timed to the thirteen shots in
LAUNCH.md section 4 (the demo shot list). Per FR-D6, this script is written
to be rendered by Operant's own local TTS voice, the Kokoro provider seam in
`sidecars/voice/src/providers/kokoroProvider.js`.

It has not been synthesized to audio. See `README.md` in this folder for
why: the voice sidecar currently ships a mock TTS provider that returns a
synthetic wav, not real speech, so `launch-demo.mp4` ships silent with these
same lines burned in as on-screen captions instead of a fake voice track.

Timestamps are each shot's start time in the assembled video (mm:ss.t). The
line for each shot is the exact text burned into that shot's caption.

| # | Shot | Start | Asset | Narration line |
|---|------|-------|-------|-----------------|
| 00 | Onboarding | 00:00.0 | `00-onboarding.gif` | Setup is a wizard. Pick a free local model, watch it download, and land on your first task. |
| 01 | Command palette | 00:08.3 | `01-palette.gif` | Hit the hotkey, type your goal in plain English, and Operant gets to work. |
| 02 | Explore | 00:13.7 | `02-explore.gif` | The first time, it explores. Watch it think through the task live, step by step. |
| 03 | Compiled steps | 00:18.1 | `03-steps.png` | Every run compiles into plain-English steps. What you read is what it runs. |
| 04 | Replay | 00:22.1 | `04-replay.gif` | Run it again and it is instant. Same result, zero model calls this time. |
| 05 | Safety gate | 00:26.4 | `05-gate.png` | Before anything risky, Operant stops and asks. Payments and deletes always wait for you. |
| 06 | Drift repair | 00:30.4 | `06-drift.gif` | When the app changes, Operant does not fail silently. It finds the broken step, proposes a fix, and waits for your approval. |
| 07 | Benchmark | 00:34.7 | `07-bench.png` | The proof lives in BENCHMARKS.md, regenerated every release: compiled replay measured against re-inferring every step. |
| 08 | Gallery | 00:39.2 | `08-gallery.png` | Or skip teaching entirely. Install a signed workflow from the gallery, permissions written as plain sentences. |
| 09 | Tray | 00:43.2 | `09-tray.png` | Operant lives quietly in your tray until you need it. |
| 10 | Undo (placeholder) | 00:46.2 | `10-undo.gif` | Placeholder, not a real capture. Undo is designed but this screen is not built yet. |
| 11 | Time saved | 00:49.2 | `11-timesaved.png` | Operant keeps a running count of the time it hands back to you every week. |
| 12 | Kill switch | 00:52.7 | `12-killswitch.gif` | And if anything ever looks wrong, one key stops it instantly, no matter what it is doing. |

Total runtime: 57.4 seconds.

## Notes

- Shot 10 is written differently on purpose. LAUNCH.md's Capture TODOs and
  `assets/alt-text.md` both mark `10-undo.gif` as an honest placeholder, not
  a real UI capture, because no undo screen exists in `ui/src` yet. The line
  above says so plainly instead of narrating it as a working feature. Once a
  real undo screen ships and the asset is recaptured, rewrite this line to
  match the other twelve.
- No specific benchmark numbers are spoken (no `{BENCH_REPLAY_P50_MS}` and
  so on). Those placeholders are filled by a later packet (L9B, per
  LAUNCH.md section 6), and this script should not state numbers it cannot
  back up yet.
- To produce real narration once Kokoro is wired in: feed each row's line to
  the voice sidecar's `tts()` call in shot order, concatenate the resulting
  wav segments, and mux the result onto `launch-demo.mp4`, or re-cut the
  video against the real speech timing. Spoken pacing will not line up
  exactly with the silent on-screen beats used here, since those were timed
  for reading, not for a synthesized voice.
