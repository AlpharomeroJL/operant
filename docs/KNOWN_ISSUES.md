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
  certificate is planned.

- **Automatic updates are not active yet.** The updater is configured but not
  wired into this build, so it does not check for or install updates on its own.
  To update, download the latest installer from the releases page. Wiring the
  updater is in progress.

- **Reinstalling shows one Windows permission prompt.** Reinstalling over an
  existing copy triggers a single Windows permission (UAC) prompt that a person
  has to click. Uninstalling does not have this prompt.

- **The uninstaller's "remove saved data" prompt points at the wrong folder.**
  When you uninstall, the prompt that offers to also remove your saved workflows
  and recordings currently checks a folder Operant does not use, so it may leave
  that data in place. Your data is never removed unexpectedly; the prompt just
  may not do what it offers. The folder path is being corrected.

## Replay

- **Replaying against a live app that has shifted may need a re-find.** A saved
  workflow repeats each step at the place it learned. If you replay against an
  app whose layout has moved, replay may need to find the element again by its
  identity rather than its old position. The taught demo and example paths are
  exact today; live re-finding at run time is being added, and it stays on your
  machine with no model calls.

- **A correction made in the middle of a run may not stick the same way.** If you
  step in and correct Operant while a run is happening, that correction may not
  fold into the saved workflow the same way a correction recorded ahead of time
  does. This is an internal naming mismatch that is being reconciled.

## Undo

- **There is no dedicated undo screen yet.** Undo works: every write action
  records how to reverse itself before it runs, and anything without a safe
  reverse is labeled before you run it. But there is not yet a dedicated screen
  to preview and confirm undoing a whole run; it is driven from the run view. A
  dedicated screen is being added.
