# Changelog

All notable changes to Operant are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Operant aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

A visual redesign is in progress: a calmer, instrument-inspired look, a new home
dashboard, a redesigned run viewer (the flight recorder), a dedicated undo
screen, and a working auto-updater. See `docs/specs/design.md`.

### Fixed

- Replaying a workflow against a live window whose layout has moved now finds
  each element again by its identity at run time, instead of clicking the
  spot it was taught. Re-finding stays on your machine with no model calls.
- A correction made in the middle of a run now folds into the saved workflow the
  same way a correction recorded ahead of time does.

## [1.0.0] - 2026-07-11

The first public release. Operant is a free, open source desktop agent for
Windows: show it a task once, by demonstration or by voice, and it saves what it
learned as a workflow you can run again with one click or on a schedule.

### Added

- Teach by demonstration or by voice, then save the result as a named workflow.
- Run a saved workflow with one click, from the command palette, or on a
  schedule.
- Replay runs entirely on your own machine. After the first time it is taught, a
  workflow replays from a file with no model calls and no network calls. Both
  are checked automatically.
- A one-key kill switch that stops everything in under 100 milliseconds, below
  the planner so no decision can delay it.
- Undo for runs: write actions record how to reverse themselves before they run,
  so a run can be undone. Anything without a safe reverse, like a sent email, is
  labeled before you run it.
- Choose how it thinks: download a free local model with a progress bar, sign in
  with a ChatGPT or Claude account you already have, or paste an API key. A demo
  mode lets you watch it work first.
- Privacy by default: everything is local-first. The optional watch-and-suggest
  feature is off until you turn it on. A plain-English audit log records what ran.
- A template gallery and a cookbook of example workflows.
- Plain-English explanations of any workflow, a first-run tour, and a Spanish
  locale.
- A Windows installer with a verified auto-updater signature, a software bill of
  materials, and a published benchmark report.

### Known issues

See [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md) for the honest edges in this
release, including the Windows SmartScreen "unknown publisher" warning and the
current replay caveats.

### Notes

Operant v1.0.0 was built with heavy AI assistance as an open, documented
project. See the repository history and the release notes for how it was made.
