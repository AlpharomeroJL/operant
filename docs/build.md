# Building Operant

This machine is the only gate (no hosted CI). `just verify` plus the pre-push
hook is the whole gate.

## Prerequisites

- Rust (cargo, rustc). This repo builds on the pinned toolchain in the root
  `Cargo.toml` (`rust-version`). `cargo` and `rustc` must be on PATH.
- Node (for the UI: vite, vitest, the token pipeline).
- `just` the command runner. Install with `cargo install just`.
- NSIS (`makensis`) only for building the Windows installer. The Tauri v2 bundler
  provisions its own NSIS on first `cargo tauri build --bundles nsis` (downloaded to
  a local Tauri cache), so a separate install is usually not needed. If you want it
  on PATH directly: `choco install nsis` (needs an elevated shell) or unzip the NSIS
  portable build into a directory on PATH.
- Ollama, only for exercising a real local model backend (Phase C1/C2): the tray app
  plus `ollama serve` listening on `http://localhost:11434`.

## Environment on this machine

The repo lives off OneDrive at `D:\dev\operant`. Two env vars matter:

- `CARGO_TARGET_DIR=D:\dev\operant-target` - keep build output off OneDrive.
- `CARGO_HOME=D:\dev\cargo` - where `cargo install` places binaries, so `just` is at
  `D:\dev\cargo\bin\just.exe`. Put `D:\dev\cargo\bin` (and the rustup `bin`) on PATH.

Bash prefix used throughout this repo:

```
export PATH="/d/dev/cargo/bin:$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="D:/dev/operant-target"
```

## Gates

- `just verify` = `ci golden ui`: the full local gate that must be green before every
  push. `just ci` = `build test check-json check-emdash check-microcopy check-airgap
  check-rawhex`. `just golden` runs the golden-path e2e. `just ui` runs the token
  build, typecheck, vitest (incl. the axe accessibility scans), and the vite build.
- `just claims` is a standalone gate (not in `verify`) that checks every published
  capability claim against a passing test or evidence doc (`CLAIMS.md`).
- The pre-push hook (`hooks/pre-push`, installed by `just setup` which sets
  `core.hooksPath hooks`) runs `just verify` on every push. Never pass `--no-verify`.

## Feature flags

The default build is headless and mock (perception, input, and transport are fixture
backed) so `cargo test` runs anywhere. The real Windows build turns on:

- `real-uia` - live UIA perception (the `windows` crate; `UiaPerceiver`).
- `real-input` - live input synthesis (`WindowsSynthesizer`).
- `real-transport` - the stdio NDJSON sidecar transport.
- `dev-agent-bridge` - DEV ONLY. Routes the explore planner to a filesystem rendezvous
  so an operator (or Claude) can drive a teach without an API key. NEVER in a release
  build; the release capability gate (`just check-release-artifact`) fails a build that
  links a mock into the shipped execution path.

Example real build of the core sidecar:

```
cargo build -p operant-cli --features real-uia,real-input,real-transport
```

## The app and the installer

- Dev run: `cargo tauri dev` from `ui/src-tauri` (needs `npm ci` in `ui/` first for
  vite). Point it at a real core with `OPERANT_CORE_BIN`.
- Installer: `just package` (`cd ui; npm ci; cargo tauri build --bundles nsis` then
  `just sign`). The NSIS installer lands under `D:\dev\operant-target\release\bundle\nsis`.
