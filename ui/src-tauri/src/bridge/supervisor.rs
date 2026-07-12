// The supervised core sidecar (docs/adr/0002-core-sidecar-ipc.md).
//
// This is the shell side of the C1 supervision pattern turned on the core
// itself: spawn the `operant` binary as a child running `serve`, run the
// capability handshake, watch that it stays alive (health = child alive), and
// restart it if it dies. It also owns kill-switch path 2: the unblockable hard
// terminate of the child (contracts/ipc.md sections 5b and 8c), complementing
// the core's own in-process freeze (path 1).
//
// The supervisor is deliberately free of any Tauri type. Everything the outside
// world observes (events for the webview, a restart toast, status changes)
// arrives through injected sinks, so the whole spawn/supervise/handshake/kill
// state machine is driven in tests by a fake core that replays the frozen
// fixtures, with no Tauri app and no real process. We do NOT link the core in
// process (the ADR's whole point); the shell talks to its child purely over
// stdio.

use std::io::{BufRead, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use super::protocol::Capabilities;
use super::transport::{read_loop, Channel};

/// Whether the shell wants a core running. A crash while `Running` is restarted;
/// an intentional `core_kill` sets `Stopped`, which the watchdog honors by NOT
/// bringing the core back (contracts/ipc.md section 8c: hard stop is a stop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesiredState {
    Running,
    Stopped,
}

/// The three streams and the liveness handle of one spawned core process.
pub struct CoreProcess {
    /// The child's stdin: the shell writes `req` frames here.
    pub stdin: Box<dyn Write + Send>,
    /// The child's stdout: `ready` / `res` / `evt` frames are read from here.
    pub stdout: Box<dyn BufRead + Send>,
    /// Liveness and termination.
    pub handle: Box<dyn ProcessHandle>,
}

/// Liveness probe and hard terminate for one child. Abstracted so tests drive a
/// fake whose exit is fully controlled, never a real spawned process.
pub trait ProcessHandle: Send {
    /// Non-blocking: `Some(exit_code)` if the child has exited, `None` if it is
    /// still running. This is the watchdog's health signal.
    fn try_wait(&mut self) -> std::io::Result<Option<i32>>;
    /// Hard-terminate now (Windows `TerminateProcess` under the hood). A wedged
    /// core cannot refuse this, which is why it is kill path 2.
    fn kill(&mut self) -> std::io::Result<()>;
    /// The OS process id, for logs and the kill report.
    fn pid(&self) -> u32;
}

/// Spawns a fresh core process. The real implementation runs `operant serve`;
/// tests inject a fake that replays a recorded session.
pub trait CoreSpawner: Send {
    fn spawn(&mut self) -> std::io::Result<CoreProcess>;
}

/// Tunables. The defaults suit production; tests shrink them so the state
/// machine runs in milliseconds without wall-clock flakiness.
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// How often the watchdog probes child liveness.
    pub poll_interval: Duration,
    /// How long to wait for the `ready` frame after a spawn.
    pub ready_timeout: Duration,
    /// How long to wait for the `get_capabilities` answer.
    pub handshake_timeout: Duration,
    /// Default deadline for a `core_call`.
    pub call_timeout: Duration,
    /// First restart backoff, doubled up to `backoff_max`.
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(200),
            ready_timeout: Duration::from_secs(5),
            handshake_timeout: Duration::from_secs(5),
            call_timeout: Duration::from_secs(30),
            backoff_initial: Duration::from_millis(500),
            backoff_max: Duration::from_secs(10),
        }
    }
}

/// The bridge state the webview reads: whether the core is connected, whether it
/// can actually automate (the gate for B3's blocking screen), and the handshake
/// result. Emitted on every transition over the status channel.
#[derive(Debug, Clone, Serialize)]
pub struct CoreStatus {
    /// The child is up and the handshake has completed.
    pub connected: bool,
    /// `connected` AND the capabilities permit real automation. B3 gates the
    /// blocking screen on this being false.
    pub core_ready: bool,
    /// "running" or "stopped": what the shell currently wants.
    pub desired: String,
    /// The capability handshake result, once known.
    pub capabilities: Option<Capabilities>,
    /// How many times the core has been restarted this session.
    pub restarts: u32,
    /// The most recent supervision error, if any (spawn failure, crash exit,
    /// kill), for the webview to surface.
    pub last_error: Option<String>,
}

