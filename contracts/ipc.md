# Contract: Shell to Core IPC (the bridge)

Status: FROZEN for Phase 2. This is the single contract every Phase 2 lane
builds against. It is append-friendly under the versioning rules at the end;
nothing already specified here is renamed or removed without a protocol-version
bump and an ADR.

The architecture decision behind this contract (the core runs as a supervised
sidecar child, not linked in-process) is recorded in
`docs/adr/0002-core-sidecar-ipc.md`. The UI-contract, storage, and
command-mapping detail in `docs/specs/ipc-bridge.md` remains valid; that spec's
in-process transport is superseded here.

Recorded example traffic (a real explore -> compile -> replay -> undo session,
framed exactly per this document) lives in `contracts/fixtures/ipc/`. Phase 2
lanes build and test against those fixtures without a live core.

## 0. Roles and shape

- The **shell** is `ui/src-tauri` plus the webview. It spawns the core child,
  supervises it (`operant_core::supervisor::Supervisor`, C1), and owns the
  webview `BusClient`.
- The **core** is the `operant` binary run as a child process
  (`--serve` mode, a Phase 2 addition to `cli/src/main.rs`). It is the same
  binary and the same engine the CLI drives, which is the point: the app and the
  CLI share one execution path.
- Traffic is a bidirectional stream of newline-delimited JSON frames over the
  child's stdio. The shell writes requests to the child's stdin; the core writes
  responses and events to the child's stdout. The child's stderr is human logs
  only and is never parsed as protocol.

## 1. Transport: newline-delimited JSON over the sidecar's stdio

We use newline-delimited JSON (one compact JSON object per line, `\n`
terminated, no embedded raw newlines) over the child process's stdin/stdout. We
choose stdio over a local socket because the lifecycle is the simplest correct
one: the pipe is created with the child and closed by the OS when either side
exits, so liveness and teardown reduce to "is the child alive," which the C1
supervisor already answers, with no port to bind, no socket file to leak or
collide, and no localhost listener for another process on the machine to probe.
It also reuses the exact channel every other core subprocess already speaks
(the agent-bridge and sidecars are stdio processes), so there is one framing to
maintain. The cost, no in-process reconnect to a surviving core, is not a real
cost here: a dead pipe means a dead process, and recovery is a supervisor
restart plus a re-handshake (section 8), which we need regardless.

Framing rules:

- One frame is one line. Encoders MUST emit compact JSON with no literal newline
  inside the object and MUST terminate each frame with a single `\n`.
- UTF-8 only. A leading UTF-8 BOM on a line is tolerated and stripped (Windows
  PowerShell producers emit one).
- A line that does not parse as a JSON object is a protocol error: the reader
  replies (if it can correlate) with error code `bad_request`, logs the raw line
  to stderr, and continues at the next newline. A malformed line never wedges
  the stream.
- Max frame size is 8 MiB. A producer that would exceed it MUST instead send a
  reference (for example a recorder blob id), never split one logical frame
  across lines. This bounds the reader's line buffer.

## 2. Framing: the four frame types

Every frame is a JSON object with a string tag `t` and an integer protocol
version `pv` (currently `1`). `pv` is the IPC protocol version and is distinct
from the bus envelope's own `v` (`contracts/bus_events.md`); the two version
independently.

### 2a. `ready` (core to shell, unsolicited)

The first frame the core writes on startup, before any request arrives. It tells
the shell the pipe is up and which protocol version the core speaks.

```json
{"t":"ready","pv":1}
```

### 2b. `req` (shell to core, request)

```json
{"t":"req","pv":1,"id":"3f1c...","cmd":"start_explore","args":{"goal":"...","window_process":"notepad.exe"}}
```

- `id`: a client-generated correlation id, unique per outstanding request, an
  opaque string (a UUIDv4 is recommended). The core echoes it verbatim in the
  one `res` that answers this `req`.
- `cmd`: a command name from section 5.
- `args`: the command's argument object (section 5). Omitted or `{}` when the
  command takes none.

