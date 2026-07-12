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
