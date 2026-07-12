# Operant v1.0.0

Teach your computer once. It does it forever. No code. Free.

Operant is a local-first desktop agent that compiles AI trajectories into
deterministic, invariant-gated automations. You explore a task once with a model;
Operant freezes the successful run into an inspectable workflow that replays in
milliseconds, offline, with zero inference cost. The model is a compiler, not a runtime.

## What is in this release

- The moat: trajectory compiler plus deterministic model-free replay (CI-asserted zero
  model and zero network calls), and the full drift-repair loop (detect, re-ground,
  approve, versioned merge).
- Perception (Windows UIA plus fixtures), action with adapters (filesystem, email,
  OCR/PDF, Office, browser/CDP), SQLite recorder with WAL and content-addressed blobs.
- Safety: capability grants, dry-run, hash-chained audit with JSONL and PDF export, and
  runtime-enforced credential and payment invariants no workflow can disable.
- Guardian set: sub-100ms kill switch, undo journal, anchor redaction.
- Zero-code layer: onboarding wizard, plain-English workflow view, demo mode, template
  gallery, doctor, first-run tour, Spanish locale scaffold. The first-timer path (install,
  teach, save, run, schedule, all in default mode) is verified end to end on the installed
  build.
- Model backends: local and API providers behind one client with a 16-provider quirk
  table, plus subscription sign-in (ChatGPT and Claude plans) with tokens in the OS vault.
- CLI and MCP both directions, scheduler, benchmark harness, and a signed workflow registry.

## Determinism, measured

Compiled replay runs each benchmark task 5/5 with zero model calls at roughly 0 to 1 ms
per step, versus model re-inference at 6 to 7 ms per step and 15 to 25 model calls per
task. See BENCHMARKS.md in the repository.

## Before you install

This installer is not OS code-signed. There is no Authenticode certificate for this
project yet, so Windows SmartScreen will warn about an "unknown publisher" the first time
you run it. Choose "More info" then "Run anyway". Your safety here does not depend on that
OS signature.

## Verify your download

- `SHA256SUMS` (attached) covers every asset.
- `Operant_0.1.0_x64-setup.exe.sig` is an Ed25519 signature over the installer; verify it
  with `node release/scripts/updater-keys.mjs verify <installer> --sig <installer>.sig`
  against `release/keys/updater_pubkey.pub`.
- The SBOM (`cargo-tree-*.txt`, `npm-ls-ui.*`) lists the exact dependency versions.

## Honest scope

This build ships the full v1.0.0 launch cut. A few items are fixture or mock verified in
CI (vision and voice grounding), the demo video is captioned rather than voiced (the voice
sidecar ships a mock TTS), and one launch asset is a labeled placeholder. Known issues and
the full outcome are documented in `docs/adr/0001-campaign-outcome.md`.

Apache-2.0. Docs: https://alpharomerojl.github.io/operant/
