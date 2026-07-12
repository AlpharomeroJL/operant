# Release build matrix

This is the structural "no mock ships as product" rule for Operant's core
binary. It says which cargo features each build MUST and MUST NOT carry, and it
is enforced by a release gate that reads a built binary's own reported
capabilities and refuses a mock artifact.

The core binary is `operant` (`cli`), the same binary the app runs as a
supervised sidecar (`contracts/ipc.md` section 0). What it can do is fixed at
compile time by cargo features, and it reports those capabilities over the
`get_capabilities` handshake (`contracts/ipc.md` section 3). Capability follows
build cfg; it is never asserted in a doc.

## The matrix

| Build | Features | real_uia / real_input | Purpose |
|---|---|---|---|
| Default (mock) | none | false / false | `just golden`, `just ci`, `just verify`, every test. Deterministic, offline, model-free. This is the build the whole gate runs on. |
| Release (shipped) | `real-uia,real-input,real-transport` | true / true | The installed app's core. It can actually perceive and drive the live Windows desktop. Built by `just build-release-core`. |
| Dev harness | adds `dev-agent-bridge` and/or `dev-ipc-record` | either | Local proof/record harnesses only (P0 live-engine proof, IPC fixture capture). NEVER in a release build. |

### Why each row is what it is

- **Default stays mock.** `real-uia` and `real-input` gate the heavy `windows`
  crate, the live UIA perceiver, and the real `SendInput` synthesizer. The
  default build links none of them, so `cargo build --workspace`, `just golden`,
  and every test replay against the deterministic mock synthesizer, headless and
  reproducible. This is what keeps the determinism proof (`just golden`) green
  and the air-gap invariant (`scripts/check_airgap.mjs`) honest.

- **The shipped release needs BOTH real features.** A real run needs live
  perception (`real-uia`) AND real input (`real-input`) together. Either one
  alone silently degrades to the mock path, which is exactly the failure this
  matrix exists to prevent, so `cli/src/commands/run.rs` makes "exactly one of
  the two" a hard `compile_error!` (the E4 rule). `real-transport` is included so
  the release feature set is uniform with the crates that do own a network
  transport (the orchestrator's teach path); on the CLI core it is inert
  (`cli/Cargo.toml`), so the shipped `operant run` stays deterministic and
  offline regardless.

- **Dev features never ship.** `dev-agent-bridge` (human/model-as-planner bridge)
  and `dev-ipc-record` (IPC fixture recorder) are opt-in development harnesses.
  They are excluded from the release feature set above, and the release gate
  additionally refuses any binary that still answers the dev-only `record-ipc`
  verb.

## Reading a build's capabilities

Any build reports its own capabilities as JSON:

```
operant capabilities
```

This prints the `get_capabilities` result (`contracts/ipc.md` section 3),
computed from the same cfg flags the rest of the CLI keys off
(`cli/src/commands/capabilities.rs`). A default build reports
`real_uia=false`/`real_input=false`; a release build reports both `true`.

## The release gate

```
just check-release-artifact
```

This builds the release core (`just build-release-core`) and then runs
`release/scripts/check-release-artifact.mjs` against the actual binary. The gate:

1. runs `<binary> capabilities` and parses the reported blob,
2. FAILS (exit 1) if `real_uia` or `real_input` is not `true` (a mock artifact),
3. FAILS if the binary still accepts the dev-only `record-ipc` verb,
4. passes only for a real, shippable core.

The gate is intentionally NOT part of `just verify`. `verify` must stay on the
mock, offline, deterministic build (so `just golden` reproduces); the release
gate is a release-time check that requires the heavy real-feature build.

### The gate is proven both ways

`cli/tests/release_gate.rs` (run by `cargo test` / `just verify`) drives the gate
script with representative blobs and asserts:

- a real-feature blob (`real_uia`/`real_input` true) PASSES (exit 0),
- a mock blob (both false, the shape of
  `contracts/fixtures/ipc/handshake.json`'s captured mock result) FAILS (exit 1),
- a half-real blob (only one feature) also FAILS (the E4 both-or-neither rule).

So the enforcement is tested, not just asserted: a deliberately mock-built
artifact fails the release gate, and a real one passes.

## One-command release build

The full installer build (`release/REPRODUCIBLE.md`) builds the Tauri shell and
NSIS bundle. That shell embeds and supervises the core binary; the core it ships
MUST be a release-matrix build (real features, no dev features), which
`just check-release-artifact` verifies against the produced core binary before a
release is cut.
