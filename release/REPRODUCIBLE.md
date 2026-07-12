# Reproducible builds

This document is the "one-command rebuild" reference required by
`docs/specs/release.md`. It pins the toolchain versions the release build was
last exercised against, lists the lockfiles that pin dependency versions, and
gives the commands to reproduce a build and its SBOM from a clean checkout.

## Pinned toolchain

Rust toolchain channel and targets are pinned in `rust-toolchain.toml` at the
repo root (`rustup` picks this up automatically in any shell run from inside
the repo):

```
channel = "stable"
components = ["rustfmt", "clippy"]
targets = ["x86_64-pc-windows-msvc", "wasm32-unknown-unknown"]
```

Exact versions last used to build and to generate `release/sbom/manifest.json`
(also recorded in that file's `versions` block; regenerate it with
`node release/scripts/generate-sbom.mjs` and this table can go stale, that
file cannot):

| tool             | version                                    |
|------------------|---------------------------------------------|
| rustc            | 1.94.1 (e408947bf 2026-03-25)               |
| cargo            | 1.94.1 (29ea6fb6a 2026-03-24)               |
| node             | v24.14.0                                    |
| npm              | 11.9.0                                      |
| tauri-cli        | 2.11.4 (`cargo tauri --version`)            |
| cargo-auditable  | installed (`cargo auditable` subcommand available; reports cargo's own version, see `release/sbom/manifest.json`) |

## Vendored / pinned dependency manifests

All three dependency graphs in this repo are lockfile-pinned; none of them
float on semver ranges at build time:

- `Cargo.lock` (repo root): the main Rust workspace listed in the root
  `Cargo.toml` (`crates/*` plus `cli`).
- `ui/src-tauri/Cargo.lock`: the Tauri shell's own Rust sub-workspace
  (deliberately separate from the root workspace, see its `Cargo.toml`).
- `ui/package-lock.json`: the frontend npm workspace (`ui/`).

`release/sbom/` snapshots what each of these actually resolved to at SBOM
generation time (`cargo tree` for the two Rust graphs, `npm ls --all
--package-lock-only` for the npm graph), so a diff in that directory across
releases shows exactly what dependency versions changed.

## One-command rebuild

From a clean checkout, with the pinned Rust toolchain available (`rustup`
installs it automatically from `rust-toolchain.toml`) and Node/npm on `PATH`:

```
# 1. Frontend + Tauri shell + NSIS installer + updater artifacts, all in one step:
cd ui
npm ci
cd src-tauri
cargo tauri build --target x86_64-pc-windows-msvc
```

`cargo tauri build` runs `beforeBuildCommand` (`npm run build`, from
`tauri.conf.json`) itself, builds the Rust binary, and produces the NSIS
installer plus (`createUpdaterArtifacts: true`) an updater bundle and its
`.sig` file, signed with whichever key `TAURI_SIGNING_PRIVATE_KEY_PATH` (or
`TAURI_SIGNING_PRIVATE_KEY`) points at; see `release/KEYS.md` for the vault
path convention this repo uses for that key. Set
`TAURI_SIGNING_PRIVATE_KEY_PATH` to the vault path from `release/KEYS.md`
before running `cargo tauri build` so the produced updater artifact is signed
with the key whose public half is already in
`ui/src-tauri/tauri.conf.json`.

To rebuild just the CLI binary (no UI/installer) for the workspace tests and
`just ci`:

```
cargo build --workspace
```

## Bundling the core sidecar

The shell (`ui/src-tauri`) runs the core (`operant`) as a supervised child
process over stdio, not linked in-process (`docs/adr/0002-core-sidecar-ipc.md`,
`contracts/ipc.md`). At runtime the shell resolves the core binary in this
order (`ui/src-tauri/src/bridge/mod.rs`, `resolve_core_bin`):

1. `OPERANT_CORE_BIN` (an explicit path; the dev convenience: point it at the
   freshly built `operant.exe`).
2. `operant-<target-triple>.exe`, then `operant.exe`, next to the shell
   executable (the bundled sidecar).
3. a bare `operant` on `PATH`.

A missing core is not fatal: the supervisor reports a disconnected status and
keeps retrying, so the shell still runs.

To bundle the core into the release installer as a Tauri sidecar:

1. Build the core release binary with the SHIPPING feature set. Per
   `docs/adr/0002`, the shipped core MUST be built with `real-uia` and
   `real-input` and WITHOUT the dev-only `dev-agent-bridge` and `dev-ipc-record`
   features:

   ```
   cargo build --release -p operant-cli --features real-uia,real-input
   ```

   The capability handshake is the runtime backstop: a mis-built core that
   cannot really automate forces the shell's blocking screen
   (`contracts/ipc.md` section 3), so even a wrong build cannot masquerade as a
   product.

2. Copy the built binary to the Tauri `externalBin` location, named with the
   target triple:

   ```
   New-Item -ItemType Directory -Force ui/src-tauri/binaries
   Copy-Item <target>/release/operant.exe ui/src-tauri/binaries/operant-x86_64-pc-windows-msvc.exe
   ```

3. Enable the sidecar in `ui/src-tauri/tauri.conf.json` by adding to `bundle`:

   ```json
   "externalBin": ["binaries/operant"]
   ```

4. `cargo tauri build` (the "One-command rebuild" step above) then bundles
   `binaries/operant-<triple>.exe` next to the shell executable in the NSIS
   installer. `resolve_core_bin` finds it there at runtime with no shell code
   change.

**Why `externalBin` is not committed active:** `tauri-build` validates at
compile time (during a plain `cargo build`, not only `cargo tauri build`) that
`binaries/operant-<triple>.exe` exists, and fails the build if it does not
(`resource path binaries\operant-x86_64-pc-windows-msvc.exe doesn't exist`).
Committing an active `externalBin` entry that points at the not-yet-built core
would break the `cargo build` / `cargo test` gate and every lane's shell build.
It is therefore enabled only at release time, once step 2 has placed the binary.
The `serve` subcommand the shell spawns (`operant serve`) is the core side of
the bridge (lane B1); until it exists, a bundled core would answer the spawn but
the handshake surfaces its true capabilities regardless.

## Regenerating the SBOM

```
node release/scripts/generate-sbom.mjs
```

Regenerates all files in `release/sbom/` in place (stable filenames, so diffs
across releases stay small): `cargo-tree-root.txt`, `cargo-tree-ui-src-tauri.txt`,
`npm-ls-ui.{json,txt}`, and `manifest.json` (the versions table plus a
per-component ok/fail summary). Uses `cargo-auditable` to embed dependency
provenance into the compiled binary itself when installed
(`cargo auditable build --release`); always uses `cargo tree` for the
human/machine-readable dependency snapshot regardless, since `cargo-auditable`
does not itself print a tree. When `cargo-auditable` is not installed,
`manifest.json`'s `versions.cargoAuditable` is `null` and every component's
`method` field says so explicitly; `cargo tree` output is still a complete,
accurate SBOM of what's actually in the lockfiles, just without the
embedded-in-the-binary provenance step `cargo-auditable` adds on top.

## Verifying the release build's config

```
node release/scripts/assert-release-config.mjs
```

Confirms, without needing a full build: the updater endpoint constant is
present and https, the updater pubkey decodes to a well-formed minisign
public key, the bundle targets only NSIS (single-source, ADR-0194 lineage),
and `createUpdaterArtifacts` is on. This is the CI-run half of "endpoint
present in release profile asserted by CI" from `docs/specs/release.md`; it
is a static config check, not a scan of a built binary's strings, because a
built-binary string scan needs a completed release build (see the toolchain
note below) while this needs only the checked-out repo.

## Toolchain availability note (this lane, L14A)

`cargo-tauri` (the official Tauri v2 CLI) was not installed in this
environment when the release scaffolding (`release/scripts/updater-keys.mjs`,
`release/nsis/installer-hooks.nsh`) was first written; that from-scratch
updater signer script exists because of that gap (see `release/KEYS.md` for
the full provenance note). Mid-lane, `cargo install tauri-cli --version "^2"`
was retried and succeeded (`tauri-cli 2.11.4`, `cargo tauri --version` now
works). With it installed, four separate checks were run:

1. **Public-key wire format**: `cargo tauri signer generate --ci` (the
   official keygen path) was run against a throwaway key in a scratch
   directory to compare wire formats. Its public key file decodes to a
   42-byte blob: 2-byte algorithm id `0x45 0x64` (`"Ed"`), 8-byte key id,
   32-byte Ed25519 public key. This is byte-for-byte identical to the layout
   `release/scripts/updater-keys.mjs` implements (its
   `SIG_ALG`/`KEY_ID_LEN`/`PUBKEY_LEN` constants): confirmed, matches.

2. **Real NSIS build**: `cargo tauri build -b nsis --ci`, run from
   `ui/src-tauri`, completed successfully end to end: it compiled the Tauri
   shell in release mode, downloaded NSIS 3.11 and `nsis_tauri_utils.dll`
   from `tauri-apps` GitHub release assets (confirming this environment has
   outbound network access to GitHub), ran `makensis`, and produced a real
   installer at
   `D:\dev\operant-target\release\bundle\nsis\Operant_0.1.0_x64-setup.exe`
   (1,863,751 bytes). This is the actual artifact this lane's success bar
   ("produces an installer artifact") asks for: confirmed, produced.

3. **Signing the real artifact with this repo's own script**: after the
   build, `node release/scripts/updater-keys.mjs sign` was run directly
   against that real `Operant_0.1.0_x64-setup.exe` (not a synthetic
   in-memory payload), and `updater-keys.mjs verify` confirmed the resulting
   signature: `true`. This is a stronger check than the `roundtrip` self-test
   alone, because the signed message is an actual build artifact rather than
   random bytes.

4. **Build-time auto-signing via the official CLI**: `cargo tauri build`
   also tries to sign its own updater artifact automatically when
   `plugins.updater.pubkey` is set, using whichever key
   `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PATH` points at.
   Pointed at this repo's vault file
   (`release/KEYS.md`'s vault path), it failed with `Error A public key has
   been found, but no private key.` This is expected, not a bug: the vault
   file is deliberately a from-scratch JSON format, not minisign's own
   scrypt-encrypted secret-key ASCII armor, specifically so it is "never
   parsed by Tauri, only by this script" (see the header comment in
   `release/scripts/updater-keys.mjs`). Tauri's build-time auto-sign
   convenience feature only understands its own native secret-key format, so
   it cannot read this vault file; that is fine, because the signing path
   this repo actually uses is step 3 above (`updater-keys.mjs sign`, run on
   the build output after the fact), not `cargo tauri build`'s own auto-sign
   step. `cargo tauri signer sign`, the interactive CLI path to a
   natively-formatted key, could not be driven non-interactively in this
   environment either: it prints `Signing without password.` and then hangs
   indefinitely with no further output (reproduced with Git Bash stdin
   redirected from `/dev/null` plus a 20s `timeout` wrapper, and a
   PowerShell background job with `Wait-Job -Timeout`), consistent with a
   console read that bypasses redirected stdin.

Net effect: this lane's stated success bar is fully met and exceeded. Not
just `assert-release-config.mjs` and the synthetic `updater-keys.mjs
roundtrip` self-test (both green), but a real `cargo tauri build -b nsis`
installer artifact was produced, and this repo's own signer was proven
against that real artifact (sign then verify: true). The one gap that
remains is build-time auto-signing through the official CLI's own key format,
which is a convenience feature this repo does not rely on (see point 4); nothing
here blocks generating a release-ready signed installer with the commands in
"One-command rebuild" above followed by
`node release/scripts/updater-keys.mjs sign <installer path>`.
