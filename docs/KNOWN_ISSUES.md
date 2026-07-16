# Known issues

An honest list of the rough edges in the current release, in plain language.
Each item says what you would notice, why it happens, and what is being done.
As items are fixed they are struck from this list and noted in the changelog.

## Installation and updates

- **Windows shows an "unknown publisher" warning the first time you run the
  installer.** Operant's installer is not yet signed with a Windows code-signing
  certificate, so SmartScreen flags it. To continue, click "More info," then
  "Run anyway." Only the auto-updater's signature is verified today. The exact
  steps and screenshots are in the install docs, and obtaining a code-signing
  certificate is planned; see docs/signing.md.

- **Automatic updates are wired in, but not yet proven against a live
  release.** Operant now checks for updates on start and every 24 hours,
  downloads a staged copy, and verifies its Ed25519 signature against the
  embedded key (release/KEYS.md) before trusting it; once verified, a
  notification asks you to restart, and restarting (or quitting normally)
  installs it. This defaults to on. Setting the environment variable
  OPERANT_AIRGAPPED stops it from ever attempting a check at all; there is no
  separate offline detection beyond that, so a normal run that happens to be
  offline still attempts the check and simply gets a failed connection,
  handled the same as any other network hiccup, not a special offline state.
  A Settings > Updates toggle exists in the app's UI, but it is not yet
  connected to this: today it only changes what the Settings screen itself
  remembers, the same gap every other Settings toggle in this build has until
  the app's own backend is wired in. The check, download, and Ed25519
  signature verification are tested end to end against a local fixture update
  server: a correctly-signed manifest is accepted, and three separate refusals
  are proven against the real signing path, a manifest signed with the wrong
  key (checked against the exact pubkey shipped in tauri.conf.json), a manifest
  whose signature field was tampered with, and an artifact whose bytes were
  swapped after signing. What is not yet proven: a real update against this
  project's actual release server, and a real install swapping files on a live
  desktop and relaunching on the new version, both of which need a published
  release to test against. To update in the meantime, downloading the latest
  installer from the releases page still works.

- **Reinstalling shows one Windows permission prompt.** Reinstalling over an
  existing copy triggers a single Windows permission (UAC) prompt that a person
  has to click. Uninstalling does not have this prompt.

- ~~**The uninstaller's "remove saved data" prompt deleted a folder Operant does not use, so it could leave your data behind.**~~
  **Fixed in code; pending the end-to-end install-and-uninstall smoke.** The
  prompt that offers to also remove your saved workflows and recordings now
  clears exactly the two real per-user data directories the app writes to
  (`%APPDATA%\dev.operant.shell` and `%LOCALAPPDATA%\dev.operant.shell`); an
  earlier build cleared a nonexistent `Operant` folder instead. The hook is
  hardened so a missing path variable can never widen the delete: it removes
  only those two identifier-scoped folders, and only when you accept the prompt.
  Declining keeps them, and No is the default, so your data is never removed
  unexpectedly. What remains is the routine confirmation on a real installer,
  part of release smoke-testing; the exact steps are in
  `release/nsis/VERIFY-UNINSTALL.md`.

## Teaching

- **You teach by describing a task, not by Operant recording you do it.** The
  shipped way to teach is to type a goal in plain language, pick which open app it
  should run in, and let a model work the task out on your desktop while you watch
  each step land. When it works, you save it. A recorder that captures you
  performing a task by hand ("show it by doing it once") is planned but not built:
  it is a labeled roadmap item, and nothing in this release records a hand-performed
  demonstration. Until it ships, every teach affordance is the describe-it path.

- **Voice can talk and listen, but a fully voice-taught workflow is not proven
  yet.** Local speech in and out works and is tested, so Operant can read steps
  aloud and take spoken input. Teaching a complete workflow start to finish using
  only your voice is not separately tested (it needs a real microphone and
  speakers), so treat voice as an input and output channel, not a proven hands-free
  teaching mode.

## Replay

- **Live re-find is wired into the run path; the live-desktop confirmation is
  the remaining step.** Replay re-finds an element by its identity at run time
  when a window has moved, with no model calls, and `operant run` now
  constructs its replayer with a live Perceiver so a real run re-resolves each
  click against the live desktop instead of replaying the location it was
  taught. This real path is on only in a real build (the `real-uia` and
  `real-input` features); the default, deterministic build still replays from
  the taught coordinate, so the golden path stays model-free and reproducible.
  Both the re-resolution itself and the wired run-path construction are covered
  by headless tests. What remains is the live confirmation on a real desktop
  (moving a window between teaching and replay): the Notepad plus Explorer 5/5
  desktop smoke is performed separately, by the orchestrator in the main
  session, and is not claimed here.

## Undo

- **There is no dedicated undo screen yet.** Undo works: every write action
  records how to reverse itself before it runs, and anything without a safe
  reverse is labeled before you run it. But there is not yet a dedicated screen
  to preview and confirm undoing a whole run; it is driven from the run view. A
  dedicated screen is being added.

## Scheduling

- **You can rerun a saved task with one click, but you cannot yet start a
  schedule from the app.** Running a saved task again on demand is fully wired.
  The scheduler itself (cron times, file-watch triggers, unattended replay-only
  runs) is built and tested in the engine, but the app's own trigger commands
  currently answer "not implemented," so there is no button in the app that
  creates a schedule end to end. Setting a task to run on its own is on the
  roadmap; until then, treat scheduling as engine-ready but not app-wired.

## Registry and model setup

- **Installing a shared workflow reads from a local copy of the index, not over
  the network yet.** `operant install <name>` verifies a workflow's Ed25519
  signature against its publisher key and refuses tampered or wrongly-signed
  workflows, and that whole path is tested. What it does not do yet is fetch the
  index over the network: today it reads from a local checkout of the registry
  (a clone of the operant-registry repository). Pulling the index over the wire
  is not wired up yet.

- **The setup wizard's "download a local model" step is a stand-in, not a real
  download yet.** The wizard can walk you through picking a local model, but the
  download it shows is a simulated placeholder rather than a real fetch of model
  weights. Configuring a backend you already have (a local runner, an API key, or
  signing in with a subscription) is real and works; the in-wizard model
  download is not a real download yet.
