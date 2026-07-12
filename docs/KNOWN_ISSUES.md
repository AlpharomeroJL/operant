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
  installs it. This defaults to on, and air-gapped or offline runs never make
  the check at all. A Settings > Updates toggle exists in the app's UI, but it
  is not yet connected to this: today it only changes what the Settings
  screen itself remembers, the same gap every other Settings toggle in this
  build has until the app's own backend is wired in. The check, download, and
  signature verification are tested end to end against a local fixture update
  server, including a tampered manifest that is correctly rejected. What is
  not yet proven: a real update against this project's actual release server,
  and a real install swapping files on a live desktop, both of which need a
  published release to test against. To update in the meantime, downloading
  the latest installer from the releases page still works.

- **Reinstalling shows one Windows permission prompt.** Reinstalling over an
  existing copy triggers a single Windows permission (UAC) prompt that a person
  has to click. Uninstalling does not have this prompt.

- **The uninstaller's "remove saved data" prompt points at the wrong folder.**
  When you uninstall, the prompt that offers to also remove your saved workflows
  and recordings currently checks a folder Operant does not use, so it may leave
  that data in place. Your data is never removed unexpectedly; the prompt just
  may not do what it offers. The folder path is being corrected.

## Replay

- **Live re-find is implemented but not yet wired into the installed app.**
  Replay can now re-find an element by its identity at run time when a window has
  moved, with no model calls, and this is covered by tests. Connecting it to the
  installed app's run path and verifying it live on a real desktop (moving a
  window between teaching and replay) is the remaining step; until then the
  installed app replays each step at the location it was taught.

## Undo

- **There is no dedicated undo screen yet.** Undo works: every write action
  records how to reverse itself before it runs, and anything without a safe
  reverse is labeled before you run it. But there is not yet a dedicated screen
  to preview and confirm undoing a whole run; it is driven from the run view. A
  dedicated screen is being added.