impl Default for CoreStatus {
    fn default() -> Self {
        Self {
            connected: false,
            core_ready: false,
            desired: "running".to_string(),
            capabilities: None,
            restarts: 0,
            last_error: None,
        }
    }
}

/// The result of `core_kill`, so the panic path can confirm the child is gone
/// and how fast it went (contracts/ipc.md section 5b requires this be fast).
#[derive(Debug, Clone, Serialize)]
pub struct KillReport {
    pub killed: bool,
    pub pid: u32,
    pub elapsed_ms: u64,
}

pub type EvtSink = Arc<dyn Fn(super::protocol::EvtFrame) + Send + Sync>;
pub type StatusSink = Arc<dyn Fn(&CoreStatus) + Send + Sync>;
pub type ToastSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Owns the child lifecycle and the bridge state.
pub struct Supervisor {
    spawner: Mutex<Box<dyn CoreSpawner>>,
    config: SupervisorConfig,
    channel: Arc<Channel>,
    current: Mutex<Option<Box<dyn ProcessHandle>>>,
    status: Mutex<CoreStatus>,
    desired: Mutex<DesiredState>,
    desired_cv: Condvar,
    shutdown: AtomicBool,
    /// Set by an explicit restart so the crash toast is suppressed for it.
    expect_restart: AtomicBool,
    on_evt: EvtSink,
    on_status: StatusSink,
    on_toast: ToastSink,
}

impl Supervisor {
    pub fn new(
        spawner: Box<dyn CoreSpawner>,
        config: SupervisorConfig,
        on_evt: EvtSink,
        on_status: StatusSink,
        on_toast: ToastSink,
    ) -> Self {
        Self {
            spawner: Mutex::new(spawner),
            config,
            channel: Arc::new(Channel::new()),
            current: Mutex::new(None),
            status: Mutex::new(CoreStatus::default()),
            desired: Mutex::new(DesiredState::Running),
            desired_cv: Condvar::new(),
            shutdown: AtomicBool::new(false),
            expect_restart: AtomicBool::new(false),
            on_evt,
            on_status,
            on_toast,
        }
    }

