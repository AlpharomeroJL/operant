# Operant v1.1.0

Teach your computer a task once, in plain language, and pick which open app it
should run in. A model works the task out live on your screen while you watch,
and Operant freezes that successful run into a workflow it repeats on its own,
with no model and nothing sent anywhere.

Windows. Free. Open source (Apache 2.0). Built by one person with heavy use of AI
coding tools.

## What is new since v1.0.0

- **Pick the app you are teaching.** The command palette now asks which open
  window the task targets, so a teach binds to the app you mean instead of
  whatever happens to be in front. The compiled workflow, and every replay,
  binds to that app.
- **Bring your own backend, for real.** The shipped core drives exploration with
  a real model backend you configure (local runners like Ollama, or an API
  provider). There is no mock planner in the shipped execution path; the app's
  capability handshake reports itself real, and refuses to show real-work UI on a
  build that could only mock.
- **The flight recorder streams live.** Each step of a teach now appears in the
  run viewer as it happens, not just after the run is recorded.
- **A material that means something.** Four surfaces (the palette, the run
  viewer, the kill-switch overlay, and the drift panel) use a glass material that
  reads the run's state: warm and alive while a model is thinking, still and
  sharp on a model-free replay. Everything else stays solid and calm. It falls
  back to solid surfaces under reduced-transparency, and the distinction still
  reads.
- **An honest instrument readout.** The run viewer shows the model-call count
  from a real measured counter: nonzero while exploring, a structural zero on
  replay. It is not a hardcoded number.
- **Reliability.** Focus is handed to the target window reliably from the
  background core, and the dev planner bridge no longer writes to the IPC
  channel.

## What holds, every run

- Replay makes zero model calls and zero network calls, asserted in CI, not just
  promised. A model call on replay is a compile-time impossibility, not a setting.
- One key stops everything under a tenth of a second, below the planner, with a
  full-screen overlay that is pre-built so revealing it never waits on anything.
- Every write action records an inverse before it runs, so a run can be undone to
  a byte-identical prior state. Anything without a safe inverse is labeled
  irreversible before you run it.
- Signed workflows install and tampered ones are refused; signed updates verify
  and a wrong key is refused.

## Known issues

- Starting a schedule from the app is not wired yet. The scheduling engine (cron,
  file-watch, unattended replay) is built and tested, but the app answers
  not-implemented for now. Running a saved task again with one click is wired.
- Local speech in and out ships as an input/output channel; a full hands-free
  voice-taught workflow is not separately proven.
- Installing a workflow reads a local registry checkout; over-the-wire fetch from
  the registry repo is not wired yet.
- The installer is not code-signed, so Windows SmartScreen will warn about an
  unknown publisher on first run. It is safe to install; the proof is in the
  test suite, not the signature.

## Install

Download the installer below and run it. Then pick a model engine and teach your
first task. Building from source instead: see CONTRIBUTING.md.
