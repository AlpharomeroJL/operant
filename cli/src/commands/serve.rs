//! `operant serve [--data <dir>] [--db <path>]`: the CORE side of the
//! shell-to-core IPC bridge (`contracts/ipc.md`), spoken over this process's
//! stdio as newline-delimited JSON frames.
//!
//! On start it writes the unsolicited `ready` frame, then loops reading `req`
//! lines from stdin, dispatches each to the mapped core API, and writes exactly
//! one `res` (same `id`) per `req`. A background writer thread drains the
//! in-process `operant_core::Bus` and forwards every bus envelope to stdout as an
//! `evt` frame, reusing the exact family subscriptions the fixture recorder
//! (`cli/src/commands/record_ipc.rs`) uses. This is the same binary and the same
//! engine the CLI verbs drive, which is the whole point of the contract: the app
//! and the CLI share one execution path.
//!
//! Determinism and honesty are load-bearing:
//! - `get_capabilities` reports THIS build's real cfg flags (`real-uia` /
//!   `real-input`), so a default (mock) build truthfully reports the BLOCKING
//!   case and the shell can gate real-work UI on it. No mock ever masquerades as
//!   a real path (`contracts/ipc.md` section 3).
//! - Replay stays model-free and offline in every build (`Replayer` alone, never
//!   the orchestrator), so `just golden` and the airgap checks stay green.
//! - Backends follow build cfg exactly as `cli/src/commands/{run,explore}.rs` do:
//!   `UiaPerceiver`/`WindowsSynthesizer` behind `real-uia`/`real-input`, the
//!   deterministic mock synthesizer + fixture perceiver otherwise. The default
//!   build compiles and runs entirely offline.
//!
//! The five commands the contract reserves but does not wire in this build
//! (`probe_backend`, `delete_workflow`, `list_triggers`, `upsert_trigger`)
//! answer with the `not_implemented` error, never a faked result
//! (`contracts/ipc.md` section 5g). `get_capabilities`, the fifth reserved
//! command, IS wired here because the handshake depends on it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Write as _};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::Engine as _;
use serde_json::{json, Value};

use operant_action::{Executor, Synthesizer};
use operant_compiler::{compile, Trajectory};
use operant_core::bus::events::{
    HaltReason, KillswitchEngaged, RunCompleted, RunHalted, RunMode as BusRunMode,
    RunOutcome as BusRunOutcome, RunStarted, RunStepExecuted, RunStepGated, StepOutcome,
};
use operant_core::config::Config;
use operant_core::{safety, Bus, Perceiver};
use operant_gates::EvalContext;
use operant_ir::bus::Envelope;
use operant_ir::{ActionKind, GateKind, GateResult};
use operant_orchestrator::backends::ModelBackend;
use operant_orchestrator::explore::{BusControl, ExploreLoop};
use operant_orchestrator::watch::{CappedBuffer, EventSink};
use operant_recorder::{Recorder, RunMode, RunStatus};
use operant_replay::{CompiledWorkflow, Replayer};

use crate::commands::run::load_compiled;

/// The IPC protocol version this core speaks (`contracts/ipc.md`, the `pv`
/// field). Distinct from the bus envelope `v`.
const PROTOCOL_VERSION: u32 = 1;

/// Max bytes in one inbound frame (`contracts/ipc.md` section 1). A longer line
/// is refused rather than buffered without bound.
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// The bounded event queue between the bus pump and the stdout writer
/// (`contracts/ipc.md` section 8a). When it fills, the OLDEST `evt` frames are
/// dropped and a single `bus.overflow` marker is emitted once the writer catches
/// up, so a shell that stops draining stdout can never block the core.
const EVENT_QUEUE_CAP: usize = 8192;

/// Every top-level bus family the pump forwards (`contracts/ipc.md` section 6),
/// identical to the recorder's set. Disjoint top-level prefixes, so no envelope
/// is delivered to two of these.
const FAMILIES: &[&str] = &[
    "run.*",
    "gate.*",
    "approval.*",
    "perception.*",
    "sidecar.*",
    "vram.*",
    "workflow.*",
    "trigger.fired",
    "schedule.*",
    "killswitch.*",
    "undo.*",
    "doctor.*",
    "metrics.*",
    "suggestion.*",
    "config.changed",
    "voice.*",
];

// ==========================================================================
// Entry point
// ==========================================================================

pub fn run(args: &[String]) -> Result<()> {
    let Some(opts) = Opts::parse(args)? else {
        return Ok(());
    };

    std::fs::create_dir_all(&opts.data)
        .with_context(|| format!("creating the serve data directory {}", opts.data.display()))?;
    let db_path = opts
        .db
        .clone()
        .unwrap_or_else(|| opts.data.join("recorder.sqlite3"));

    let core = Core::open(&db_path.to_string_lossy(), opts.data.clone())?;

    // Writer thread owns stdout exclusively so no two frames ever interleave.
    let outbox = Arc::new(Outbox::new(EVENT_QUEUE_CAP));
    let writer = {
        let outbox = outbox.clone();
        std::thread::spawn(move || run_writer(&outbox))
    };

    // ready is the FIRST frame the core writes (`contracts/ipc.md` section 2a).
    // Control frames are prioritized over events in the writer, so this lands
    // before any orphan-reconciliation event below.
    outbox.send_ctrl(ready_frame());

    // Subscribe every family and start forwarding BEFORE reconciliation so the
    // orphan `run.halted` events reach the shell.
    let mut pumps = Vec::new();
    for family in FAMILIES {
        let sub = core.bus.subscribe(family);
        let outbox = outbox.clone();
        pumps.push(std::thread::spawn(move || {
            for env in sub.rx.iter() {
                if should_forward(&env.topic) {
                    outbox.send_evt(env.seq, evt_frame(&env));
                }
            }
        }));
    }

    // Orphan reconciliation (`contracts/ipc.md` section 8b): close any run left
    // `running` by a previous crashed process, so no run row is ever open.
    core.reconcile_orphans();

    // Read loop: one line is one frame. Exactly one `res` per `req`.
    let mut stdin = io::stdin().lock();
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = match stdin.read_line(&mut buf) {
            Ok(0) => break, // stdin EOF: the shell is gone (section 8c).
            Ok(n) => n,
            Err(e) => {
                eprintln!("serve: stdin read error: {e}");
                break;
            }
        };
        if n > MAX_FRAME_BYTES {
            eprintln!("serve: dropping a line over the {MAX_FRAME_BYTES}-byte frame cap");
            continue;
        }
        if let Some(res) = handle_line(&core, &buf) {
            outbox.send_ctrl(res);
        }
    }

    // Graceful shutdown (section 8c): close any active run, flush, exit 0.
    core.shutdown();
    // Give the pump a moment to forward the closing event, then stop the writer.
    std::thread::sleep(Duration::from_millis(50));
    outbox.close();
    let _ = writer.join();
    let _ = pumps; // detached; they end when the bus (in `core`) is dropped.
    Ok(())
}