    /// Start the supervision thread. It spawns the core, handshakes, watches for
    /// death, and restarts, for the life of the app.
    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        thread::Builder::new()
            .name("operant-core-supervisor".to_string())
            .spawn(move || me.supervise())
            .expect("supervision thread spawns");
    }

    /// A snapshot of the current bridge state.
    pub fn status(&self) -> CoreStatus {
        self.status.lock().unwrap().clone()
    }

    /// Whether the core is up, handshaken, and able to automate.
    pub fn core_ready(&self) -> bool {
        self.status.lock().unwrap().core_ready
    }

    /// The capability handshake result, if the handshake has completed.
    pub fn capabilities(&self) -> Option<Capabilities> {
        self.status.lock().unwrap().capabilities.clone()
    }

    /// Proxy one `req`/`res` to the core (the `core_call` command). Blocking;
    /// the command layer runs it off the main thread.
    pub fn call(&self, cmd: &str, args: Value) -> Result<Value, super::protocol::CoreError> {
        self.channel.call(cmd, args, self.config.call_timeout)
    }

    // -- kill-switch path 2: hard terminate ---------------------------------

    /// Hard-terminate the child immediately and keep it down. This is the panic
    /// path (contracts/ipc.md section 5b). It first fires a best-effort `kill`
    /// command as a courtesy nudge for the core's own in-process freeze (path
    /// 1), then terminates the process (path 2). Neither step can block on a
    /// wedged core: the nudge is non-blocking, and terminate does not depend on
    /// the core cooperating.
    pub fn kill(&self) -> Result<KillReport, super::protocol::CoreError> {
        let start = Instant::now();

        // Path 2 must never restart the core out from under a stop.
        self.set_desired(DesiredState::Stopped);

        // Path 1 nudge: best-effort, non-blocking. Ignored if the writer is
        // contended or gone; the terminate below is the guarantee.
        let _ = self.channel.try_fire_and_forget("kill");

        // Path 2: terminate and confirm the process is gone.
        let mut pid = 0;
        let killed = {
            let mut guard = self.current.lock().unwrap();
            match guard.as_mut() {
                Some(handle) => {
                    pid = handle.pid();
                    let _ = handle.kill();
                    // Confirm exit quickly; TerminateProcess is effectively
                    // immediate, this just reaps. Bounded so a stuck reap can
                    // never hang the panic path.
                    let mut confirmed = false;
                    for _ in 0..50 {
                        if matches!(handle.try_wait(), Ok(Some(_))) {
                            confirmed = true;
                            break;
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                    confirmed
                }
                None => false,
            }
        };

        // Release any in-flight callers immediately.
        self.channel.fail_all();

        let elapsed_ms = start.elapsed().as_millis() as u64;

        {
            let mut status = self.status.lock().unwrap();
            status.connected = false;
            status.core_ready = false;
            status.desired = "stopped".to_string();
            status.last_error = Some(format!("core killed (pid {pid})"));
        }
        self.emit_status();

        Ok(KillReport {
            killed,
            pid,
            elapsed_ms,
        })
    }

    /// Bring the core back after an intentional kill (or force a fresh process
    /// while running). The webview offers this so a killed core is recoverable
    /// without relaunching the app.
    pub fn request_restart(&self) {
        self.set_desired(DesiredState::Running);
        // If a child is still alive, terminate it so the watchdog respawns a
        // clean process and re-handshakes. Only then arm the crash-toast
        // suppression: we asked for this death, so it should be silent. If no
        // child is alive (a prior kill already stopped it), the watchdog was
        // parked and the desired-state change above wakes it to respawn, with
        // no crash path to suppress.
        if let Some(handle) = self.current.lock().unwrap().as_mut() {
            self.expect_restart.store(true, Ordering::SeqCst);
            let _ = handle.kill();
        }
    }

    /// Best-effort graceful shutdown on app exit: stop supervising, drop the
    /// child's stdin (EOF, which a real core treats as "shell gone, exit";
    /// contracts/ipc.md section 8c), and terminate if still present.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.channel.set_writer(None);
        if let Some(handle) = self.current.lock().unwrap().as_mut() {
            let _ = handle.kill();
        }
        self.desired_cv.notify_all();
    }

    // -- internals ----------------------------------------------------------

    fn desired(&self) -> DesiredState {
        *self.desired.lock().unwrap()
    }

    fn set_desired(&self, state: DesiredState) {
        *self.desired.lock().unwrap() = state;
        self.desired_cv.notify_all();
    }

    fn emit_status(&self) {
        let snapshot = self.status.lock().unwrap().clone();
        (self.on_status)(&snapshot);
    }

    fn supervise(self: Arc<Self>) {
        let mut backoff = self.config.backoff_initial;
        loop {
            self.wait_until_running_or_shutdown();
            if self.shutdown.load(Ordering::SeqCst) {
                return;
            }

            let proc = match self.spawner.lock().unwrap().spawn() {
                Ok(proc) => proc,
                Err(e) => {
                    self.set_disconnected(format!("could not start the core: {e}"), false);
                    self.sleep_backoff(&mut backoff);
                    continue;
                }
            };

            let CoreProcess {
                stdin,
                stdout,
                handle,
            } = proc;
            self.channel.set_writer(Some(stdin));
            *self.current.lock().unwrap() = Some(handle);

            // A reader thread per process. It ends when the child's stdout hits
            // EOF (the process is gone).
            let (ready_tx, ready_rx) = mpsc::channel::<()>();
            let reader_self = self.clone();
            let reader = thread::Builder::new()
                .name("operant-core-reader".to_string())
                .spawn(move || {
                    let channel = reader_self.channel.clone();
                    let on_evt = reader_self.on_evt.clone();
                    let mut stdout = stdout;
                    let _ = read_loop(
                        &mut *stdout,
                        &channel,
                        move || {
                            let _ = ready_tx.send(());
                        },
                        move |evt| (on_evt)(evt),
                    );
                })
                .expect("reader thread spawns");

            match self.handshake(&ready_rx) {
                Ok(caps) => {
                    self.set_connected(caps);
                    backoff = self.config.backoff_initial;
                }
                Err(e) => {
                    // A core that came up but did not complete the handshake is
                    // no use. Terminate it, back off, and retry. No user toast
                    // for this: it never became a working core to lose.
                    if let Some(handle) = self.current.lock().unwrap().as_mut() {
                        let _ = handle.kill();
                    }
                    self.channel.fail_all();
                    self.channel.set_writer(None);
                    *self.current.lock().unwrap() = None;
                    let _ = reader.join();
                    self.set_disconnected(e, false);
                    self.sleep_backoff(&mut backoff);
                    continue;
                }
            }

            // Healthy. Watch until the child dies or an intentional stop.
            let exit_code = self.watch_until_death();

            // Death handling. Dropping the writer EOFs the child's stdin, which
            // in turn ends the reader loop; join it so no thread leaks.
            self.channel.fail_all();
            self.channel.set_writer(None);
            *self.current.lock().unwrap() = None;
            let _ = reader.join();

            let intentional = self.desired() == DesiredState::Stopped
                || self.shutdown.load(Ordering::SeqCst);
            self.after_death(exit_code, intentional);
        }
    }

    /// Block while the shell wants the core stopped, waking on a desired-state
    /// change or shutdown.
    fn wait_until_running_or_shutdown(&self) {
        let mut desired = self.desired.lock().unwrap();
        while *desired == DesiredState::Stopped && !self.shutdown.load(Ordering::SeqCst) {
            desired = self.desired_cv.wait(desired).unwrap();
        }
    }

    /// Wait for `ready`, then send `get_capabilities` and parse the result
    /// (contracts/ipc.md section 3). Returns the capabilities or a reason.
    fn handshake(&self, ready_rx: &mpsc::Receiver<()>) -> Result<Capabilities, String> {
        match ready_rx.recv_timeout(self.config.ready_timeout) {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => {
                return Err("core did not signal ready in time".to_string())
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("core exited before signalling ready".to_string())
            }
        }
        let result = self
            .channel
            .call(
                "get_capabilities",
                Value::Object(Default::default()),
                self.config.handshake_timeout,
            )
            .map_err(|e| format!("get_capabilities failed: {e}"))?;
        serde_json::from_value::<Capabilities>(result)
            .map_err(|e| format!("core sent malformed capabilities: {e}"))
    }

    /// Poll child liveness until it exits, an intentional stop is requested, or
    /// shutdown. Returns the exit code (or a synthetic one for a lost handle).
    fn watch_until_death(&self) -> i32 {
        loop {
            if self.shutdown.load(Ordering::SeqCst) {
                return 0;
            }
            {
                let mut guard = self.current.lock().unwrap();
                match guard.as_mut() {
                    Some(handle) => match handle.try_wait() {
                        Ok(Some(code)) => return code,
                        Ok(None) => {}
                        Err(_) => return -1,
                    },
                    None => return -1,
                }
            }
            // An intentional stop (kill) sets the exit flag, so try_wait catches
            // it within one interval; no need to also block on the condvar here.
            thread::sleep(self.config.poll_interval);
        }
    }

    fn after_death(&self, exit_code: i32, intentional: bool) {
        if intentional {
            {
                let mut status = self.status.lock().unwrap();
                status.connected = false;
                status.core_ready = false;
                status.desired = "stopped".to_string();
            }
            self.emit_status();
            return;
        }

        // A crash: the core was running and we did not ask it to stop.
        let was_connected = {
            let mut status = self.status.lock().unwrap();
            let was = status.connected;
            status.connected = false;
            status.core_ready = false;
            status.restarts += 1;
            status.last_error = Some(format!("core exited unexpectedly (code {exit_code})"));
            was
        };
        self.emit_status();

        // Surface a user-visible toast, but only when a working core was
        // actually lost, and not for a restart we ourselves asked for.
        let expected = self.expect_restart.swap(false, Ordering::SeqCst);
        if was_connected && !expected {
            (self.on_toast)("The automation core stopped and is being restarted.");
        }
    }

    fn set_connected(&self, caps: Capabilities) {
        let ready = caps.can_automate();
        {
            let mut status = self.status.lock().unwrap();
            status.connected = true;
            status.core_ready = ready;
            status.desired = "running".to_string();
            status.capabilities = Some(caps);
            status.last_error = None;
        }
        self.emit_status();
    }

    fn set_disconnected(&self, reason: String, connected_before: bool) {
        {
            let mut status = self.status.lock().unwrap();
            status.connected = false;
            status.core_ready = false;
            status.last_error = Some(reason);
            if connected_before {
                status.restarts += 1;
            }
        }
        self.emit_status();
    }

    fn sleep_backoff(&self, backoff: &mut Duration) {
        thread::sleep(*backoff);
        *backoff = (*backoff * 2).min(self.config.backoff_max);
    }
}

