# 2. The core runs as a supervised sidecar process, not an in-process library

Status: Accepted

Date: 2026-07-12

## Context

The installed Tauri app renders entirely on a MOCK bus
(`ui/src/bus/mockClient.ts`, `simulateDemoRun`) with no connection to the Rust
core. The core is real and, as of P0b, proven to automate a live Windows
desktop model-free (`docs/evidence/P0-live-engine.md`: real UIA perception, real
Win32 input, 5/5 model-free replays). The remaining gap is structural: the
webview and the core are two worlds, and the app ships a demo that no real
engine is wired behind. A second, unwired execution path existed and nobody
noticed, which is the whole reason this campaign exists.

We must connect the webview to the real core. There are two shapes for that
connection:

1. **In-process linking.** Compile the core crates into the Tauri shell binary
   (`ui/src-tauri`), store an `Arc<Bus>` in Tauri managed state, and expose the
   command surface as `#[tauri::command]` functions that call core APIs
   directly. This is what the earlier design spec (`docs/specs/ipc-bridge.md`)
   assumed: one process, a pump thread bridging the in-process `Bus` to
   `app.emit`, and a thin TS client over `invoke` + `listen`.

2. **Supervised sidecar.** The core runs as its own child process. The shell
   spawns and supervises it, and the webview's `BusClient` becomes a real
   transport over that process rather than a call into linked-in code.

The earlier spec's UI-contract, storage, and command-mapping sections remain
valid and are carried into `contracts/ipc.md`. Its TRANSPORT section (the
in-process pump) is what this ADR supersedes.

Two facts about the system make this decision load-bearing rather than a matter
of taste:

- The core already owns a process-supervision pattern. C1 shipped
  `operant_core::supervisor::Supervisor<C: Child>`
  (`crates/core/src/supervisor.rs`): start, health check, restart-with-budget,
  crash accounting, and `sidecar.started` / `sidecar.health` / `sidecar.crashed`
  / `sidecar.restarted` bus events (`contracts/bus_events.md`). Sidecars are how
  this system already runs vision and voice. A supervised core is the same
  pattern turned on the core itself.
- The kill switch is never-cut safety. P0a added a process-global atomic input
  freeze (`operant_core::safety::set_frozen`) that the Windows synthesizer
  checks before every `SendInput` / `SetCursorPos`. That freeze is in force the
  instant it is set, but it lives INSIDE the core process. If the core process
  itself hangs (a pathological loop, a wedged UIA call, a deadlock), an
  in-process freeze flag cannot be observed, because the thread that would
  observe it is the wedged one.

## Decision

**The core runs as a supervised sidecar process. The shell (`ui/src-tauri`)
spawns and supervises it using the existing C1 supervisor pattern. The
webview's `BusClient` becomes a real transport over that child process
(`contracts/ipc.md`). We do NOT link the core in-process.**

The transport is newline-delimited JSON over the child's stdio. The framing,
the command set, the event stream, the capability handshake, backpressure,
reconnect, and shutdown are frozen in `contracts/ipc.md`. This ADR records WHY
the boundary is a process boundary; that contract records WHAT crosses it.

### Why a process boundary

1. **Reuse the C1 supervisor.** Supervising the core is not new code to design;
   it is `Supervisor<Child>` pointed at the core binary. Start, health,
   restart-with-budget, and the `sidecar.*` event vocabulary already exist and
   are tested. The shell gets crash detection and bounded restart for free.

2. **A second, unblockable kill path.** The kill switch now has two independent
   paths, and they fail independently:
   - Path 1 (in-process, fast): the P0a atomic freeze
     (`operant_core::safety::set_frozen(true)`), which blocks all real input
     synthesis in under 100 ms while the core is healthy.
   - Path 2 (out-of-process, unblockable): the shell hard-terminates the child.
     A hung core cannot ignore process termination the way it could ignore a
     cooperative in-process flag. A wedged thread cannot refuse `TerminateProcess`.
   In-process linking would collapse both paths into one process; a wedged core
   would take the kill switch, the supervisor, and the UI event loop down with
   it. The process boundary is what makes "stop" mean stop even when the core is
   the thing that is broken.

