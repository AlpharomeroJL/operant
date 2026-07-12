# Operant v{VERSION}

{Release notes are assembled here from the merged DECISIONS lines per
docs/specs/release.md. Keep the two sections below in every release.}

## Before you install

This installer is not OS code-signed. There is no Authenticode certificate for
this project yet, so Windows SmartScreen will warn about an "unknown publisher"
the first time you run the installer. Choose "More info" then "Run anyway" to
proceed. Your safety here does not depend on that OS signature: every automatic
update Operant downloads is verified against the project's Ed25519 key before it
is applied, and the SHA256SUMS file attached to this release lets you check the
installer bytes yourself. See `docs/install.md` for the full walkthrough with a
screenshot of the SmartScreen dialog, and `docs/signing.md` for the plan to get
a signed installer.

## Verify your download

- `SHA256SUMS` (attached) covers the installer and every other asset.
- The Software Bill of Materials (`sbom/`) lists the exact dependency versions.
- Auto-updates are Ed25519-signed and verified before install (see
  `release/KEYS.md`).