/// Parse, validate, and dispatch one inbound line. Returns the `res` frame to
/// send, or `None` when the line carries no answerable request (a blank line, a
/// malformed frame that cannot be correlated, or a non-`req` frame).
fn handle_line(core: &Core, raw: &str) -> Option<Value> {
    // Tolerate a leading UTF-8 BOM (PowerShell producers emit one) and trailing
    // CR/LF; `read_line` keeps the newline.
    let line = raw
        .trim_start_matches('\u{feff}')
        .trim_end_matches(['\r', '\n'])
        .trim();
    if line.is_empty() {
        return None;
    }

    let obj = match serde_json::from_str::<Value>(line) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            // Not a JSON object: cannot correlate a `res`, so log to stderr and
            // continue at the next newline (`contracts/ipc.md` section 1).
            eprintln!("serve: dropping a line that is not a JSON object");
            return None;
        }
    };

    let id = obj.get("id").and_then(Value::as_str);
    if let Some(pv) = obj.get("pv").and_then(Value::as_u64) {
        if pv != PROTOCOL_VERSION as u64 {
            return Some(res_err(
                id.unwrap_or(""),
                IpcError::new(
                    "unsupported_protocol",
                    format!("this core speaks IPC protocol version {PROTOCOL_VERSION}"),
                    false,
                ),
            ));
        }
    }

    if obj.get("t").and_then(Value::as_str) != Some("req") {
        eprintln!("serve: ignoring a non-req frame");
        return None;
    }
    let Some(id) = id else {
        eprintln!("serve: ignoring a req with no correlation id");
        return None;
    };
    let Some(cmd) = obj.get("cmd").and_then(Value::as_str) else {
        return Some(res_err(
            id,
            IpcError::new("bad_request", "req is missing `cmd`".into(), false),
        ));
    };
    let args = obj.get("args").cloned().unwrap_or_else(|| json!({}));

    match core.dispatch(cmd, &args) {
        Ok(result) => Some(res_ok(id, result)),
        Err(err) => Some(res_err(id, err)),
    }
}

// ==========================================================================
// Frame builders (`contracts/ipc.md` section 2)
// ==========================================================================

fn ready_frame() -> Value {
    json!({ "t": "ready", "pv": PROTOCOL_VERSION })
}

fn res_ok(id: &str, result: Value) -> Value {
    json!({ "t": "res", "pv": PROTOCOL_VERSION, "id": id, "ok": true, "result": result })
}

fn res_err(id: &str, err: IpcError) -> Value {
    json!({
        "t": "res",
        "pv": PROTOCOL_VERSION,
        "id": id,
        "ok": false,
        "error": { "code": err.code, "message": err.message, "retryable": err.retryable }
    })
}

/// Wrap a captured bus envelope as an `evt` frame. `thumb` is null: this core is
/// headless (mock perception), so there is no screenshot to redact and downscale
/// (`contracts/ipc.md` section 7). A real-vision core populates it on
/// `run.step.executed` frames.
fn evt_frame(env: &Envelope) -> Value {
    json!({
        "t": "evt",
        "pv": PROTOCOL_VERSION,
        "env": serde_json::to_value(env).unwrap_or(Value::Null),
        "thumb": Value::Null
    })
}

/// The backpressure marker (`contracts/ipc.md` section 6): a synthetic
/// `bus.overflow` event the writer emits after dropping `dropped` event frames,
/// carrying the `seq` at which delivery resumes so the shell can reconcile the
/// gap. Never dropped itself.
fn overflow_frame(dropped: u64, resume_seq: u64) -> Value {
    let env = json!({
        "v": 1,
        "seq": resume_seq,
        "ts": format!("seq:{resume_seq}"),
        "topic": "bus.overflow",
        "payload": { "dropped": dropped, "resume_seq": resume_seq }
    });
    json!({ "t": "evt", "pv": PROTOCOL_VERSION, "env": env, "thumb": Value::Null })
}

/// The `run.control.*` topics are shell-to-core COMMANDS, not events, and are
/// never forwarded back as `evt` frames (`contracts/ipc.md` section 6). The
/// `run.*` family subscription would otherwise catch them.
fn should_forward(topic: &str) -> bool {
    !topic.starts_with("run.control.")
}

// ==========================================================================
// Typed errors (`contracts/ipc.md` section 2c catalog)
// ==========================================================================

#[derive(Debug)]
struct IpcError {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl IpcError {
    fn new(code: &'static str, message: String, retryable: bool) -> Self {
        Self {
            code,
            message,
            retryable,
        }
    }
    fn not_implemented(cmd: &str) -> Self {
        Self::new(
            "not_implemented",
            format!("`{cmd}` is reserved in this contract but not wired in this build"),
            false,
        )
    }
    fn unknown_command(cmd: &str) -> Self {
        Self::new(
            "unknown_command",
            format!("unknown command `{cmd}`"),
            false,
        )
    }
    fn invalid_args(message: impl Into<String>) -> Self {
        Self::new("invalid_args", message.into(), false)
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message.into(), false)
    }
    fn conflict(message: impl Into<String>) -> Self {
        Self::new("conflict", message.into(), false)
    }
    fn refused(message: impl Into<String>) -> Self {
        Self::new("refused", message.into(), false)
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message.into(), true)
    }
}

// ==========================================================================
// The stdout writer: control frames prioritized, events lossy under pressure
// ==========================================================================

struct Outbox {
    inner: Mutex<OutboxInner>,
    signal: Condvar,
    capacity: usize,
}

struct OutboxInner {
    /// `ready`/`res` frames: the control plane, small and correlated, NEVER
    /// dropped (`contracts/ipc.md` section 8a).
    ctrl: std::collections::VecDeque<Value>,
    /// `evt` frames: lossy. Bounded ring, oldest dropped first.
    evt: std::collections::VecDeque<(u64, Value)>,
    dropped: u64,
    closed: bool,
}

impl Outbox {
    fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(OutboxInner {
                ctrl: std::collections::VecDeque::new(),
                evt: std::collections::VecDeque::new(),
                dropped: 0,
                closed: false,
            }),
            signal: Condvar::new(),
            capacity,
        }
    }

    fn send_ctrl(&self, frame: Value) {
        let mut g = self.inner.lock().unwrap();
        g.ctrl.push_back(frame);
        self.signal.notify_one();
    }

    fn send_evt(&self, seq: u64, frame: Value) {
        let mut g = self.inner.lock().unwrap();
        if g.evt.len() >= self.capacity {
            g.evt.pop_front();
            g.dropped += 1;
        }
        g.evt.push_back((seq, frame));
        self.signal.notify_one();
    }

    fn close(&self) {
        let mut g = self.inner.lock().unwrap();
        g.closed = true;
        self.signal.notify_one();
    }
}

/// One outbound item the writer thread pulls. Control frames win over events,
/// and a pending overflow marker precedes the events it summarizes.
enum Outbound {
    Frame(Value),
    Overflow { dropped: u64, resume_seq: u64 },
    Done,
}