// ---------------------------------------------------------------------------
// The real spawner: `operant serve` as a child process.
// ---------------------------------------------------------------------------

/// Spawns the real core binary as `operant serve`, piping stdin/stdout for the
/// NDJSON protocol and inheriting stderr (human logs; never parsed as protocol,
/// per contracts/ipc.md section 0).
pub struct RealCoreSpawner {
    bin: std::path::PathBuf,
}

impl RealCoreSpawner {
    pub fn new(bin: std::path::PathBuf) -> Self {
        Self { bin }
    }
}

impl CoreSpawner for RealCoreSpawner {
    fn spawn(&mut self) -> std::io::Result<CoreProcess> {
        let mut child = Command::new(&self.bin)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("core child has no stdin pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("core child has no stdout pipe"))?;

        Ok(CoreProcess {
            stdin: Box::new(stdin),
            stdout: Box::new(std::io::BufReader::new(stdout)),
            handle: Box::new(RealProcessHandle { child }),
        })
    }
}

struct RealProcessHandle {
    child: std::process::Child,
}

impl ProcessHandle for RealProcessHandle {
    fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
        Ok(self.child.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }

    fn kill(&mut self) -> std::io::Result<()> {
        // On Windows this is TerminateProcess: a wedged core cannot refuse it.
        match self.child.kill() {
            Ok(()) => {}
            // Already exited: not an error for our purposes.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {}
            Err(e) => return Err(e),
        }
        let _ = self.child.wait();
        Ok(())
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::protocol::{parse_inbound, InboundFrame};
    use std::io::BufReader;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU32;
    use std::sync::mpsc::Receiver;

    fn ipc_fixture(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/ipc")
            .join(rel)
    }

    /// One command's reply in the recorded session: the res line, and the evt
    /// lines that follow before the next command.
    #[derive(Clone)]
    struct Exchange {
        cmd: String,
        res_line: String,
        evt_lines: Vec<String>,
    }

    fn load_exchanges() -> (String, Vec<Exchange>) {
        let text =
            std::fs::read_to_string(ipc_fixture("session-explore-compile-replay-undo.jsonl"))
                .unwrap();
        let mut ready_line = String::new();
        let mut exchanges: Vec<Exchange> = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match parse_inbound(line).unwrap() {
                InboundFrame::Ready { .. } => ready_line = line.to_string(),
                InboundFrame::Req { cmd, .. } => exchanges.push(Exchange {
                    cmd,
                    res_line: String::new(),
                    evt_lines: Vec::new(),
                }),
                InboundFrame::Res(_) => {
                    exchanges
                        .last_mut()
                        .expect("a res follows a req")
                        .res_line = line.to_string();
                }
                InboundFrame::Evt(_) => {
                    exchanges
                        .last_mut()
                        .expect("an evt follows a command")
                        .evt_lines
                        .push(line.to_string());
                }
            }
        }
        (ready_line, exchanges)
    }