### 2c. `res` (core to shell, response)

Exactly one `res` answers each `req`, carrying the same `id`.

Success:

```json
{"t":"res","pv":1,"id":"3f1c...","ok":true,"result":{ }}
```

Failure:

```json
{"t":"res","pv":1,"id":"3f1c...","ok":false,"error":{"code":"invalid_args","message":"window_process is required","retryable":false}}
```

The error shape is fixed:

- `code`: a stable snake_case string from the catalog below.
- `message`: a plain-language sentence, safe to surface to a user.
- `retryable`: `true` when the same request may succeed later unchanged (for
  example `core_busy`), `false` when it never will (for example
  `unknown_command`).

Error code catalog (append-only):

| code | meaning | retryable |
|---|---|---|
| `bad_request` | frame malformed or not a JSON object | false |
| `unsupported_protocol` | `pv` the receiver does not speak | false |
| `unknown_command` | `cmd` not in the section 5 set | false |
| `not_implemented` | `cmd` is reserved in this contract but not wired in this build | false |
| `invalid_args` | `args` failed validation | false |
| `not_found` | a referenced run/workflow/trigger id does not exist | false |
| `conflict` | the command cannot apply in the current state (for example a run is already active) | false |
| `core_busy` | a run is active and this command needs an idle core | true |
| `refused` | a safety or gate refusal (a typed, expected "no") | false |
| `internal` | an unexpected core-side error | true |

### 2d. `evt` (core to shell, asynchronous event)

An event is NOT correlated to any request. It carries one bus envelope,
byte-identical to `contracts/bus_events.md` and `crates/ir/src/bus.rs`, under
`env`.

```json
{"t":"evt","pv":1,"env":{"v":1,"seq":42,"ts":"2026-07-12T12:00:00.000Z","topic":"run.step.executed","payload":{ }},"thumb":null}
```

- `env`: the bus `Envelope` unchanged. The shell's existing `BusEnvelope`
  handling (`ui/src/bus/types.ts`) consumes it with no translation.
- `thumb`: the flight-recorder screenshot sidecar (section 7). Present only on
  run-step events; `null` on every other event and on headless cores.

## 3. The capability handshake (load-bearing)

The first request the shell sends, immediately after it reads `ready`, is
`get_capabilities`. The shell renders no real-work UI until that `res` arrives.

Response `result`:

```json
{
  "real_uia": false,
  "real_input": false,
  "real_vision": false,
  "mock_planner_only": true,
  "transport_kind": "stdio",
  "version": "1.0.0",
  "git_sha": "unknown"
}
```

| field | type | meaning |
|---|---|---|
| `real_uia` | bool | the core has a live UIA perceiver (built with `real-uia`). `false` = fixture/mock perception. |
| `real_input` | bool | the core drives real Windows input (built with `real-input`). `false` = the deterministic mock synthesizer. |
| `real_vision` | bool | a vision grounder sidecar is available for pixel grounding. `false` = no pixel fallback. |
| `mock_planner_only` | bool | `true` when the only planner is a scripted/mock backend (no configured model, no dev bridge). A core that can teach with a real model reports `false`. |
| `transport_kind` | string | the transport this core speaks; `"stdio"` for this contract. |
| `version` | string | the core build version (`operant --version`). |
| `git_sha` | string | the build's commit, or `"unknown"` if not stamped. |

### The structural guarantee

This is the mechanism that a demo can never ship as a product. The shell MUST
gate real-work UI on these booleans:

- **Real automation requires `real_uia && real_input`.** This mirrors the CLI's
  E4 rule (`cli/src/commands/run.rs`: a real run needs BOTH features; either
  alone silently degrades to mock). If the core reports either as `false`, the
  shell MUST NOT expose any surface that starts a real run or a real teach. It
  MUST instead show a BLOCKING screen that names, by contract field, exactly
  what is missing, for example: "This core cannot automate your computer. It was
  built without real desktop input (real_input=false) and without live
  perception (real_uia=false). It can show you the interface but cannot act on
  your machine. Rebuild the core with real-uia and real-input, or reinstall a
  release build." The blocking screen enumerates each `false` capability by its
  field name so the failure is legible, not a generic error.