fn run_writer(outbox: &Outbox) {
    let mut stdout = io::stdout();
    loop {
        let item = {
            let mut g = outbox.inner.lock().unwrap();
            loop {
                if let Some(frame) = g.ctrl.pop_front() {
                    break Outbound::Frame(frame);
                }
                if g.dropped > 0 {
                    let dropped = std::mem::take(&mut g.dropped);
                    let resume_seq = g.evt.front().map(|(s, _)| *s).unwrap_or(0);
                    break Outbound::Overflow {
                        dropped,
                        resume_seq,
                    };
                }
                if let Some((_, frame)) = g.evt.pop_front() {
                    break Outbound::Frame(frame);
                }
                if g.closed {
                    break Outbound::Done;
                }
                g = outbox.signal.wait(g).unwrap();
            }
        };
        match item {
            Outbound::Frame(frame) => write_line(&mut stdout, &frame),
            Outbound::Overflow {
                dropped,
                resume_seq,
            } => write_line(&mut stdout, &overflow_frame(dropped, resume_seq)),
            Outbound::Done => return,
        }
    }
}

fn write_line(stdout: &mut io::Stdout, frame: &Value) {
    // `serde_json::to_string` is compact and contains no literal newline, so one
    // frame is exactly one line (`contracts/ipc.md` section 1).
    if let Ok(s) = serde_json::to_string(frame) {
        let _ = stdout.write_all(s.as_bytes());
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
    }
}

// ==========================================================================
// The core engine
// ==========================================================================