    /// Shared control the test uses to observe and crash the current fake core.
    #[derive(Default)]
    struct FakeCtrl {
        spawn_count: AtomicU32,
        current_exit: Mutex<Option<Arc<Mutex<Option<i32>>>>>,
    }

    impl FakeCtrl {
        fn crash(&self, code: i32) {
            if let Some(exit) = self.current_exit.lock().unwrap().as_ref() {
                *exit.lock().unwrap() = Some(code);
            }
        }
        fn spawns(&self) -> u32 {
            self.spawn_count.load(Ordering::SeqCst)
        }
    }

    /// A fake core that replays the frozen fixtures. On spawn it wires two OS
    /// pipes, emits `ready`, then answers each incoming `req` with that
    /// command's recorded res (id rewritten to the caller's id) and its evts.
    /// This drives the whole supervisor state machine with real contract bytes
    /// and real correlation, with no core process and no Tauri.
    struct FakeCoreSpawner {
        ready_line: String,
        exchanges: Arc<Vec<Exchange>>,
        ctrl: Arc<FakeCtrl>,
    }

    impl CoreSpawner for FakeCoreSpawner {
        fn spawn(&mut self) -> std::io::Result<CoreProcess> {
            let (core_in_r, shell_in_w) = std::io::pipe()?;
            let (shell_out_r, core_out_w) = std::io::pipe()?;

            let exit = Arc::new(Mutex::new(None));
            *self.ctrl.current_exit.lock().unwrap() = Some(exit.clone());
            self.ctrl.spawn_count.fetch_add(1, Ordering::SeqCst);

            let ready = self.ready_line.clone();
            let exchanges = self.exchanges.clone();
            thread::spawn(move || fake_core_main(core_in_r, core_out_w, ready, exchanges));

            Ok(CoreProcess {
                stdin: Box::new(shell_in_w),
                stdout: Box::new(BufReader::new(shell_out_r)),
                handle: Box::new(FakeHandle { exit, pid: 4242 }),
            })
        }
    }

