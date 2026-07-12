# Updater signing keys

Operant's auto-updater (`tauri-plugin-updater`) verifies every downloaded update
artifact against an Ed25519 public key baked into `ui/src-tauri/tauri.conf.json`
(`plugins.updater.pubkey`). The matching private key signs release artifacts at
build/ship time and must never be committed to this repository.

## No OS code signing (Authenticode)

There is no Authenticode (Windows OS) code-signing certificate available on this
build machine, so the NSIS installer itself is not OS-signed. Windows SmartScreen
will therefore show an "unknown publisher" warning the first time a user runs the
installer (choose "More info" then "Run anyway" to proceed).

This is a separate trust mechanism from the updater. The installer's OS-level
signature is absent; the auto-updater's integrity instead relies entirely on the
Ed25519 signature documented below, which `tauri-plugin-updater` verifies before
applying any update. Shipping an OS-signed installer would require purchasing an
Authenticode certificate. Until that exists, the release notes must state plainly
that the installer is unsigned so people downloading it expect the SmartScreen
warning (see `release/RELEASE_NOTES_TEMPLATE.md`).

## Where the private key lives

The private key is generated to, and read from, a per-user vault path outside
the repository:

- Windows (this build machine): `%LOCALAPPDATA%\Operant\updater-keys\updater_ed25519.key`
  (currently `C:\Users\<you>\AppData\Local\Operant\updater-keys\updater_ed25519.key`)
- macOS/Linux: `~/.local/share/operant/updater-keys/updater_ed25519.key`
- Override on any platform with the `OPERANT_UPDATER_KEY_PATH` environment
  variable (used in CI so a runner can point at a secrets-mounted path instead
  of the default user profile location).

That path is outside `release/` and outside the git worktree entirely, so
normal `.gitignore` review of tracked paths never even sees it. As a second
line of defense, `.gitignore` also excludes `*.key` (see `release/private/`
and the top-level `*.key` rule) in case a private key file is ever copied
somewhere under the repo for inspection.

The private key file is JSON, written with `0o600` permissions on POSIX (on
Windows the ACL inherited from the per-user `LOCALAPPDATA` profile directory
is the real protection boundary; `chmod`-equivalent bits are not meaningful
there, see the comment in `release/scripts/updater-keys.mjs`). It can
optionally be encrypted at rest with a passphrase (AES-256-GCM, scrypt-derived
key) by setting `OPERANT_UPDATER_KEY_PASSPHRASE` before running `generate` (and
again before `sign`/`roundtrip`). The key currently on this machine was
generated without a passphrase, relying on the per-user profile ACL as the
access boundary; set a passphrase before this key is used from a shared build
machine or CI runner.

## Format

The public key file (`release/keys/updater_pubkey.pub`) and the signatures
produced by `release/scripts/updater-keys.mjs sign` use the same wire format
`tauri-plugin-updater` expects: minisign's legacy (non-prehashed) "Ed" format,
i.e. a 2-byte algorithm id + 8-byte key id + 32-byte Ed25519 public key for the
key file, and algorithm id + key id + 64-byte signature + trusted-comment line
+ 64-byte global signature (over signature bytes + trusted comment) for a
`.sig` file.

`ui/src-tauri/tauri.conf.json`'s `plugins.updater.pubkey` value is the base64
encoding of the *entire* public key file's text (comment line and all), which
is what `release/keys/updater_pubkey.tauri-config-value.txt` holds and what
`node release/scripts/updater-keys.mjs print-pubkey` prints. Do not paste the
raw 32-byte key or the `.pub` file's bytes directly; Tauri expects the base64
wrapper.

## How the pubkey landed in `ui/src-tauri/tauri.conf.json`

1. `node release/scripts/updater-keys.mjs generate` created the keypair, wrote
   the private key to the vault path above, and wrote
   `release/keys/updater_pubkey.pub` plus
   `release/keys/updater_pubkey.tauri-config-value.txt` (both safe to commit;
   they contain only public key material).
2. The contents of `updater_pubkey.tauri-config-value.txt` were copied
   verbatim into `plugins.updater.pubkey` in `ui/src-tauri/tauri.conf.json`.