struct Core {
    bus: Arc<Bus>,
    recorder: Arc<Recorder>,
    config: Config,
    rt: tokio::runtime::Runtime,
    data_dir: PathBuf,
    /// The local observation buffer the watch-and-suggest detector fills. Held
    /// here so `purge_observation_buffer` is a REAL purge with an honest
    /// lifetime write count (`contracts/ipc.md` section 5f).
    obs_buffer: Mutex<CappedBuffer>,
    /// The single active run's id, if any (the serialized run queue,
    /// `docs/ARCHITECTURE.md` section 5). Targets `pause`/`stop`/`kill`.
    active_run: Arc<Mutex<Option<String>>>,
    /// The background explore task, so `stop`/`kill` can abort it.
    active_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Core {
    fn open(db_path: &str, data_dir: PathBuf) -> Result<Self> {
        let bus = Arc::new(Bus::new());
        let recorder = Arc::new(
            Recorder::open(db_path).with_context(|| format!("opening the recorder at {db_path}"))?,
        );
        let config = Config::with_bus(bus.clone());
        let rt = tokio::runtime::Runtime::new().context("starting the tokio runtime")?;
        Ok(Self {
            bus,
            recorder,
            config,
            rt,
            data_dir,
            obs_buffer: Mutex::new(CappedBuffer::new(4096)),
            active_run: Arc::new(Mutex::new(None)),
            active_task: Mutex::new(None),
        })
    }

    /// The command dispatch table (`contracts/ipc.md` section 5): each command
    /// name maps to the core API named in the contract, or to `not_implemented`
    /// for the reserved-but-unwired set.
    fn dispatch(&self, cmd: &str, args: &Value) -> std::result::Result<Value, IpcError> {
        match cmd {
            // 5a. Lifecycle and capabilities
            "get_capabilities" => Ok(capabilities()),
            "configure_backend" => self.configure_backend(args),
            "probe_backend" => Err(IpcError::not_implemented("probe_backend")),

            // 5b. Run control
            "list_windows" => self.list_windows(),
            "start_explore" => self.start_explore(args),
            "start_replay" => self.start_replay(args),
            "dry_run" => self.dry_run(args),
            "pause" => self.control("run.control.pause", json!({})),
            "resume" => self.control("run.control.resume", json!({})),
            "redirect" => {
                let instruction = arg_str(args, "instruction")?;
                self.control("run.control.redirect", json!({ "instruction": instruction }))
            }
            "stop" => self.stop(),
            "kill" => self.kill(),

            // 5c. Workflows and runs
            "list_workflows" => self.list_workflows(),
            "get_workflow" => self.get_workflow(args),
            "explain_workflow" => self.explain_workflow(args),
            "delete_workflow" => Err(IpcError::not_implemented("delete_workflow")),
            "compile_run" => self.compile_run(args),
            "list_runs" => self.list_runs(),
            "get_run" => self.get_run(args),
            "preview_undo" => self.preview_undo(args),
            "undo_run" => self.undo_run(args),

            // 5d. Registry
            "install_workflow" => self.install_workflow(args),
            "publish_workflow" => self.publish_workflow(args),

            // 5e. Scheduler and triggers
            "list_triggers" => Err(IpcError::not_implemented("list_triggers")),
            "upsert_trigger" => Err(IpcError::not_implemented("upsert_trigger")),

            // 5f. Diagnostics, metrics, settings, buffer, backup
            "get_metrics" => self.get_metrics(args),
            "run_doctor" => self.run_doctor(args),
            "get_settings" => Ok(json!(self.config.snapshot())),
            "set_settings" => self.set_settings(args),
            "purge_observation_buffer" => self.purge_observation_buffer(),
            "export_backup" => self.export_backup(),
            "import_backup" => self.import_backup(args),

            other => Err(IpcError::unknown_command(other)),
        }
    }

    // ---- 5a ----

    fn configure_backend(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        // Dotted `Config` keys, per `docs/specs/ipc-bridge.md` sections 2 and 7
        // ("Config::set model/provider/key"). Key naming is this lane's choice.
        // Each `set` echoes `config.changed` on the bus.
        let provider = arg_str(args, "provider")?;
        let model = arg_str(args, "model")?;
        self.config.set("model.provider", json!(provider));
        self.config.set("model.model", json!(model));
        if let Some(endpoint) = opt_str(args, "endpoint") {
            self.config.set("model.endpoint", json!(endpoint));
        }
        if let Some(api_key) = opt_str(args, "api_key") {
            self.config.set("model.api_key", json!(api_key));
        }
        Ok(json!({ "ok": true }))
    }

    // ---- 5b ----

    /// `list_windows` (ADR 0003): the open top-level windows the core can
    /// perceive, z-ordered topmost first, with Operant's own window excluded,
    /// so the palette target picker binds a teach to the app the user means
    /// rather than to Operant (the foreground window while the palette is
    /// open). A build without `real-uia` has no live windows to offer and
    /// returns an empty list; the UI then falls back to its switch-to-next path.
    fn list_windows(&self) -> std::result::Result<Value, IpcError> {
        #[cfg(feature = "real-uia")]
        let windows: Vec<Value> = operant_perception_uia::enumerate_windows()
            .into_iter()
            .filter(|w| {
                // Never offer Operant itself: excluding it is the whole point.
                let p = w.process.to_ascii_lowercase();
                !(p.contains("operant") || w.title == "Operant")
            })
            .map(|w| {
                json!({
                    "process": w.process,
                    "title": w.title,
                    "id": format!("{:#010x}", w.hwnd),
                })
            })
            .collect();
        #[cfg(not(feature = "real-uia"))]
        let windows: Vec<Value> = Vec::new();
        Ok(json!({ "windows": windows }))
    }

    fn start_explore(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let goal = arg_str(args, "goal")?;
        let window_process = arg_str(args, "window_process")?;
        if self.active_run.lock().unwrap().is_some() {
            return Err(IpcError::conflict("a run is already active"));
        }

        let planner = build_planner()?;
        let perceiver = build_perceiver(&window_process)?;

        // The synthesizer is cfg-selected exactly as `cli/src/commands/run.rs`
        // (E4): a real run drives real Windows input, the default build stays on
        // the deterministic mock synthesizer. `ExploreLoop` is generic over it.
        #[cfg(feature = "real-input")]
        {
            use operant_action::WindowsSynthesizer;
            let executor = Executor::new(WindowsSynthesizer::new());
            self.spawn_explore(perceiver, planner, executor, goal, window_process)
        }
        #[cfg(not(feature = "real-input"))]
        {
            use operant_action::{MockSynthesizer, NoopSleeper};
            let executor =
                Executor::new(MockSynthesizer::new()).with_sleeper(Box::new(NoopSleeper));
            self.spawn_explore(perceiver, planner, executor, goal, window_process)
        }
    }

    /// Spawn the explore loop on the runtime and return promptly with the
    /// `run_id` (`contracts/ipc.md` section 4: long-running work returns quickly
    /// and reports through `evt`). The read loop stays free to process
    /// `pause`/`stop`/`kill` while the run proceeds in the background, which is
    /// exactly what keeps the kill switch responsive during a real run.
    fn spawn_explore<S: Synthesizer + Send + 'static>(
        &self,
        perceiver: Box<dyn Perceiver>,
        planner: Box<dyn ModelBackend>,
        executor: Executor<S>,
        goal: String,
        window_process: String,
    ) -> std::result::Result<Value, IpcError> {
        let bus = self.bus.clone();
        let recorder = self.recorder.clone();
        let active_run = self.active_run.clone();
        // `run_id` is canonical on `run.started` (`contracts/ipc.md` section 4);
        // subscribe before spawning so we can read it back for the result.
        let started = self.bus.subscribe("run.started");

        let handle = self.rt.spawn(async move {
            let explore = ExploreLoop::new(perceiver, planner, executor, window_process);
            // `BusControl` steers the run from `run.control.*`, so the
            // `pause`/`resume`/`redirect` commands actually reach the loop.
            let mut control = BusControl::subscribe(&bus);
            let _ = explore.run(&bus, &recorder, &goal, &mut control).await;
            *active_run.lock().unwrap() = None;
        });

        match started.rx.recv_timeout(Duration::from_secs(10)) {
            Ok(env) => {
                let run_id = env
                    .payload
                    .get("run_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_default();
                *self.active_run.lock().unwrap() = Some(run_id.clone());
                *self.active_task.lock().unwrap() = Some(handle);
                Ok(json!({ "run_id": run_id }))
            }
            Err(_) => {
                handle.abort();
                Err(IpcError::internal("the explore run did not start"))
            }
        }
    }

    fn start_replay(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let path = arg_str(args, "path")?;
        let inputs = inputs_from_args(args);
        let workflow =
            load_compiled(&path).map_err(|e| IpcError::invalid_args(format!("{e:#}")))?;
        let ctx = EvalContext::new().with_snapshot(crate::snapshot::bundled_notepad_snapshot());

        // A REAL replay (both `real-uia` and `real-input`) drives real input and
        // re-resolves selectors against live perception; every other build keeps
        // the deterministic, model-free mock path (`cli/src/commands/run.rs`).
        #[cfg(all(feature = "real-uia", feature = "real-input"))]
        let report = {
            use operant_action::WindowsSynthesizer;
            use operant_perception_uia::UiaPerceiver;
            let replayer = Replayer::new(WindowsSynthesizer::new())
                .with_perceiver(Box::new(UiaPerceiver::new()));
            replayer
                .replay_compiled(&workflow, &inputs, &ctx, &ctx)
                .map_err(|e| IpcError::internal(format!("{e}")))?
        };
        #[cfg(not(all(feature = "real-uia", feature = "real-input")))]
        let report = Replayer::with_mock()
            .replay_compiled(&workflow, &inputs, &ctx, &ctx)
            .map_err(|e| IpcError::internal(format!("{e}")))?;

        self.wrap_replay(&workflow, &report, RunMode::Replay)
    }

    fn dry_run(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let path = arg_str(args, "path")?;
        let inputs = inputs_from_args(args);
        let workflow =
            load_compiled(&path).map_err(|e| IpcError::invalid_args(format!("{e:#}")))?;
        let ctx = EvalContext::new().with_snapshot(crate::snapshot::bundled_notepad_snapshot());
        // dry_run NEVER dispatches a real synthesizer, in any build: the mock
        // replayer (`contracts/ipc.md` section 5b maps this to `Replayer::with_mock`).
        let report = Replayer::with_mock()
            .replay_compiled(&workflow, &inputs, &ctx, &ctx)
            .map_err(|e| IpcError::internal(format!("{e}")))?;
        self.wrap_replay(&workflow, &report, RunMode::Dry)
    }

    /// Record a replay/dry run row and wrap the (event-free) `Replayer` in the
    /// synthetic `run.*` envelope the `start_replay`/`dry_run` commands are
    /// contracted to publish (`contracts/ipc.md` section 5b), never pulling the
    /// orchestrator into the replay path. Mirrors the fixture recorder.
    fn wrap_replay(
        &self,
        workflow: &CompiledWorkflow,
        report: &operant_replay::ReplayReport,
        mode: RunMode,
    ) -> std::result::Result<Value, IpcError> {
        let name = workflow.manifest.name.clone();
        let goal = format!("Replay {name}");
        let run_id = self
            .recorder
            .start_run(&goal, mode, None)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        let bus_mode = match mode {
            RunMode::Dry => BusRunMode::Dry,
            _ => BusRunMode::Replay,
        };
        let _ = self.bus.publish_event(&RunStarted {
            run_id: run_id.clone(),
            goal,
            mode: bus_mode,
            workflow_name: Some(name),
        });
        let mut steps = 0u32;
        for action in &workflow.actions {
            if action.kind == ActionKind::Assert {
                continue; // never dispatched; surfaced as the postcondition gate
            }
            let _ = self.bus.publish_event(&RunStepGated {
                run_id: run_id.clone(),
                step_id: action.id.clone(),
                gate_kind: GateKind::Pre,
                result: GateResult::Pass,
                expr: None,
            });
            let _ = self.bus.publish_event(&RunStepExecuted {
                run_id: run_id.clone(),
                step_id: action.id.clone(),
                outcome: StepOutcome::Ok,
                ms: 0,
                grounding: action.grounding,
            });
            steps += 1;
        }
        let _ = self.bus.publish_event(&RunCompleted {
            run_id: run_id.clone(),
            outcome: BusRunOutcome::Ok,
            steps,
            wall_ms: 0,
        });
        self.recorder
            .end_run(&run_id, RunStatus::Completed)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        Ok(json!({
            "run_id": run_id,
            "steps_executed": report.steps_executed,
            "pre": gate_results(&report.pre),
            "post": gate_results(&report.post)
        }))
    }

    fn control(&self, topic: &str, payload: Value) -> std::result::Result<Value, IpcError> {
        // The observable outcome (`run.paused`/`run.redirected`/`run.resumed`)
        // arrives as an `evt` published by the loop; the `res` only confirms the
        // command was accepted (`contracts/ipc.md` section 4).
        self.bus.publish(topic, payload);
        Ok(json!({ "ok": true }))
    }

    fn stop(&self) -> std::result::Result<Value, IpcError> {
        // Path 1 freeze halts real input synthesis in-process immediately, then a
        // cooperative pause; then we ensure the run row closes (`contracts/ipc.md`
        // section 5b: the freeze + cooperative-pause + close orchestration).
        safety::set_frozen(true);
        self.bus.publish("run.control.pause", json!({}));
        if let Some(handle) = self.active_task.lock().unwrap().take() {
            handle.abort();
        }
        let active = self.active_run.lock().unwrap().take();
        if let Some(run_id) = active {
            let _ = self.bus.publish_event(&RunHalted {
                run_id: run_id.clone(),
                reason: HaltReason::Human,
                error_id: None,
            });
            let _ = self.recorder.end_run(&run_id, RunStatus::Aborted);
        }
        // stop ends this run, it does not disable the core: release the freeze so
        // a later run can synthesize input again.
        safety::set_frozen(false);
        Ok(json!({ "ok": true }))
    }

    fn kill(&self) -> std::result::Result<Value, IpcError> {
        // The panic path. Path 1 (freeze) has stopped input synthesis in-process;
        // path 2 (the shell hard-terminating this child) is the shell's job and
        // may cut the pipe before this best-effort `res` is written
        // (`contracts/ipc.md` section 5b). The freeze is NOT released here.
        safety::set_frozen(true);
        let _ = self.bus.publish_event(&KillswitchEngaged { at_ms: now_ms() });
        if let Some(handle) = self.active_task.lock().unwrap().take() {
            handle.abort();
        }
        if let Some(run_id) = self.active_run.lock().unwrap().take() {
            let _ = self.bus.publish_event(&RunHalted {
                run_id: run_id.clone(),
                reason: HaltReason::Killswitch,
                error_id: None,
            });
            let _ = self.recorder.end_run(&run_id, RunStatus::Aborted);
        }
        Ok(json!({ "ok": true }))
    }

    // ---- 5c ----

    fn list_workflows(&self) -> std::result::Result<Value, IpcError> {
        let workflows = self
            .recorder
            .list_workflows()
            .map_err(|e| IpcError::internal(e.to_string()))?;
        Ok(json!(workflows))
    }

    fn get_workflow(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let id = arg_str(args, "id")?;
        match self
            .recorder
            .get_workflow(&id)
            .map_err(|e| IpcError::internal(e.to_string()))?
        {
            Some(w) => Ok(json!(w)),
            None => Err(IpcError::not_found(format!("no workflow with id `{id}`"))),
        }
    }

    fn explain_workflow(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let path = arg_str(args, "path")?;
        // Renders via `@operant/sdk/render` through node, exactly as
        // `operant explain` (`contracts/ipc.md` section 5c). No Rust reimplementation.
        crate::commands::explain::render_workflow_json(&path)
            .map_err(|e| IpcError::internal(format!("{e:#}")))
    }

    fn compile_run(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let run_id = arg_str(args, "run_id")?;
        let run = self
            .recorder
            .get_run(&run_id)
            .map_err(|e| IpcError::internal(e.to_string()))?
            .ok_or_else(|| IpcError::not_found(format!("no run with id `{run_id}`")))?;
        let steps = self
            .recorder
            .list_steps(&run_id)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        let traj: Trajectory = serde_json::from_value(export_trajectory(&run, &steps))
            .map_err(|e| IpcError::internal(format!("re-parsing the trajectory: {e}")))?;
        let compilation =
            compile(&traj).map_err(|e| IpcError::internal(format!("compiling: {e}")))?;
        let name = compilation.workflow.manifest.name.clone();
        let version = compilation.workflow.manifest.version.clone();
        let step_count = compilation.workflow.actions.len();

        // Persist the compiled artifacts so the `workflow.compiled` event points
        // at real files a later `start_replay` can load.
        let out_dir = self.data_dir.join("compiled").join(&name);
        std::fs::create_dir_all(&out_dir).ok();
        let manifest_path = out_dir.join("manifest.json");
        let dsl_path = out_dir.join("workflow.ts");
        let compiled_path = out_dir.join("compiled.json");
        if let Ok(s) = serde_json::to_string_pretty(&compilation.workflow.manifest) {
            std::fs::write(&manifest_path, s).ok();
        }
        std::fs::write(&dsl_path, &compilation.dsl_source).ok();
        if let Ok(s) = serde_json::to_string_pretty(&compilation.workflow) {
            std::fs::write(&compiled_path, s).ok();
        }

        self.bus.publish(
            "workflow.compiled",
            json!({
                "name": name,
                "version": version,
                "manifest_path": manifest_path.to_string_lossy(),
                "dsl_path": dsl_path.to_string_lossy(),
                "source_run_id": run_id
            }),
        );
        Ok(json!({ "name": name, "version": version, "steps": step_count }))
    }

    fn list_runs(&self) -> std::result::Result<Value, IpcError> {
        let runs = self
            .recorder
            .list_runs()
            .map_err(|e| IpcError::internal(e.to_string()))?;
        Ok(json!(runs))
    }

    fn get_run(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let run_id = arg_str(args, "run_id")?;
        let run = self
            .recorder
            .get_run(&run_id)
            .map_err(|e| IpcError::internal(e.to_string()))?
            .ok_or_else(|| IpcError::not_found(format!("no run with id `{run_id}`")))?;
        let steps = self
            .recorder
            .list_steps(&run_id)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        Ok(json!({ "run": run, "steps": steps }))
    }

    fn preview_undo(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let run_id = arg_str(args, "run_id")?;
        self.require_run(&run_id)?;
        // The real recorder undo journal publishes the populated `undo.previewed`
        // items (`contracts/ipc.md` section 5c); the `res` only confirms.
        self.recorder
            .publish_undo_preview(&self.bus, &run_id)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        Ok(json!({ "ok": true }))
    }

    fn undo_run(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let run_id = arg_str(args, "run_id")?;
        self.require_run(&run_id)?;
        let narration = self
            .recorder
            .undo_run(&run_id)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        self.bus.publish(
            "undo.applied",
            json!({ "run_id": run_id, "restored": narration.len(), "narration": narration }),
        );
        Ok(json!({ "restored": narration.len() }))
    }

    // ---- 5d ----

    fn install_workflow(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        use operant_registry::{install as registry_install, Approval, FsStore};

        let manifest_json = arg_str(args, "manifest_json")?;
        let dsl_b64 = arg_str(args, "dsl_bytes")?;
        let dsl_bytes = base64::engine::general_purpose::STANDARD
            .decode(dsl_b64.as_bytes())
            .map_err(|e| IpcError::invalid_args(format!("dsl_bytes is not valid base64: {e}")))?;
        let publisher_key = match opt_str(args, "publisher_key") {
            Some(hex) => Some(
                operant_registry::parse_publisher_key_hex(&hex)
                    .map_err(|e| IpcError::invalid_args(format!("publisher_key: {e}")))?,
            ),
            None => None,
        };
        let approval = match args.get("approval") {
            Some(Value::Bool(true)) => Approval::Approved,
            Some(Value::String(s)) if s == "approved" => Approval::Approved,
            _ => Approval::Denied,
        };

        let pins_path = self.data_dir.join("pins.json");
        let mut pins = load_pins(&pins_path)?;
        let mut store = FsStore::new(self.data_dir.join("installed"));
        let installed = registry_install(
            manifest_json.as_bytes(),
            publisher_key.as_ref().map(|k| k.as_slice()),
            dsl_bytes,
            &mut pins,
            approval,
            &mut store,
        )
        .map_err(|e| IpcError::refused(format!("{e}")))?;
        save_pins(&pins_path, &pins)?;

        self.bus.publish(
            "workflow.installed",
            json!({
                "name": installed.name,
                "version": installed.version,
                "publisher": installed.publisher,
                "dry_run": installed.dry_run
            }),
        );
        Ok(json!({
            "name": installed.name,
            "version": installed.version,
            "publisher": installed.publisher,
            "dry_run": installed.dry_run
        }))
    }

    fn publish_workflow(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        // Signs via `operant_registry::{sign_manifest, dsl_hash}` and shells to
        // `git`, exactly as `operant publish` (`contracts/ipc.md` section 5d).
        let draft_path = arg_str(args, "draft_manifest_path")?;
        let dsl_path = arg_str(args, "dsl_path")?;
        let registry_dir = arg_str(args, "registry_dir")?;
        let key_path = arg_str(args, "key_path")?;
        let branch = opt_str(args, "branch");

        let draft = std::fs::read_to_string(&draft_path)
            .map_err(|e| IpcError::invalid_args(format!("reading {draft_path}: {e}")))?;
        let dsl_bytes = std::fs::read(&dsl_path)
            .map_err(|e| IpcError::invalid_args(format!("reading {dsl_path}: {e}")))?;
        let key_pem = std::fs::read_to_string(&key_path)
            .map_err(|e| IpcError::invalid_args(format!("reading {key_path}: {e}")))?;

        let outcome = crate::commands::publish::publish(
            &draft,
            &dsl_bytes,
            &key_pem,
            std::path::Path::new(&registry_dir),
            branch.as_deref(),
        )
        .map_err(|e| IpcError::internal(format!("{e:#}")))?;
        Ok(json!({ "branch": outcome.branch, "commit": outcome.commit }))
    }

    // ---- 5f ----

    fn get_metrics(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        // Aggregate per-week system metrics (`contracts/ipc.md` section 5f maps
        // this to `get_weekly_system_metrics`). The weeks that exist come from the
        // stored metrics rows, newest first; `weeks` optionally caps the count.
        let limit = args.get("weeks").and_then(Value::as_u64).map(|n| n as usize);
        let rows = self
            .recorder
            .list_metrics()
            .map_err(|e| IpcError::internal(e.to_string()))?;
        let mut weeks: Vec<String> = rows
            .into_iter()
            .map(|m| m.week)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        weeks.sort();
        weeks.reverse(); // newest first
        if let Some(limit) = limit {
            weeks.truncate(limit);
        }
        let mut out = Vec::new();
        for week in weeks {
            let m = self
                .recorder
                .get_weekly_system_metrics(&week)
                .map_err(|e| IpcError::internal(e.to_string()))?;
            let workflows: Vec<Value> = m
                .workflows
                .iter()
                .map(|w| {
                    json!({
                        "workflow_id": w.workflow_id,
                        "week": w.week,
                        "runs": w.runs,
                        "minutes_saved": w.minutes_saved
                    })
                })
                .collect();
            out.push(json!({
                "week": m.week,
                "minutes_saved_total": m.total_minutes_saved,
                "total_runs": m.total_runs,
                "workflows": workflows
            }));
        }
        Ok(json!(out))
    }

    fn run_doctor(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        use operant_doctor::{
            run_doctor_verb, AccessibilityPermissionCheck, AudioDevicesPresentCheck, Check,
            DiskFreeCheck, VramHeadroomCheck,
        };
        let drive = opt_str(args, "drive")
            .and_then(|s| s.chars().next())
            .unwrap_or('C');
        let min_disk_bytes = args
            .get("min_disk_gb")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            * 1_000_000_000;
        let checks: Vec<Box<dyn Check>> = vec![
            Box::new(DiskFreeCheck::windows_drive(min_disk_bytes, drive)),
            Box::new(AccessibilityPermissionCheck::best_effort()),
            Box::new(AudioDevicesPresentCheck::best_effort()),
            Box::new(VramHeadroomCheck::best_effort()),
        ];
        // Publishes each finding as `doctor.finding` on the bus (section 5f).
        let report = run_doctor_verb(&checks, Some(&self.bus));
        let findings = serde_json::to_value(&report.findings)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        Ok(json!({ "findings": findings, "exit_code": report.exit_code }))
    }

    fn set_settings(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let key = arg_str(args, "key")?;
        let value = args
            .get("value")
            .cloned()
            .ok_or_else(|| IpcError::invalid_args("`value` is required"))?;
        self.config.set(&key, value); // echoes config.changed
        Ok(json!({ "ok": true }))
    }

    fn purge_observation_buffer(&self) -> std::result::Result<Value, IpcError> {
        let mut buffer = self.obs_buffer.lock().unwrap();
        buffer.purge();
        // Purge clears stored events but NEVER resets the lifetime write count
        // (`contracts/ipc.md` section 5f, the "provably never written" signal).
        Ok(json!({ "purged": true, "total_writes": buffer.total_writes() }))
    }

    fn export_backup(&self) -> std::result::Result<Value, IpcError> {
        let settings = self.config.snapshot();
        let bytes = operant_recorder::backup::export(&self.recorder, &settings)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        Ok(json!({ "bytes_b64": b64 }))
    }

    fn import_backup(&self, args: &Value) -> std::result::Result<Value, IpcError> {
        let b64 = arg_str(args, "bytes_b64")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|e| IpcError::invalid_args(format!("bytes_b64 is not valid base64: {e}")))?;
        let mut settings = self.config.snapshot();
        let data = operant_recorder::backup::import(&bytes, &self.recorder, &mut settings)
            .map_err(|e| IpcError::internal(e.to_string()))?;
        // Re-apply imported settings so they persist and echo `config.changed`.
        for (key, value) in settings {
            self.config.set(&key, value);
        }
        Ok(json!({
            "imported": {
                "workflows": data.workflows.len(),
                "workflow_versions": data.workflow_versions.len(),
                "metrics": data.metrics.len()
            }
        }))
    }

