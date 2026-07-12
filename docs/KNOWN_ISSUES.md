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
  the app's own backend is wired in. The check, download, and
  signature verification are tested end to end against a local fixture update
  server, including a tampered manifest that is correctly rejected. What is
  not yet proven: a real update against this project's actual release server,
  and a real install swapping files on a live desktop, both of which need a
  published release to test against. To update in the meantime, downloading
  the latest installer from the releases page still works.

- **Reinstalling shows one Windows permission prompt.** Reinstalling over an
  existing copy triggers a single Windows permission (UAC) prompt that a person
  has to click. Uninstalling does not have this prompt.

- **The uninstaller's "remove saved data" prompt now targets the correct
  folders, pending an end-to-end check.** The prompt that offers to also remove
  your saved workflows and recordings now clears the real per-user data
  directories (`%APPDATA%\dev.operant.shell` and `%LOCALAPPDATA%\dev.operant.shell`);
  an earlier build checked a folder Operant does not use, so it could leave that
  data in place. Your data is never removed unexpectedly. The corrected path is in
  the installer script but not yet confirmed by a full install-and-uninstall run,
  which is part of release smoke-testing.

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
