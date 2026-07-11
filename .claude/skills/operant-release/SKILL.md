---
name: operant-release
description: Signing, packaging, docs deploy, and GitHub release procedure.
---
Single-source NSIS bundle only (ADR-0194 lineage). Ed25519 updater keypair: private key
to the OS vault path documented in release/KEYS.md, NEVER committed. CI asserts the
update endpoint is live in release-profile builds (ADR-0193 lineage). Docs site deploys
to GitHub Pages from site/. gh release create v1.0.0 with installer, checksums, SBOM,
notes assembled from merged DECISIONS lines. After upload, download the asset back and
verify its signature. NOTE: no Authenticode certificate exists on the build machine, so
the installer is Ed25519-updater-signed only, not OS-code-signed; state this plainly in
release notes and release/KEYS.md.