    // ---- shared helpers ----

    fn require_run(&self, run_id: &str) -> std::result::Result<(), IpcError> {
        match self
            .recorder
            .get_run(run_id)
            .map_err(|e| IpcError::internal(e.to_string()))?
        {
            Some(_) => Ok(()),
            None => Err(IpcError::not_found(format!("no run with id `{run_id}`"))),
        }
    }

    /// On startup, mark any still-`running` run row as halted (`run.halted`,
    /// reason error), so a run interrupted by a previous crash is never left open
    /// (`contracts/ipc.md` section 8b, orphan reconciliation).
    fn reconcile_orphans(&self) {
        let Ok(run_ids) = self.recorder.list_runs() else {
            return;
        };
        for run_id in run_ids {
            if let Ok(Some(run)) = self.recorder.get_run(&run_id) {
                if run.status == RunStatus::Running {
                    let _ = self.bus.publish_event(&RunHalted {
                        run_id: run_id.clone(),
                        reason: HaltReason::Error,
                        error_id: Some("core_restart".to_string()),
                    });
                    let _ = self.recorder.end_run(&run_id, RunStatus::Aborted);
                }
            }
        }
    }

    /// Graceful shutdown on stdin EOF (`contracts/ipc.md` section 8c): close any
    /// active run so a run row is never left open, then let the writer flush.
    fn shutdown(&self) {
        if let Some(handle) = self.active_task.lock().unwrap().take() {
            handle.abort();
        }
        if let Some(run_id) = self.active_run.lock().unwrap().take() {
            let _ = self.bus.publish_event(&RunHalted {
                run_id: run_id.clone(),
                reason: HaltReason::Human,
                error_id: None,
            });
            let _ = self.recorder.end_run(&run_id, RunStatus::Aborted);
        }
    }
}