- **Teaching requires a real planner.** When `mock_planner_only` is `true`, the
  teach surface MUST NOT present itself as producing a real taught workflow. A
  mock planner replays a scripted trajectory and ignores the goal. The shell
  either hides the teach entry point or labels it unmistakably as a demo and
  routes the user to backend configuration (`configure_backend`). Replay and
  real-run surfaces are NOT blocked by `mock_planner_only` alone, because replay
  needs no planner.
- **`real_vision`** gates only vision-dependent affordances (pixel grounding
  fallbacks). Its absence curtails those, it does not block the app.

The four automation booleans follow build cfg and are constant for a process
lifetime, so the shell gates on the handshake once per core process. A vision
sidecar going down mid-session is surfaced through `sidecar.*` events, not a
capability change. A core restart re-runs the handshake (section 8).

The committed fixture `contracts/fixtures/ipc/handshake.json` is a real capture
from a default (mock) recorder build, so it shows the BLOCKING case
(`real_uia`/`real_input` both `false`). A real-capable core reports the same
shape with the booleans `true`; Phase 2 tests of the normal render path
construct that variant from this documented shape.

## 4. Command envelope conventions

- Every command is one `req` and exactly one `res`. Long-running work (a run)
  returns quickly (for example with the `run_id`) and reports progress through
  the `evt` stream, never by holding the `res` open.
- Where a command's real effect is an event (for example `pause` publishes
  `run.control.pause`), the `res` confirms the command was accepted; the
  observable outcome arrives as an `evt`. This matches the existing
  self-publishing-screen inversion in `docs/specs/ipc-bridge.md` section 8b.
- `run_id` for a started run is delivered by the `run.started` event, not the
  command result, because the UI already reacts to `run.started`
  (`docs/specs/ipc-bridge.md` section 9). `start_explore`/`start_replay` return
  the `run_id` in the result too, for convenience, but the event is canonical.

## 5. The command set

Every command the shell may send. For each: its args, its result, and the real
core API it maps to (crate and function), or a NOT-YET-IMPLEMENTED marker so
Phase 2 knows the wiring is theirs to add. "Implemented" means the underlying
core API exists today; the thin `req`/`res` command wrapper in the `--serve`
loop is Phase 2 work in every case.

### 5a. Lifecycle and capabilities

| Command | Args | Result | Maps to | Status |
|---|---|---|---|---|
| `get_capabilities` | none | capability object (section 3) | reads build cfg (`real-uia`/`real-input`) + sidecar availability; `operant --version` | NOT-YET-IMPLEMENTED (new command; no core entrypoint yet). The recorder demonstrates the exact shape. |
| `configure_backend` | `{provider, model, api_key?, endpoint?}` | `{ok:true}`; echoes `config.changed` | `operant_core::Config::set` (`crates/core/src/config.rs:70`), dotted keys per `docs/specs/ipc-bridge.md` section 7 | Implemented (via `Config::set`). Key naming + validation is Phase 2. |
| `probe_backend` | `{provider, model, endpoint?}` | `{reachable:bool, detail:string}` | compose `operant_doctor::ModelReachableCheck::tcp` (`crates/doctor/src/checks.rs:67`) + `operant_orchestrator::backends::live_config` validation | NOT-YET-IMPLEMENTED (no single `probe_backend` entrypoint today). |

### 5b. Run control

