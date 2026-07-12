# Operant IPC bridge + real-engine wiring (design spec)

Status: Binding for the "wire app+engine+ui for real" build. Distilled from five
read-only core surveys (bus/orchestrator, engine capability, storage, uninstaller,
UI contract). Every lane works from this doc. When code and this doc disagree,
this doc wins until amended. File:line anchors point at the surveyed tree
(`D:\dev\operant`, branch `redesign`).

## 0. What we are building and why

The installed Tauri app renders entirely on a MOCK bus (`ui/src/bus/mockClient.ts`,
`simulateDemoRun`) with NO connection to the Rust core. The core is real but
CLI-adjacent and, as surveyed, cannot yet automate a live desktop. We are:

1. Building a real Tauri IPC bridge so the webview drives the real core.
2. Fixing the replay engine so a compiled workflow actually runs on a live desktop.
3. Wiring **model-driven teach** (the existing `ExploreLoop`) end to end (user's
   choice: model-driven now, demonstration-recorder as a labeled fast-follow).
4. Making the kill switch actually stop a running loop (safety, never-cut).
5. Wiring undo, settings, storage; fixing stale docs. Then proving on the desktop.

Determinism is sacred: **replay must stay model-free and offline.** The replay
command depends on `operant-replay` ONLY, never `operant-orchestrator`/`reqwest`.
Teach legitimately uses a model (network), gated behind explicit config.

## 1. Transport

- Bus is `operant-core::Bus` (`crates/core/src/bus.rs`): SYNCHRONOUS crossbeam
  channels. `subscribe(pattern) -> Subscription{pattern, rx}`, `publish(topic,
  value) -> seq`, `publish_event::<E: BusEvent>`. Global monotonic `seq`
  (`AtomicU64`). `Bus: Send+Sync` -> store as `Arc<Bus>` in Tauri managed state.
- Envelope (`crates/ir/src/bus.rs:14-22`) `{v:1, seq, ts, topic, payload}` is
  byte-identical to the webview's `BusEnvelope` (`ui/src/bus/types.ts:49-55`) and
  the contract (`contracts/bus_events.md`). No translation layer.
- **No `"*"` catch-all in Rust** (`Envelope::matches` is exact or `prefix.*`
  only). The pump subscribes per top-level family (see 8a). The TS client keeps
  the mock's `"*"` support locally.
- **`ts` is a placeholder** (`format!("seq:{seq}")`, `bus.rs:60-64`), not ISO-8601.
  FIX in the bridge: stamp real ISO-8601 UTC at publish (the UI uses relative
  time). Change `Bus::publish` to emit an ISO timestamp; keep `seq` for ordering.

### Pump (core -> webview)
A dedicated `std::thread` (NOT a tokio task; crossbeam `recv()` blocks). Subscribe
once per family, `crossbeam_channel::Select` over the receivers, and for each
envelope `app.emit("operant://bus", &envelope)` (needs `tauri::Emitter`). Start it
in `.setup()`. Mirror `ui/src-tauri/src/updater.rs:152-167` for handle capture.

### TS client (webview)
`ui/src/bus/tauriClient.ts` `createTauriBusClient(): BusClient`, one
`listen("operant://bus", e => dispatch(e.payload))`, reuse the mock's exact
prefix-filter (`mockClient.ts:40-42`, incl. `"*"`). Drop-in for
`createMockBusClient()`. `publish(topic,payload)` routes command-topics to
`invoke` (table 2) and is a no-op for core-owned event topics (the core echoes the
real event back). `main.ts` selects the real client when `window.__TAURI__` /
`isTauri()` is present, else the mock (dev/demo). Keep the mock + `simulateDemoRun`
for `npm run dev` outside Tauri.

## 2. Commands (webview -> core), all `#[tauri::command]`