// ==========================================================================
// Capabilities (`contracts/ipc.md` section 3): the honest, load-bearing handshake
// ==========================================================================

/// This build's real capabilities, computed from the same cfg flags the rest of
/// the CLI uses. A default (mock) build reports `real_uia`/`real_input` false:
/// the BLOCKING case the shell must gate real-work UI on. Nothing here is faked.
fn capabilities() -> Value {
    json!({
        "real_uia": cfg!(feature = "real-uia"),
        "real_input": cfg!(feature = "real-input"),
        // No vision grounder sidecar is compiled into this binary.
        "real_vision": false,
        // The only planner in a non-bridge build is the scripted mock.
        "mock_planner_only": !cfg!(feature = "dev-agent-bridge"),
        "transport_kind": "stdio",
        "version": env!("CARGO_PKG_VERSION"),
        "git_sha": option_env!("OPERANT_GIT_SHA").unwrap_or("unknown")
    })
}

// ==========================================================================
// Backend assembly (cfg), mirroring cli/src/commands/explore.rs exactly
// ==========================================================================

fn build_planner() -> std::result::Result<Box<dyn ModelBackend>, IpcError> {
    #[cfg(feature = "dev-agent-bridge")]
    {
        use operant_orchestrator::backends::AgentBridgeBackend;
        let bridge = AgentBridgeBackend::from_env()
            .map_err(|e| IpcError::internal(format!("agent bridge: {e}")))?;
        Ok(Box::new(bridge))
    }
    #[cfg(not(feature = "dev-agent-bridge"))]
    {
        Ok(Box::new(crate::commands::explore::scripted_mock_planner()))
    }
}