| Command | Args | Result | Maps to | Status |
|---|---|---|---|---|
| `list_windows` | `{}` | `{windows: [{process, title, id}]}`, z-ordered topmost first, Operant excluded | `operant_perception_uia::enumerate_windows` (`crates/perception-uia/src/uia/window.rs`) | Implemented (ADR 0003). Additive per section 9 rule 2. Without `real-uia` returns `{windows:[]}`. |
| `start_explore` | `{goal, window_process, title_pattern?}` | `{run_id}` (canonical via `run.started`) | `operant_orchestrator::explore::ExploreLoop::run` (`crates/orchestrator/src/explore/mod.rs:144`), assembled by cfg as in `cli/src/commands/explore.rs` | Implemented (the loop). `window_process` is the picked target (ADR 0003). `title_pattern` is optional, reserved for same-process disambiguation, not yet consulted. |
| `start_replay` | `{path, inputs?}` | `{run_id, steps_executed, pre, post}` | `operant_replay::Replayer::replay_compiled` (`crates/replay/src/lib.rs:163`), wrapped in synthetic `run.*` events per `docs/specs/ipc-bridge.md` section 3b | Implemented (the replayer). The run.* wrapper is Phase 2 and MUST NOT pull `operant-orchestrator` into the replay path. |
| `dry_run` | `{path, inputs?}` | `{run_id, steps_executed, pre, post}` | `operant_replay::Replayer::with_mock` (mode `dry`, no real synth); see `crates/safety/src/dryrun.rs` | Implemented. |
| `pause` | `{run_id?}` | `{ok:true}`; observable via `run.paused` | `bus.publish("run.control.pause", {})` (`crates/orchestrator/src/explore/control.rs`) | Implemented (BusControl honors it). |
| `redirect` | `{instruction}` | `{ok:true}`; observable via `run.redirected` | `bus.publish("run.control.redirect", {instruction})` | Implemented. |
| `resume` | `{run_id?}` | `{ok:true}`; observable via `run.resumed` | `bus.publish("run.control.resume", {})` | Implemented. |
| `stop` | `{run_id?}` | `{ok:true}`; ends with `run.completed`/`run.halted` | `operant_core::safety::set_frozen(true)` (path 1) + `run.control.pause`, then `Recorder::end_run` and a closing event | Implemented (pieces). The cooperative stop-then-close orchestration (freeze, ~1s cooperative pause, ensure the run row closes) is Phase 2. |
| `kill` | none | `{ok:true}`; echoes `killswitch.engaged` | `operant_core::safety::set_frozen(true)` (path 1, `crates/core/src/safety.rs`) AND the shell hard-terminates the child (path 2, `docs/adr/0002`) | Implemented (path 1). Path 2 is a shell responsibility over the supervised child. |

`kill` is the panic path. Its `res` is best-effort: the shell MUST NOT depend on
receiving it, because path 2 (hard-terminating the child) may cut the pipe
before the `res` is written. The freeze (path 1) has already stopped input
synthesis in-process; the terminate guarantees stop even if the core is wedged.

### 5c. Workflows and runs

| Command | Args | Result | Maps to | Status |
|---|---|---|---|---|
| `list_workflows` | none | `[{id,name,version,...}]` | `operant_recorder::Recorder::list_workflows` (`crates/recorder/src/backup.rs:28`) + registry `FsStore` | Implemented (recorder side). |
| `get_workflow` | `{id}` | workflow manifest DTO | `operant_recorder::Recorder::get_workflow` (`crates/recorder/src/misc.rs:41`) | Implemented. |
| `explain_workflow` | `{path}` | `{title, summary, grant, inputs, steps}` | `@operant/sdk/render` via node (`cli/src/commands/explain.rs`) | Implemented (via the SDK renderer; no Rust reimplementation). |
| `delete_workflow` | `{id}` | `{ok:true}` | `Recorder::delete_workflow()` and `InstallStore::delete()` | NOT-YET-IMPLEMENTED (neither exists; `docs/specs/ipc-bridge.md` section 5 flagged both to ADD). |
| `compile_run` | `{run_id}` | workflow manifest DTO; echoes `workflow.compiled` | `operant_compiler::compile` / `compile_records` (`crates/compiler/src/lib.rs:60,82`) over the recorded trajectory | Implemented (the compiler). The read-trajectory-from-run_id glue is Phase 2. |
| `list_runs` | none | `[run_id]` | `operant_recorder::Recorder::list_runs` (`crates/recorder/src/runs.rs:160`) | Implemented. |
| `get_run` | `{run_id}` | `{run, steps}` | `Recorder::get_run` + `Recorder::list_steps` (`crates/recorder/src/runs.rs:120`, `steps.rs:203`) | Implemented. |
| `preview_undo` | `{run_id}` | none in result; delivered via `undo.previewed` items | `operant_recorder::Recorder::publish_undo_preview` (`crates/recorder/src/undo.rs:308`) / `preview_undo_event` (`undo.rs:282`) | Implemented. |
| `undo_run` | `{run_id}` | `{restored}`; echoes `undo.applied` | `operant_recorder::Recorder::undo_run` (`crates/recorder/src/undo.rs:247`) | Implemented. |