| Command | Args | Returns | Impl (file:line anchor) |
|---|---|---|---|
| `start_teach_run` | `goal:String, window_process:String` | `()` (run_id via `run.started`) | assemble `ExploreLoop` behind cfg; `tauri::async_runtime::spawn(loop.run(&bus,&rec,&goal,&mut BusControl::subscribe(&bus)))` (`explore/mod.rs:144`, `control.rs:66`) |
| `run_saved_workflow` | `path:String, inputs:Option<Map>` | `RunSummaryDto` | `load_compiled`+`Replayer::replay_compiled` (`replay/lib.rs:139`); **publish synthetic run.* around it** (3b) |
| `dry_run_workflow` | `path:String, inputs?` | `RunSummaryDto` | same as replay but mode `dry`, no real synth |
| `pause_run` | - | `()` | `bus.publish("run.control.pause", {})` (`control.rs:81`) |
| `resume_run` | - | `()` | `bus.publish("run.control.resume", {})` |
| `redirect_run` | `instruction:String` | `()` | `bus.publish("run.control.redirect", {instruction})` |
| `stop_run` | `run_id?` | `()` | engage the freeze flag (4) + `run.control.pause`; ensure `end_run`+`run.completed`/`run.halted` |
| `engage_killswitch` / `release_killswitch` | - / `run_id?` | `()` | set/clear global freeze (4); publish `killswitch.engaged/released` |
| `request_undo_preview` | `run_id:String` | `()` (result via `undo.previewed` items) | `Recorder::publish_undo_preview(&bus, run_id)` (`undo.rs:308`) |
| `apply_undo` | `run_id:String` | `()` (echo `undo.applied`) | `Recorder::undo_run` (`undo.rs:247`) then publish `undo.applied` |
| `list_workflows` | - | `Vec<WorkflowSummary>` | recorder workflows + registry FsStore; ADD `list_workflows()`/`list()` (see 5) |
| `get_workflow` | `id` | `WorkflowManifestDto` | `Recorder::get_workflow` (`misc.rs:41`) |
| `list_runs`/`list_steps` | - / `run_id` | history DTOs | `runs.rs:160`, `steps.rs:203` |
| `get_config`/`set_config` | - / `key,value` | snapshot / `()` echo `config.changed` | `Config::snapshot/set` (`config.rs:70,87`) |
| `metrics_history` | `weeks:u32` | `Vec<WeekDto>` | derive from `list_runs`/steps (minutes-saved); no bus source |
| `upcoming_schedule` | - | `Vec<ScheduleDto>` | scheduler crate; `library.schedule` needs a new command+topic |
| `compile_run` | `run_id` | `WorkflowManifestDto` echo `workflow.compiled` | compiler over the recorded trajectory |

DTOs are serde structs in the shell mapping core types -> the exact TS payloads in
`ui/src/bus/types.ts`. UI camelCase settings keys map to dotted `Config` keys
(e.g. `voiceEnabled` <-> `voice.enabled`, `config.rs:157,179`).

## 3. Run paths

### 3a. Teach (model-driven): `ExploreLoop`
`explore/mod.rs:107-150`. Async; publishes all `run.*` internally; `run_id` from
the `run.started` envelope. Assemble behind shell cfg mirroring `cli/run.rs:52-64`:
- perceiver: `UiaPerceiver::new()` (`real-uia`) else `FixturePerceiver`.
- executor synth: `WindowsSynthesizer` (`real-input`) else `MockSynthesizer`.
- planner `Box<dyn ModelBackend>`: real `HttpBackend` (`real-transport`, reqwest)
  else `MockPlannerBackend`. **Network only here, only with explicit config**
  (`OPERANT_LIVE_BACKEND` + provider/model/key, `backends/live_config.rs:33`).
Steered via `run.control.*` (`BusControl`). Wizard configures the backend (7).

### 3b. Replay (deterministic): `Replayer`
`replay/lib.rs:139`. Sync, model-free by crate graph (test `:372-393`). GAP: it
publishes nothing. The `run_saved_workflow` command wraps it: publish
`run.started`(mode replay) -> for each executed action publish
`run.step.gated`(pass) + `run.step.executed` -> `run.completed`, sourced from
`wf.actions` + `ReplayReport`. Do NOT pull `operant-orchestrator` into this command.

## 4. Kill switch enforcement (SAFETY, never-cut)
A1 found the explore loop honors `run.control.*` but NOT `killswitch.*`; the panic
button renders but does not stop a live loop. REQUIRED: a process-global freeze
(e.g. `AtomicBool` in `operant-action`/shell checked by `WindowsSynthesizer`
before every `SendInput`/`SetCursorPos`, and by the loop between actions). Panic
sets it (<100ms), blocking all real input synthesis immediately; release clears it.
Cover with a test: freeze set => synth calls become no-ops/Err. This is a gate.