fn build_perceiver(window_process: &str) -> std::result::Result<Box<dyn Perceiver>, IpcError> {
    #[cfg(feature = "real-uia")]
    {
        let _ = window_process;
        Ok(Box::new(operant_perception_uia::UiaPerceiver::new()))
    }
    #[cfg(not(feature = "real-uia"))]
    {
        let p = crate::commands::explore::fixture_perceiver(window_process)
            .map_err(|e| IpcError::internal(format!("{e:#}")))?;
        Ok(Box::new(p))
    }
}

// ==========================================================================
// Small helpers
// ==========================================================================

fn arg_str(args: &Value, key: &str) -> std::result::Result<String, IpcError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| IpcError::invalid_args(format!("`{key}` is required and must be a string")))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Parse `args.inputs` (a JSON object) into workflow input bindings. Values are
/// coerced to strings; a non-object or absent `inputs` yields no bindings.
fn inputs_from_args(args: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(obj) = args.get("inputs").and_then(Value::as_object) {
        for (k, v) in obj {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.insert(k.clone(), s);
        }
    }
    out
}

fn gate_results(results: &[GateResult]) -> Value {
    Value::Array(
        results
            .iter()
            .map(|r| match r {
                GateResult::Pass => json!("pass"),
                GateResult::Fail => json!("fail"),
            })
            .collect(),
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn load_pins(path: &std::path::Path) -> std::result::Result<operant_registry::PinStore, IpcError> {
    if !path.exists() {
        return Ok(operant_registry::PinStore::new());
    }
    let raw = std::fs::read(path)
        .map_err(|e| IpcError::internal(format!("reading pins {}: {e}", path.display())))?;
    let map: std::collections::HashMap<String, String> = serde_json::from_slice(&raw)
        .map_err(|e| IpcError::internal(format!("parsing pins: {e}")))?;
    Ok(operant_registry::PinStore::from_pins(map))
}

fn save_pins(
    path: &std::path::Path,
    pins: &operant_registry::PinStore,
) -> std::result::Result<(), IpcError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let bytes = serde_json::to_vec_pretty(pins.pins())
        .map_err(|e| IpcError::internal(format!("serializing pins: {e}")))?;
    std::fs::write(path, bytes)
        .map_err(|e| IpcError::internal(format!("writing pins {}: {e}", path.display())))
}

/// Turn a recorded run and its steps into the compiler's trajectory JSON,
/// identical to `cli/src/commands/explore.rs`'s `export_trajectory`.
fn export_trajectory(run: &operant_recorder::RunRecord, steps: &[operant_recorder::StepRecord]) -> Value {
    let steps_json: Vec<Value> = steps
        .iter()
        .map(|s| {
            let mut step = json!({
                "seq": s.seq,
                "action": s.action,
                "grounding": s.grounding,
                "outcome": s.outcome,
                "ms": s.ms,
                "outcome_bearing": s.outcome_bearing,
            });
            let obj = step.as_object_mut().expect("json object");
            if let Some(b) = &s.snapshot_digest_before {
                obj.insert("snapshot_digest_before".to_string(), json!(b));
            }
            if let Some(a) = &s.snapshot_digest_after {
                obj.insert("snapshot_digest_after".to_string(), json!(a));
            }
            if let Some(n) = &s.note {
                obj.insert("note".to_string(), json!(n));
            }
            if let Some(hc) = &s.human_correction {
                obj.insert("human_correction".to_string(), hc.clone());
            }
            step
        })
        .collect();
    json!({
        "v": 1,
        "description": format!("Recorded by `operant serve` from run {}", run.id),
        "run": {
            "id": run.id,
            "goal": run.goal,
            "mode": run.mode,
            "status": run.status,
            "started": run.started,
            "ended": run.ended,
            "model_config": run.model_config,
        },
        "steps": steps_json,
    })
}

// ==========================================================================
// Args
// ==========================================================================

struct Opts {
    data: PathBuf,
    db: Option<PathBuf>,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Option<Self>> {
        let mut data = None;
        let mut db = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => {
                    print_help();
                    return Ok(None);
                }
                "--data" => {
                    i += 1;
                    data = Some(PathBuf::from(
                        args.get(i).cloned().context("--data needs a value")?,
                    ));
                }
                "--db" => {
                    i += 1;
                    db = Some(PathBuf::from(
                        args.get(i).cloned().context("--db needs a value")?,
                    ));
                }
                other => anyhow::bail!("operant serve: unexpected argument `{other}`"),
            }
            i += 1;
        }
        Ok(Some(Self {
            data: data.unwrap_or_else(|| PathBuf::from("out").join("serve")),
            db,
        }))
    }
}