    fn fake_core_main(
        stdin: std::io::PipeReader,
        mut stdout: std::io::PipeWriter,
        ready_line: String,
        exchanges: Arc<Vec<Exchange>>,
    ) {
        let _ = writeln!(stdout, "{}", ready_line.trim());
        let _ = stdout.flush();

        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break, // EOF: shell dropped our stdin.
                Ok(_) => {}
            }
            if line.trim().is_empty() {
                continue;
            }
            let req: Value = match serde_json::from_str(line.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let id = req["id"].as_str().unwrap_or("").to_string();
            let cmd = req["cmd"].as_str().unwrap_or("").to_string();

            match exchanges.iter().find(|e| e.cmd == cmd) {
                Some(exchange) => {
                    let mut res: Value = serde_json::from_str(&exchange.res_line).unwrap();
                    res["id"] = Value::String(id);
                    let _ = writeln!(stdout, "{}", serde_json::to_string(&res).unwrap());
                    for evt in &exchange.evt_lines {
                        let _ = writeln!(stdout, "{evt}");
                    }
                    let _ = stdout.flush();
                }
                None => {
                    let res = serde_json::json!({
                        "t": "res", "pv": 1, "id": id, "ok": false,
                        "error": {"code": "unknown_command", "message": "no script", "retryable": false}
                    });
                    let _ = writeln!(stdout, "{}", serde_json::to_string(&res).unwrap());
                    let _ = stdout.flush();
                }
            }
        }
    }

    struct FakeHandle {
        exit: Arc<Mutex<Option<i32>>>,
        pid: u32,
    }

    impl ProcessHandle for FakeHandle {
        fn try_wait(&mut self) -> std::io::Result<Option<i32>> {
            Ok(*self.exit.lock().unwrap())
        }
        fn kill(&mut self) -> std::io::Result<()> {
            *self.exit.lock().unwrap() = Some(137);
            Ok(())
        }
        fn pid(&self) -> u32 {
            self.pid
        }
    }

    /// Fast config so the state machine runs in milliseconds.
    fn fast_config() -> SupervisorConfig {
        SupervisorConfig {
            poll_interval: Duration::from_millis(5),
            ready_timeout: Duration::from_secs(2),
            handshake_timeout: Duration::from_secs(2),
            call_timeout: Duration::from_secs(2),
            backoff_initial: Duration::from_millis(5),
            backoff_max: Duration::from_millis(50),
        }
    }

    struct Harness {
        supervisor: Arc<Supervisor>,
        ctrl: Arc<FakeCtrl>,
        status_rx: Receiver<CoreStatus>,
        toast_rx: Receiver<String>,
        evts: Arc<Mutex<Vec<super::super::protocol::EvtFrame>>>,
    }

    fn build_harness() -> Harness {
        let (ready_line, exchanges) = load_exchanges();
        let ctrl = Arc::new(FakeCtrl::default());
        let spawner = FakeCoreSpawner {
            ready_line,
            exchanges: Arc::new(exchanges),
            ctrl: ctrl.clone(),
        };

        let (status_tx, status_rx) = mpsc::channel();
        let (toast_tx, toast_rx) = mpsc::channel();
        let evts = Arc::new(Mutex::new(Vec::new()));

        let evts_sink = evts.clone();
        let on_evt: EvtSink = Arc::new(move |evt| evts_sink.lock().unwrap().push(evt));
        let on_status: StatusSink = Arc::new(move |s| {
            let _ = status_tx.send(s.clone());
        });
        let on_toast: ToastSink = Arc::new(move |m| {
            let _ = toast_tx.send(m.to_string());
        });

        let supervisor = Arc::new(Supervisor::new(
            Box::new(spawner),
            fast_config(),
            on_evt,
            on_status,
            on_toast,
        ));
        supervisor.start();

        Harness {
            supervisor,
            ctrl,
            status_rx,
            toast_rx,
            evts,
        }
    }

