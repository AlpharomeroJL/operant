# E1 truth gate: the installed binary

This records the E1 checks run against the binary produced by the actual NSIS
installer, plus honest pointers to where each remaining capability is proven on
the identical engine. It is the release gate evidence for v1.1.0.

The rule for this document: it states only what was directly observed, and where
a capability is proven elsewhere it says so and points at the proof rather than
restating it as if re-run here.

## The artifact under test

- Installer: `Operant_1.1.0_x64-setup.exe` (NSIS, 6.88 MB), built by
  `just package` (which runs `just stage-core` first, so the real core is
  bundled as an external binary).
- Installed to `%LOCALAPPDATA%\Programs\Operant` (per-user, silent).
- Install directory contents after install:
  - `operant-shell.exe` (13.5 MB) - the Tauri shell.
  - `operant.exe` (11.65 MB) - the core, placed by Tauri's `externalBin`.
  - `uninstall.exe` - the NSIS uninstaller.

## Directly verified on the installed binary

Launched the installed `operant-shell.exe` with no `OPERANT_CORE_BIN` set, so the
shell had to find and spawn its bundled core on its own. Ollama env
(`OPERANT_LIVE_BACKEND=1`, provider `ollama`, model `qwen3:8b`) was set so the
core would drive exploration with a real local model.

1. **Standalone launch, bundled core spawned.** Both `operant-shell.exe` and
   `operant.exe` were running from the install directory
   (`%LOCALAPPDATA%\Programs\Operant`). The shell resolved and spawned its bundled
   core with no external path hint. `externalBin` bundling works end to end: the
   installer ships the core and the installed shell runs it.
2. **Real capability handshake, no mock in the shipped path.** `get_capabilities`
   through the bundled core returned:

   ```json
   {"mock_planner_only": false, "real_input": true, "real_uia": true,
    "real_vision": false, "transport_kind": "stdio", "version": "1.0.0"}
   ```

   `mock_planner_only` is `false` on the installed binary: the shipped build has
   no mock planner in its execution path. (`version` here is the frozen IPC
   contract version, not the app version; the app and installer are 1.1.0.)
3. **Real UI mounted, not the capability-block screen.** The webview reported
   `__TAURI_INTERNALS__` present and no capability-block or mock banner on screen.
4. **The target picker works live on the installed binary.** `list_windows`
   through the installed app returned 10 real top-level windows including a live
   Notepad. This is the A1 picker (ADR 0003) enumerating real windows from the
   installed shell's bundled core, so a teach can bind to the app the user means.

## Proven on the identical engine (pointers, not re-run here)

The installer bundles the same core crate that the dev-staged core is built from
(`just stage-core` and `just package` compile the same `cli` binary). These were
proven live earlier this campaign on that identical engine:

- **Teach with a real model, real Action IR, gated and executed.** `qwen3:8b`
  over Ollama drove the explore loop and proposed three valid Action IR steps that
  were gated and executed against a real Notepad. See
  [P0-live-engine.md](P0-live-engine.md) and the C1 notes. The fix that made this
  work (real tool schema sent to the planner, tolerant Action IR defaults) is in
  the shipped core.
- **Compile and model-free replay.** A compiled workflow replays with zero model
  and zero network calls, asserted structurally in CI (`replay` crate is
  backend-free) and by the run's own measured model-call counter reading zero on
  replay. See [P2-live-app-proof.md](P2-live-app-proof.md).
- **Live run streaming to the viewer (B2).** The webview attaches to the bus via
  the `core:event` capability grant (`ui/src-tauri/capabilities/default.json`) and
  renders steps as they happen; the earlier "nothing has run yet" was that missing
  grant, now fixed.
- **Undo, safety invariants, kill switch.** Undo replays recorded inverses to a
  byte-identical prior state; safety invariants halt on payment/delete/password
  fields; the kill switch freezes under 100 ms below the planner. These are unit
  and integration tested and each is a backed row in `CLAIMS.md`.

## Honestly not proven / not wired

- **Scheduling from the app is not wired.** The scheduling engine (cron,
  file-watch, unattended replay) is built and tested, but the app returns
  not-implemented for starting a schedule. Documented in `docs/KNOWN_ISSUES.md`
  and the v1.1.0 release notes. "Run a saved task again" (quick-run) is wired.
- **A full unattended 12-step GUI drive was not scripted on the installed binary
  in one pass.** The installed binary is proven to launch standalone, spawn its
  real core, and enumerate windows live; the teach/compile/replay/undo/safety/
  panic capabilities are proven on the identical engine as above rather than
  re-driven click-by-click on the installed binary here.
- **OAuth sign-in and voice were skipped by the owner** for this release and are
  out of scope for v1.1.0.

## Verdict

The binary from the real installer launches standalone, spawns its bundled real
core, reports itself real (no mock in the shipped path), mounts the real UI, and
enumerates real windows for the target picker. The engine capabilities it fronts
are proven live on the identical core. The gaps above are documented as known
issues rather than presented as working. On that basis v1.1.0 ships, unsigned,
with the SmartScreen unknown-publisher note in the release notes and
KNOWN_ISSUES.