## 5. Storage wiring
Canonical data dir keyed by identifier `dev.operant.shell` (NOT product name):
- Config JSON + recorder DB under `app_config_dir()`/`app_data_dir()`
  (`%APPDATA%\dev.operant.shell`); WebView2 localStorage already under
  `%LOCALAPPDATA%\dev.operant.shell`.
- `.manage(Arc<Bus>)`, `.manage(Arc<Recorder::open(app_data_dir()/"recorder.sqlite3")>)`,
  `.manage(Config::load_or_default(app_config_dir()/"config.json")?.with_bus(bus))`.
- ADD to core: `Recorder::list_workflows()` + `delete_workflow()` (`misc.rs`),
  registry `InstallStore::list()/delete()` (`install.rs:248`). Reconcile the
  duplicate auto-update setting (UI localStorage vs `updater-settings.json`) into
  `Config` as the single source.
Uninstaller already deletes both identifier dirs (fixed in `12bcf87`); only
`docs/KNOWN_ISSUES.md:41-45` + `docs/install.md:83` are stale (strike them).

## 6. Engine fixes (crates/action, crates/replay, cli)
- **E1 focus regex bug (blocks live replay):** `WindowsSynthesizer::focus_window`
  passes the IR regex `title_pattern` (`.* - Notepad`) literally to `FindWindowW`
  (`real_win.rs:151-163`). FIX: enumerate windows (`EnumWindows` +
  `GetWindowTextW`), regex-match `title_pattern`, also honor `process`; prefer
  focusing the HWND the perceiver already resolved. Test with a real regex title.
- **E2 focus-then-verify:** re-read the focused UIA element before keystrokes
  (`real_win.rs:10`), so type/key are not fire-and-hope.
- **E3 live gates:** run pre/post gates against live perception, not
  `bundled_notepad_snapshot()` (`cli/run.rs:37-41`, `snapshot.rs`), so PASS proves
  the desktop changed.
- **E4 flag footgun:** either `real-uia` or `real-input` alone silently degrades to
  mock (`run.rs:60`). Make a partial real build a hard error (or single feature).

## 7. Wizard real backend (teach setup)
Wizard provider setup (ChatGPT/Claude/API key/local model) must write real config
(`Config::set` model/provider/key) so `start_teach_run` assembles a real planner.
Local-model download + VRAM/disk probes stay a Tauri IPC surface (NOT the bus).
Demo mode (zero grants) may keep `MockPlannerBackend` for the first-run experience.

## 8. Concrete shapes
### 8a. Pump families to subscribe
`run.*`, `gate.*`, `approval.*`, `perception.*`, `sidecar.*`, `vram.*`,
`workflow.*`, `trigger.fired`, `schedule.*`, `killswitch.*`, `undo.*`, `doctor.*`,
`metrics.*`, `suggestion.*`, `config.changed`, `voice.*`. (Forward all so the
Advanced audit sink `main.ts:737` shows real activity.)
### 8b. Screens (from the UI contract survey)
Most screens need NO change once the real client emits the same envelopes. Invert
the three self-publishing screens (RunViewer stop/pause/intervene, Tray
panic/pauseAll, Undo open/confirm) to call commands (table 2); the core echoes the
event back and existing handlers render it. Fill the non-bus queries (metrics
history, workflow list, config values, undo journal, frecency, schedule) via
commands at mount. Settings must also subscribe to `config.changed` to avoid stale
values. Doctor + Gallery exist but are unmounted; wire if time permits.

## 9. Open decisions / gaps to honor
- Stop semantics: teach stop = freeze + cooperative pause (~1s) then ensure
  `end_run`+closing event; never leave a run row open.
- `run_id` returns via `run.started`, not the command result; UI already reacts.
- Keep `simulateDemoRun`/mock for non-Tauri dev; never ship it as the real path.
- Every real path stays behind shell features (`real-uia`/`real-input`/
  `real-transport`); default `just verify` build stays deterministic + offline so
  golden + airgap gates stay green.