    /// Wait for a status change that satisfies `pred`, draining intermediate
    /// ones. Fails the test if none arrives in time.
    fn wait_for_status(
        rx: &Receiver<CoreStatus>,
        pred: impl Fn(&CoreStatus) -> bool,
    ) -> CoreStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(remaining > Duration::ZERO, "timed out waiting for status");
            match rx.recv_timeout(remaining) {
                Ok(status) if pred(&status) => return status,
                Ok(_) => continue,
                Err(_) => panic!("status channel closed before the expected state"),
            }
        }
    }

    #[test]
    fn handshake_gates_the_blocking_case_from_the_fixture() {
        let h = build_harness();
        let status = wait_for_status(&h.status_rx, |s| s.connected);
        // The fixture is a mock recorder build: connected, but not able to
        // automate, so B3 shows the blocking screen.
        assert!(status.connected);
        assert!(!status.core_ready, "mock caps must not report ready");
        let caps = status.capabilities.expect("capabilities after handshake");
        assert!(!caps.real_uia);
        assert!(!caps.real_input);
        assert_eq!(caps.transport_kind, "stdio");
        h.supervisor.shutdown();
    }

    #[test]
    fn core_call_proxies_and_evts_are_forwarded() {
        let h = build_harness();
        wait_for_status(&h.status_rx, |s| s.connected);

        // Drive the real command surface: start_explore should return the
        // recorded run_id, and its whole bus event stream should reach the
        // evt sink (which the shell wires onto operant://bus).
        let result = h
            .supervisor
            .call("start_explore", serde_json::json!({"goal": "x", "window_process": "notepad.exe"}))
            .expect("start_explore resolves");
        assert_eq!(result["run_id"], "run_0");

        // Give the evt frames a moment to flow through the reader thread.
        let deadline = Instant::now() + Duration::from_secs(2);
        while h.evts.lock().unwrap().len() < 11 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let evts = h.evts.lock().unwrap();
        let topics: Vec<String> = evts
            .iter()
            .map(|e| e.env["topic"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(topics.contains(&"run.started".to_string()));
        assert!(topics.contains(&"run.completed".to_string()));
        drop(evts);
        h.supervisor.shutdown();
    }

    #[test]
    fn a_crash_restarts_and_rehandshakes_with_a_toast() {
        let h = build_harness();
        wait_for_status(&h.status_rx, |s| s.connected);
        assert_eq!(h.ctrl.spawns(), 1);

        // Simulate the core dying underneath the watchdog.
        h.ctrl.crash(1);

        // It must come back: a second connected status, a second spawn, and a
        // user-visible toast for the restart.
        let restarted = wait_for_status(&h.status_rx, |s| s.connected && s.restarts >= 1);
        assert!(restarted.connected);
        assert_eq!(h.ctrl.spawns(), 2, "the watchdog respawned the core");

        let toast = h.toast_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(toast.contains("restarted"), "the restart surfaced a toast: {toast}");
        h.supervisor.shutdown();
    }

    #[test]
    fn kill_is_fast_and_does_not_restart() {
        let h = build_harness();
        wait_for_status(&h.status_rx, |s| s.connected);
        assert_eq!(h.ctrl.spawns(), 1);

        let report = h.supervisor.kill().unwrap();
        assert!(report.killed, "the child was confirmed terminated");
        assert!(
            report.elapsed_ms < 200,
            "the panic path terminates fast, took {}ms",
            report.elapsed_ms
        );

        // A hard kill is a stop: the core stays down, no restart.
        wait_for_status(&h.status_rx, |s| !s.connected && s.desired == "stopped");
        thread::sleep(Duration::from_millis(100));
        assert_eq!(h.ctrl.spawns(), 1, "an intentional kill must not restart");
        h.supervisor.shutdown();
    }

    #[test]
    fn restart_after_kill_brings_the_core_back() {
        let h = build_harness();
        wait_for_status(&h.status_rx, |s| s.connected);
        h.supervisor.kill().unwrap();
        wait_for_status(&h.status_rx, |s| s.desired == "stopped");

        // The webview's recover action.
        h.supervisor.request_restart();
        wait_for_status(&h.status_rx, |s| s.connected);
        assert_eq!(h.ctrl.spawns(), 2, "restart respawned the core");
        h.supervisor.shutdown();
    }
}