### 5d. Registry

| Command | Args | Result | Maps to | Status |
|---|---|---|---|---|
| `install_workflow` | `{manifest_json, dsl_bytes, publisher_key?, approval}` | installed workflow DTO; echoes `workflow.installed` | `operant_registry::install` (`crates/registry/src/install.rs:327`) into an `InstallStore` (`FsStore`) | Implemented. |
| `publish_workflow` | `{draft_manifest_path, dsl_path, registry_dir, key_path, branch?}` | `{branch, commit}` | `operant_registry::{sign_manifest, dsl_hash}` + `git` (`cli/src/commands/publish.rs`) | Implemented (via the CLI verb; shells to `git`). In-app publishing may be deferred to a later phase. |

### 5e. Scheduler and triggers

| Command | Args | Result | Maps to | Status |
|---|---|---|---|---|
| `list_triggers` | none | `[{trigger_id, kind, workflow_name, spec, enabled}]` | trigger types exist (`crates/scheduler/src/trigger.rs`); no persistent trigger store | NOT-YET-IMPLEMENTED (no CRUD/persistence layer; `docs/specs/ipc-bridge.md` notes "needs a new command+topic"). |
| `upsert_trigger` | `{trigger_id?, kind, workflow_name, spec, enabled}` | `{trigger_id}` | same; would enqueue via `operant_scheduler::enqueue` (`crates/scheduler/src/lib.rs:53`) when fired | NOT-YET-IMPLEMENTED (no store to upsert into). |

### 5f. Diagnostics, metrics, settings, buffer, backup

| Command | Args | Result | Maps to | Status |
|---|---|---|---|---|
| `get_metrics` | `{weeks?}` | `[{week, minutes_saved_total, ...}]` | `operant_recorder::Recorder::get_weekly_system_metrics` / `get_weekly_workflow_metrics` (`crates/recorder/src/metrics.rs:80,54`), `estimate_minutes_saved` | Implemented. |
| `run_doctor` | `{drive?, min_disk_gb?}` | `{findings, exit_code}`; each finding also on `doctor.finding` | `operant_doctor::run_doctor_verb` (`crates/doctor/src/lib.rs:49`, publishes to the bus) | Implemented. |
| `get_settings` | none | config snapshot (dotted keys to values) | `operant_core::Config::snapshot` (`crates/core/src/config.rs:87`) | Implemented. |
| `set_settings` | `{key, value}` | `{ok:true}`; echoes `config.changed` | `operant_core::Config::set` (`crates/core/src/config.rs:70`) | Implemented. |
| `purge_observation_buffer` | none | `{purged:true, total_writes}` | `operant_orchestrator::watch::EventSink::purge` / `CappedBuffer::purge` (`crates/orchestrator/src/watch/buffer.rs:47`) | Implemented. Purge clears stored events but never resets `total_writes` (the "provably never written" signal). |
| `export_backup` | `{}` | `{bytes_b64}` (a portable archive) | `operant_recorder::backup::export` (`crates/recorder/src/backup.rs:143`) | Implemented. |
| `import_backup` | `{bytes_b64}` | `{imported}` summary | `operant_recorder::backup::import` (`crates/recorder/src/backup.rs:165`) | Implemented. |

### 5g. NOT-YET-IMPLEMENTED summary (for Phase 2 planning)

Five commands are reserved in this contract but have no core wiring yet. A build
that has not wired them MUST answer them with error code `not_implemented`:

1. `get_capabilities` (new command; the recorder demonstrates the exact result
   shape and the shell's blocking behavior depends on it, so this is the
   highest-priority wiring).
2. `probe_backend` (no single entrypoint; compose from doctor + backend config).
3. `delete_workflow` (needs `Recorder::delete_workflow()` and
   `InstallStore::delete()`).
4. `list_triggers` (needs a persistent trigger store).
5. `upsert_trigger` (needs the same store).

## 6. The event stream

Every bus event documented in `contracts/bus_events.md` flows from core to shell
UNCHANGED, wrapped in an `evt` frame (section 2d). The core subscribes on the
in-process bus to every top-level family and forwards each envelope:

```
run.*  gate.*  approval.*  perception.*  sidecar.*  vram.*  workflow.*
trigger.fired  schedule.*  killswitch.*  undo.*  doctor.*  metrics.*
suggestion.*  config.changed  voice.*
```

(The `run.control.*` topics are shell-to-core commands, not events, and are NOT
forwarded back as `evt` frames; they are the `pause`/`resume`/`redirect`
commands' underlying publications.)

The envelope `payload` is byte-identical to the catalog in
`contracts/bus_events.md`. The IPC layer adds NOTHING to the bus payload. The
one augmentation the flight recorder needs (a screenshot thumbnail) rides the
`evt` frame beside the envelope, not inside it (section 7), precisely so the bus
event contract stays unchanged.

One new bus topic is appended by this contract for backpressure signalling:

| Topic | Payload (required fields) | Notes |
|---|---|---|
| bus.overflow | dropped (count), resume_seq | emitted by the IPC writer after it dropped event frames under backpressure (section 8a); lets the shell detect and reconcile a gap. Never dropped itself. |

`bus.overflow` is added per the append-only rule in `contracts/bus_events.md`
(new topics may be added freely; consumers must not crash on unknown topics).

## 7. Flight-recorder thumbnails on run-step events

Run-step `evt` frames carry an OPTIONAL redacted screenshot thumbnail for the
flight recorder. It rides the `evt` frame as a sibling of `env`, NOT inside the
bus payload, so the bus event stays byte-identical to `contracts/bus_events.md`.

- `thumb` is present only on `evt` frames whose `env.topic` is
  `run.step.executed` (the executed-step frame the flight recorder renders) and,
  when the producer has one, `run.step.proposed`. On every other event and on a
  headless/mock core, `thumb` is `null`.
- Shape:

```json
{
  "run_id": "run_...",
  "step_id": "s2",
  "format": "png",
  "w": 320,
  "h": 200,
  "redacted": true,
  "data_b64": "<base64 png bytes>"
}
```

- **Redaction is required and fail-closed.** The producer runs the capture
  through `operant_recorder::redact::redact` (`crates/recorder/src/redact.rs`,
  C20/FR-S7) BEFORE downscaling, so every element flagged sensitive is blacked
  out. `redacted` MUST be `true`. If redaction cannot produce a clean frame
  (`RedactError`), the producer emits NO thumbnail (`thumb` is `null`); it never
  emits raw or half-redacted pixels. This mirrors the recorder's own
  decode -> redact -> encode -> store discipline: a correctly wired producer
  structurally cannot ship unredacted pixels.
- `w`/`h` are the downscaled thumbnail dimensions (bounded; a thumbnail, not a
  full frame). `data_b64` is the base64 of the encoded PNG of the redacted,
  downscaled image.
- Headless cores (mock perception, no pixels) produce no thumbnail. The
  fixtures show `thumb: null` throughout for this reason, which is the honest
  output of a mock recorder build.

## 8. Backpressure, reconnect and resume, shutdown

### 8a. Backpressure

The core NEVER blocks its automation loop on the shell. The loop publishes to the
in-process bus; a dedicated IPC writer thread drains the bus subscriptions into
stdout. Between the bus and the pipe writer sits a bounded queue.

- `req`/`res` frames are the control plane. They are small, correlated, and MUST
  NOT be dropped. They are always enqueued.
- `evt` frames are lossy under sustained pressure. If the shell stops draining
  stdout and the bounded queue fills, the writer drops the OLDEST `evt` frames,
  counts them, and once it can write again emits a single `bus.overflow`
  event `{dropped, resume_seq}`. The monotonic `seq` on every envelope
  (`crates/ir/src/bus.rs`) lets the shell detect the gap independently of the
  marker.
- The shell MUST drain stdout continuously on its own thread and MUST NOT do
  slow work inline with reading. Durable truth is the recorder database; a shell
  that missed events re-queries (`get_run`, `list_runs`) rather than expecting
  the core to replay them.
- The kill switch does not depend on the event queue draining. Path 1 (the
  in-process freeze) is set by the core independent of IPC, and path 2 (hard
  terminate) is the shell acting on the child, not a frame in the queue. A
  backed-up event stream can never delay a stop.

### 8b. Reconnect and resume

stdio has no in-place reconnect: the pipe dies with the process. Recovery is a
supervisor restart plus a re-handshake.

- On a core crash the supervisor observes the child exit and emits
  `sidecar.crashed`, then restarts within its budget
  (`operant_core::supervisor::Supervisor`).
- After a restart the shell (1) waits for `ready`, (2) re-sends
  `get_capabilities`, and (3) re-queries durable state (`list_runs`, `get_run`,
  `list_workflows`, `get_settings`) to rebuild its view.
- `seq` is per-process and resets to `0` on restart. A `ready` frame followed by
  a `seq` lower than the last one the shell saw is the signal that the core
  restarted; the shell discards its in-memory event view and rebuilds from
  queries. Event history is NOT replayed across a restart; the recorder database
  is the durable record.
- **Orphan reconciliation.** A run that was `running` when the core died is
  reconciled on restart: the core marks any still-`running` run row as halted
  (`run.halted`, reason `error`) at startup, so no run row is ever left open.
  The shell sees the halted state via `get_run`.
- **Resume is not reconnect.** Resuming a cooperatively PAUSED run is the normal
  `resume` command against a live core. Reconnect rebuilds the shell's view
  after a core restart. They are unrelated.

### 8c. Shutdown

- **Graceful.** The shell sends `stop` to close any active run, then closes the
  child's stdin. The core treats stdin EOF as "shell gone, exit": it ends any
  active run (`Recorder::end_run` plus a closing `run.completed`/`run.halted`),
  flushes pending `res`/`evt` frames, and exits `0`. A run row is never left
  open.
- **EOF is authoritative.** The core MUST exit on stdin EOF even with no `stop`
  or `shutdown` command, so a crashed shell cannot orphan a running core (the
  process dies with its pipe).
- **Hard.** The shell hard-terminates the child (kill-switch path 2, or an
  unresponsive core). There is no graceful drain; the freeze (path 1) has
  already stopped input synthesis. The supervisor observes the exit and emits
  `sidecar.crashed`; the shell may restart via 8b.

## 9. Versioning rules

1. `pv` (the IPC protocol version) bumps only on a breaking framing change (a
   frame type, correlation, or error-shape change). It is independent of the bus
   envelope `v`.
2. Additive change is free and does NOT bump `pv`: new commands, new OPTIONAL
   `args` fields, new OPTIONAL `result` fields, new error codes, new bus topics
   forwarded as `evt`. Consumers MUST ignore unknown fields and MUST NOT crash on
   an unknown command result field, error code, or event topic.
3. A NOT-YET-IMPLEMENTED command becoming implemented is additive (it stops
   answering `not_implemented`); it does not bump `pv`.
4. A breaking change needs an ADR, a `pv` bump, and fixtures at both versions
   (mirroring the bus-events rule in `contracts/bus_events.md`).
5. This contract is append-friendly, but Phase 2 depends on every clause above,
   so it is complete as of this freeze: the full command set, the handshake, the
   event stream, the thumbnail channel, and the lifecycle semantics are all
   specified now, not deferred.