fn print_help() {
    println!("operant serve [--data <dir>] [--db <path>]");
    println!();
    println!("Run the CORE side of the shell-to-core IPC bridge (contracts/ipc.md) over this");
    println!("process's stdio as newline-delimited JSON: write `ready`, then answer each `req`");
    println!("line with exactly one `res` and forward every bus event as an `evt` frame.");
    println!();
    println!("  --data  state directory (recorder db, install store, pins, compiled output);");
    println!("          default ./out/serve");
    println!("  --db    override the recorder database path (default <data>/recorder.sqlite3)");
}

// ==========================================================================
// Tests
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed fixture session, framed per `contracts/ipc.md`. Its explore
    /// section is what `operant serve` reproduces byte-for-byte through the same
    /// engine, so it is the conformance oracle for the `res`/`evt` shapes.
    const SESSION: &str =
        include_str!("../../../contracts/fixtures/ipc/session-explore-compile-replay-undo.jsonl");

    fn fixture_lines() -> Vec<Value> {
        SESSION
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("fixture line parses"))
            .collect()
    }

    fn test_core() -> Core {
        // ":memory:" is SQLite's private in-memory database (recorder store doc).
        let data = std::env::temp_dir().join(format!("operant-serve-test-{}", std::process::id()));
        std::fs::create_dir_all(&data).ok();
        Core::open(":memory:", data).expect("core opens")
    }

    /// Recursively rewrite the single explore run id to `run_0` and zero the
    /// volatile `ms`/`wall_ms` timings, exactly as the fixture recorder
    /// normalizes, so a live capture is comparable to the committed fixture.
    fn normalize(v: &mut Value, run_id: &str) {
        match v {
            Value::String(s) => {
                if s == run_id {
                    *s = "run_0".to_string();
                }
            }
            Value::Object(m) => {
                for (k, val) in m.iter_mut() {
                    if (k == "ms" || k == "wall_ms") && val.is_number() {
                        *val = json!(0);
                    } else {
                        normalize(val, run_id);
                    }
                }
            }
            Value::Array(a) => {
                for val in a.iter_mut() {
                    normalize(val, run_id);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn get_capabilities_matches_the_handshake_fixture() {
        let core = test_core();
        let result = core
            .dispatch("get_capabilities", &json!({}))
            .expect("get_capabilities succeeds");
        // The handshake fixture is a default (mock) build capture, which is what
        // `cargo test` builds: real_uia/real_input are false (the BLOCKING case).
        let lines = fixture_lines();
        let fixture = &lines[2]["result"]; // line 3: the capability response
        assert_eq!(&result, fixture, "capability object must match the fixture");
        assert_eq!(result["real_uia"], json!(false));
        assert_eq!(result["real_input"], json!(false));
        assert_eq!(result["mock_planner_only"], json!(true));
        assert_eq!(result["transport_kind"], json!("stdio"));
    }

    #[test]
    fn list_windows_returns_a_windows_array() {
        // The default (mock) build cargo test runs has no `real-uia`, so there
        // are no live windows to enumerate: list_windows must still answer the
        // contract shape (an object with a `windows` array, here empty) so the
        // palette target picker falls back cleanly (ADR 0003).
        let core = test_core();
        let result = core
            .dispatch("list_windows", &json!({}))
            .expect("list_windows succeeds");
        assert!(
            result["windows"].is_array(),
            "list_windows returns {{windows: [...]}}, got {result}"
        );
        assert_eq!(
            result["windows"].as_array().unwrap().len(),
            0,
            "no real-uia in the test build means no windows to offer"
        );
    }

    #[test]
    fn start_explore_res_and_evt_frames_match_the_fixture() {
        let core = test_core();
        // Subscribe as the pump would, plus a completion latch, BEFORE dispatch.
        let run_family = core.bus.subscribe("run.*");
        let completed = core.bus.subscribe("run.completed");

        // Feed the same `start_explore` req the fixture recorded (line 4's args).
        let result = core
            .dispatch(
                "start_explore",
                &json!({
                    "goal": "Write an invoice note in Notepad and save it",
                    "window_process": "notepad.exe"
                }),
            )
            .expect("start_explore succeeds");
        let run_id = result["run_id"].as_str().expect("run_id string").to_string();
        assert!(run_id.starts_with("run_"), "run_id is recorder-shaped");

        // The run proceeds in the background; wait for completion, then drain.
        completed
            .rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the mock explore run completes");

        // Build `evt` frames from the run family, normalized like the fixture.
        let mut produced: Vec<Value> = run_family
            .rx
            .try_iter()
            .map(|env| evt_frame(&env))
            .collect();
        for f in &mut produced {
            normalize(f, &run_id);
        }

        // The fixture's explore `evt` frames are lines 6..=16 (run.* only).
        let expected: Vec<Value> = fixture_lines()[5..=15].to_vec();
        assert_eq!(
            produced.len(),
            expected.len(),
            "explore emits the same number of run.* events as the fixture"
        );
        for (got, want) in produced.iter().zip(expected.iter()) {
            // Framing shape: every event is a well-formed `evt` frame.
            assert_eq!(got["t"], json!("evt"));
            assert_eq!(got["pv"], json!(PROTOCOL_VERSION));
            assert_eq!(got["thumb"], Value::Null, "headless core: thumb is null");
            assert!(got["env"].is_object(), "carries a bus envelope");
            // Full byte-shape conformance against the committed capture.
            assert_eq!(got, want, "evt frame matches the fixture");
        }
    }

    #[test]
    fn unknown_command_and_not_implemented_are_typed_errors() {
        let core = test_core();
        let reserved = ["probe_backend", "delete_workflow", "list_triggers", "upsert_trigger"];
        for cmd in reserved {
            let err = core.dispatch(cmd, &json!({})).err().expect("reserved is an error");
            assert_eq!(err.code, "not_implemented", "{cmd} is not_implemented");
            assert!(!err.retryable);
        }
        let err = core
            .dispatch("does_not_exist", &json!({}))
            .err()
            .expect("unknown is an error");
        assert_eq!(err.code, "unknown_command");
    }

    #[test]
    fn missing_run_is_not_found() {
        let core = test_core();
        let err = core
            .dispatch("get_run", &json!({ "run_id": "run_nope" }))
            .err()
            .expect("a missing run is an error");
        assert_eq!(err.code, "not_found");
    }
}
