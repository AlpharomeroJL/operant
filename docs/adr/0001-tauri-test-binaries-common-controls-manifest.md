# 1. Tauri test and example binaries need the Common-Controls v6 manifest on Windows

Status: Accepted

Date: 2026-07-12

## Context

The Tauri shell in `ui/src-tauri` could not be tested with `cargo test`. Any
`cargo test` binary in that crate, and likewise any `[[example]]` binary,
crashed at process load on Windows with STATUS_ENTRYPOINT_NOT_FOUND
(0xC0000139) before a single test function ran. A bare
`tauri::Builder::default().build(tauri::generate_context!())` with no plugins
registered was enough to trigger it. The same call compiled as a normal `[[bin]]`
target ran fine.

We first recorded this as a machine-specific or sandbox-specific toolchain
quirk and worked around it by shipping the updater verification as a `[[bin]]`
behind a `fixture-check` feature instead of a real test. That description was
wrong. The behavior is deterministic Tauri-on-Windows behavior and reproduces
on any standard Windows toolchain. This ADR records the actual root cause and
the fix so the workaround does not get reintroduced.

### Root cause

Two layers, verified by direct experiment.

Load-time failure:

- tao, the windowing layer beneath Tauri, statically imports
  `comctl32!TaskDialogIndirect`.
- `TaskDialogIndirect` is only exported by the Common Controls version 6
  assembly, which lives in the WinSxS store. The `comctl32.dll` in System32 is
  version 5.82 and does not export that symbol.
- Which comctl32 a process binds is selected by an application manifest that
  declares a dependency on `Microsoft.Windows.Common-Controls` version
  6.0.0.0. Without that manifest, the loader binds the System32 v5.82 DLL,
  fails to resolve the statically imported entry point, and terminates the
  process at load with 0xC0000139.
- tauri-build embeds the Common-Controls v6 application manifest, but only for
  binary targets. It emits `cargo:rustc-link-arg-bins`, which applies to
  `[[bin]]` targets only. Test binaries and example binaries are built by a
  different target kind and receive no manifest, so they bind v5.82 and crash.
  This is why a normal `[[bin]]` ran while the identical code under `cargo test`
  or as an `[[example]]` did not. It was never about which directory the output
  landed in.

Run-time requirement, exposed once loading is fixed:

- The real Wry event loop (again, tao underneath) must be created on the main
  thread. `cargo test` runs each test function on a worker thread, so
  constructing the real runtime inside a test panics.

## Decision

1. Embed the Common-Controls v6 manifest for the test and example target kinds
   in `ui/src-tauri/build.rs`, in addition to the bin manifest that tauri-build
   already provides. After `tauri_build::build()`, for each of `tests` and
   `examples`, and only when a matching `.rs` source actually exists, emit:

   - `cargo:rustc-link-arg-tests=/MANIFEST:EMBED` (and the `-examples` form)
   - `cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:...Common-Controls...version='6.0.0.0'...`

   The existence check is required. cargo rejects a `rustc-link-arg-tests` or
   `rustc-link-arg-examples` directive for a target kind that has no source,
   with "does not have a test/example target", which would break a plain
   `cargo build`. The check is gated on `#[cfg(windows)]` because the manifest
   is meaningful only on Windows.

2. Write updater tests against `MockRuntime` via
   `tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets()))`
   rather than the real runtime. MockRuntime does not create a tao event loop,
   so it has no main-thread requirement and runs on cargo's test worker
   threads. This requires the `test` feature on the `tauri` dev-dependency.

With both in place, `tests/updater_signature.rs` builds a real Tauri app and
exercises the updater end to end: a fixture HTTP server, `check()`, a staged
`download()` with Ed25519 signature verification, reaching the real
`Update::install`, plus a separate tampered-manifest rejection case.

## Consequences

- The updater verification is now a real `cargo test` integration test instead
  of a `[[bin]]` behind a `fixture-check` feature. That feature and the
  `signed_artifact_check` bin have been removed, and `tiny_http` and `url`
  moved to `[dev-dependencies]`.
- `cargo test` and future `[[example]]` binaries in `ui/src-tauri` load and run
  on Windows.
- A plain `cargo build` is unaffected, because the build script only emits the
  test and example link args when those sources exist.
- Tests must use `MockRuntime`. A test that needs the real Wry runtime and a
  real window is still out of reach of `cargo test` and belongs in a manual or
  bin-based check run on the main thread.
- `ui/src-tauri` remains its own Cargo workspace and is not built by the
  repo-root `just ci`. It is built and tested on its own with `cargo build` and
  `cargo test` inside `ui/src-tauri`.