3. **One execution path for the app and the CLI.** The CLI already drives the
   real engine end to end (`operant explore` / `compile` / `run`,
   `docs/evidence/P0-live-engine.md`). Running the core as a child means the app
   drives that SAME binary and the SAME command surface, rather than a parallel
   in-process wiring that can drift. Collapsing the app path and the CLI path
   into one is the point: it is the fix for the exact failure that started this
   campaign, where a second unwired path existed and nobody noticed. Two paths
   rot; one path is exercised by everyone.

## Alternatives considered

### In-process linking (rejected)

Link the core crates into the shell binary and expose commands as
`#[tauri::command]`s over an `Arc<Bus>` in managed state (the original
`docs/specs/ipc-bridge.md` transport).

Rejected because:

- **It weakens the kill switch.** The only stop path would be the in-process
  freeze flag. A hung core is exactly the case where an in-process flag cannot
  be observed, and it is also the case where a user most needs the stop button
  to work. Safety that fails in the failure case is not safety.
- **It forks the execution path.** The app would drive core APIs through a
  Tauri-specific wiring while the CLI drives them through `main.rs`. Two call
  sites into the same engine is how the original unwired-path defect happened.
  We would be rebuilding the conditions we are here to remove.
- **It couples lifecycles and toolchains.** The core would rebuild and reship
  with the shell; a core panic would abort the webview process; the core's
  Windows/UIA dependencies would have to link cleanly into the Tauri binary on
  every build. `ui/src-tauri` is already its own Cargo workspace, deliberately
  not built by the repo-root `just ci` (ADR 0001). A process boundary keeps that
  separation instead of erasing it.

The in-process atomic freeze from P0a is NOT discarded by this decision. It is
retained as kill-switch path 1 (the fast path). The sidecar boundary ADDS path 2
(the unblockable path). The two are complementary, not alternatives.

### Local socket instead of stdio (deferred to the contract)

Whether the sidecar transport is stdio or a local TCP/named-pipe socket is a
transport-level choice, not an architecture choice, and is settled in
`contracts/ipc.md` (stdio, newline-delimited JSON). It does not change this
ADR: the boundary is a process either way.

## Consequences

- **The shell owns core lifecycle.** `ui/src-tauri` spawns the core child at
  startup, drives it with `Supervisor`, surfaces `sidecar.*` health to the UI,
  and hard-terminates it as kill-switch path 2. Crash-and-restart is a supported
  state, so the UI must re-handshake and re-query durable state after a restart
  (`contracts/ipc.md`, reconnect + resume).
- **The transport is a real surface with a real contract.** The webview no
  longer calls Rust directly; it frames requests and parses an event stream.
  `contracts/ipc.md` is frozen precisely because all 15 Phase 2 lanes build
  against it.
- **Determinism is unaffected.** The transport carries the same bus envelope
  (`crates/ir/src/bus.rs`) unchanged. Replay stays model-free and offline: the
  sidecar boundary is orthogonal to `operant-replay`'s backend-free crate graph.
  `just golden` and the airgap gate are untouched.
- **The release build must never enable dev-only features.** The P0b proof used
  `--features real-uia,real-input,dev-agent-bridge`. `dev-agent-bridge` is a
  filesystem planner rendezvous for a human-as-brain teach session; it is a
  development harness, never a product surface. The shipped core sidecar must be
  built without `dev-agent-bridge` (and without the new `dev-ipc-record`
  recorder feature). The capability handshake in `contracts/ipc.md` is the
  runtime backstop: a core that reports it cannot really automate forces the
  shell into a blocking screen, so even a mis-built sidecar cannot masquerade as
  a product.
- **Startup adds a spawn and a handshake.** The shell cannot render real-work UI
  until the child is up and `get_capabilities` has returned. This is a feature,
  not a cost: it is the structural point at which a demo-only core is caught and
  named.

## References

- `docs/evidence/P0-live-engine.md` (P0b): the engine automates a live desktop
  model-free (5/5); the release build never enables `dev-agent-bridge`.
- `docs/specs/ipc-bridge.md`: the earlier design; its UI-contract, storage, and
  command-mapping sections remain valid, its in-process transport is superseded
  here.
- `contracts/ipc.md`: the frozen transport, envelope, command set, event stream,
  capability handshake, and lifecycle semantics that implement this decision.
- `crates/core/src/supervisor.rs` (C1): the supervision pattern reused to run
  the core as a child.
- `crates/core/src/safety.rs` (P0a): the in-process atomic freeze retained as
  kill-switch path 1.
