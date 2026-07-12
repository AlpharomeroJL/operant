# IPC fixtures: a recorded shell to core session

These fixtures are a REAL, recorded `operant` session, framed exactly per
`contracts/ipc.md` (the frozen shell-to-core IPC contract). Phase 2 lanes build
and test their transport, their `BusClient`, and their command handlers against
these files without a live core.

They are NOT hand-written. They are captured by the dev-only recorder
`operant record-ipc` (`cli/src/commands/record_ipc.rs`, built behind the
`dev-ipc-record` cargo feature), which drives a real explore -> compile ->
replay -> undo session against the real `operant_core::Bus` and writes the frames
here.

## Files

| File | What it is |
|---|---|
| `handshake.json` | The `get_capabilities` handshake extract: the `ready` frame, the `get_capabilities` request, and the capability response. This capture is from a default (mock) recorder build, so `real_uia`/`real_input` are `false`: the BLOCKING case that must force the shell's blocking screen (`contracts/ipc.md` section 3). |
| `session-explore-compile-replay-undo.jsonl` | The full session as newline-delimited frames (one JSON object per line): `ready`, the handshake, then the `req`/`res` pairs and the real bus `evt` stream for `start_explore`, `compile_run`, `start_replay`, `preview_undo`, and `undo_run`. |

## How they were produced

From the worktree, with the isolated target dir:

```powershell
$env:CARGO_TARGET_DIR = 'D:\dev\operant-target-p1'
cargo run -p operant-cli --features dev-ipc-record -- record-ipc
```

That is the entire reproduction. The recorder writes its throwaway artifacts
(the recorder SQLite database and the compiled workflow) under
`./out/record-ipc/`, and the fixtures into `contracts/fixtures/ipc/` (override
with `--out` and `--fixtures`).

## What is real, and what is synthesized

- **Explore** (`run.*` events for the explore run): 100 percent real. The
  recorder runs the real `operant_orchestrator::explore::ExploreLoop`, which
  publishes real, typed `run.started` / `run.step.*` / `run.completed` events to
  the real bus. The planner and perceiver are mock (the scripted mock planner
  and the bundled Notepad snapshot), which is exactly how the default
  `operant explore` runs headless. The event STRUCTS are the same real types a
  live core emits; only the perception source and synthesizer are mock.
- **Compile**: real. The trajectory the explore run recorded is compiled by the
  real `operant_compiler::compile`. The recorder then publishes `workflow.compiled`
  as the `compile_run` command is contracted to (`contracts/ipc.md` section 5c).
- **Replay** (`run.*` events for the replay run): the replay itself is the real
  `operant_replay::Replayer`. The `Replayer` publishes no events (by design, so
  the replay crate stays backend-free), so the recorder wraps it in the
  synthetic `run.started` / `run.step.gated` / `run.step.executed` /
  `run.completed` envelope that the `start_replay` command is contracted to
  publish (`contracts/ipc.md` section 5b, `docs/specs/ipc-bridge.md` section 3b).
  These are the real bus event structs, published to the real bus, from the real
  compiled workflow's actions.
- **Undo** (`undo.previewed`, `undo.applied`): real events from the real
  recorder undo journal. Because the headless mock synthesizer performs no real
  OS writes, the run itself journaled nothing, so the recorder seeds the journal
  through the recorder's real `journal_ahead` API (a `CreateFile` inverse on a
  relative path that never exists on disk, so the later undo is a guarded no-op,
  plus one `Irreversible` entry). `preview_undo` then emits a real, populated
  `undo.previewed` exercising the F1b `items[]` wire shape.

## The `thumb` field

Every `evt` frame carries `"thumb": null`. This is a headless/mock recorder with
no pixels, so there is no screenshot to redact and downscale
(`contracts/ipc.md` section 7). The field is present, and null, to document its
place on the frame. A real core populates it (redacted, downscaled) on
`run.step.executed` frames only.

## Determinism normalization

The recorder normalizes exactly the volatile fields so the committed fixture is
byte-stable across regenerations and reviewable in a diff:

- recorder-generated ids (`run_<hex>_<hex>`, `step_<hex>_<hex>`, per
  `crates/recorder/src/ids.rs`) are replaced with stable tokens (`run_0`,
  `run_1`, `step_0`, ...), consistently everywhere they appear;
- `ms` and `wall_ms` timings are zeroed.

Everything else is the raw capture. The envelope `ts` is the deterministic
monotonic placeholder the current bus emits (`seq:<n>`, `crates/core/src/bus.rs`);
the real bridge stamps ISO-8601 UTC at publish (`docs/specs/ipc-bridge.md`
section 1), and the framing is identical either way.

## Validation

`node scripts/check_json.mjs` (part of `just ci` / `just verify`) validates every
`.json` here parses and every line of every `.jsonl` here parses. The recorder is
also the regenerator: re-run the command above to refresh these files after a
contract change.