3. `node release/scripts/assert-release-config.mjs` confirms the config's
   pubkey decodes to a well-formed minisign public key file and that the
   updater endpoint, bundle target, and `createUpdaterArtifacts` flag all
   match `docs/specs/release.md`.

The current key id is `CEE33C7F3B56D5CA` (see
`release/keys/updater_pubkey.pub`, first line).

## Regenerating or rotating the key

Rotating the key invalidates every previously-signed update artifact still
being served, so only do this alongside a full release cut (old clients will
stop being able to verify new updates until they update once more from a
build signed with the new key, or the update manifest is republished under
both keys during a transition window).

```
node release/scripts/updater-keys.mjs generate --force
```

Then:

1. Copy the new value out of
   `release/keys/updater_pubkey.tauri-config-value.txt` into
   `plugins.updater.pubkey` in `ui/src-tauri/tauri.conf.json`.
2. Run `node release/scripts/assert-release-config.mjs` to confirm the config
   is well-formed.
3. Run `node release/scripts/updater-keys.mjs roundtrip` to confirm the new
   key can sign and verify.
4. Commit the updated `release/keys/*.pub`, `*.tauri-config-value.txt`, and
   `ui/src-tauri/tauri.conf.json` together, and update the key id noted above.
5. Back up the private key vault file out of band (password manager or
   equivalent secret storage); losing it means the next release cannot be
   signed with a key existing clients already trust.

## Signing and verifying real artifacts

```
node release/scripts/updater-keys.mjs sign <path-to-artifact> [--out <path>.sig] [--comment "..."]
node release/scripts/updater-keys.mjs verify <path-to-artifact> [--sig <path>.sig] [--pubkey release/keys/updater_pubkey.pub]
node release/scripts/updater-keys.mjs roundtrip
```

`roundtrip` signs and verifies a throwaway in-memory artifact end to end and
exits non-zero if verification fails; it is the automated part of this lane's
success bar.

## Implementation provenance and the one open validation gap

`release/scripts/updater-keys.mjs` is a from-scratch Node implementation of
the minisign wire format, written because `cargo tauri` (the official Tauri
CLI, which ships the `tauri signer` subcommand and would otherwise be the
source of truth for this format) was not installed in the environment this
was first built in. Its byte layout was cross-checked against the public
`minisign-verify` crate source (the crate `tauri-plugin-updater` itself uses
to parse signatures) and against a `tauri-plugin-updater` test fixture.

Status as of this lane (see `release/REPRODUCIBLE.md` for the full toolchain
attempt log): `cargo install tauri-cli --version "^2"` was retried and
succeeded (`tauri-cli 2.11.4`). With the official CLI available:

- `cargo tauri signer generate --ci` was used to generate a throwaway
  official key, and its public key file decoded to the exact same 42-byte
  wire layout (`"Ed"` algorithm id, 8-byte key id, 32-byte Ed25519 key) that
  this script produces: the public-key format matches, confirmed.
- A real `cargo tauri build -b nsis --ci` was run and produced an actual NSIS
  installer (`Operant_0.1.0_x64-setup.exe`). This script's `sign` command was
  then run directly against that real installer file, and `verify` confirmed
  the signature: a genuine artifact was signed and verified, not just the
  synthetic payload the `roundtrip` self-test uses.
- The one thing that did not work: pointing `cargo tauri build`'s own
  build-time auto-sign feature (`TAURI_SIGNING_PRIVATE_KEY_PATH`) at this
  vault file. That failed, as expected, because the vault file's format is
  intentionally not minisign's native secret-key ASCII armor (see "Where the
  private key lives" above); Tauri's auto-sign step only reads its own
  native format. This does not affect real releases, because the signing
  step this repo actually uses is this script's own `sign` command run on
  the build output afterward, which the point above confirms works against
  a real artifact.

This from-scratch implementation is validated against the real `tauri-cli`
both on wire format (public key bytes match exactly) and end to end (signed
and verified a real build artifact). It remains a from-scratch
reimplementation rather than a call-through to the official signer, so keep
this note attached to any future format changes to
`release/scripts/updater-keys.mjs`.
