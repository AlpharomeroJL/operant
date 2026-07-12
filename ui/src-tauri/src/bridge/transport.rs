// Framing and correlation over the sidecar's stdio, independent of any process
// or Tauri handle so it is testable against in-memory pipes.
//
// [`Channel`] owns the write half (the child's stdin) and the table of
// outstanding requests. A single reader loop ([`read_loop`]) drains the child's
// stdout, routing each `res` to the caller that is blocked in [`Channel::call`]
// by correlation id (contracts/ipc.md section 2c) and handing each `evt` to a
// sink the shell wires onto the webview (section 2d and 6). `ready` fires a
// one-shot signal the supervisor waits on before the handshake (section 3).

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use super::protocol::{encode_req_line, parse_inbound, CoreError, EvtFrame, InboundFrame, ResFrame};

/// The control plane and the write half of one bridge. Requests are correlated
/// by a shell-generated id; the reader loop delivers the matching `res` back
/// through the one-shot channel registered here.
///
/// The write half is swappable: on a core restart the supervisor installs the
/// new child's stdin with [`Channel::set_writer`], and clears it (to `None`) on
/// death so an in-flight or new [`Channel::call`] fails fast with
/// `core_unavailable` instead of blocking on a dead pipe.
pub struct Channel {
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    pending: Mutex<HashMap<String, mpsc::Sender<ResFrame>>>,
    next_id: AtomicU64,
}

impl Default for Channel {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel {
    pub fn new() -> Self {
        Self {
            writer: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
        }
    }

    /// Install (or clear) the current child's stdin. Clearing it wakes nothing
    /// on its own; callers pair this with [`Channel::fail_all`] on a death so
    /// blocked callers are released.
    pub fn set_writer(&self, writer: Option<Box<dyn Write + Send>>) {
        *self.writer.lock().unwrap() = writer;
    }

    /// Deliver a `res` to whichever caller is waiting on its id. A `res` whose
    /// id is unknown (the caller already timed out and unregistered) is dropped.
    pub fn deliver(&self, res: ResFrame) {
        let sender = self.pending.lock().unwrap().remove(&res.id);
        if let Some(sender) = sender {
            // The receiver may have been dropped on a timeout race; ignore.
            let _ = sender.send(res);
        }
    }

    /// Release every outstanding caller (the pipe died). Dropping the senders
    /// turns each blocked `recv_timeout` into a `Disconnected`, which
    /// [`Channel::call`] maps to `core_unavailable`.
    pub fn fail_all(&self) {
        self.pending.lock().unwrap().clear();
    }

    fn next_id(&self) -> String {
        // A monotonic per-process counter is enough: the id only has to be
        // unique among outstanding requests, and the core echoes it verbatim.
        // Avoids pulling in a uuid dependency for an opaque token.
        format!("req-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Send one `req` and block until its `res` arrives or the deadline passes.
    /// This is the primitive behind both the startup handshake and the
    /// `core_call` command. It never blocks the Tauri main thread: the command
    /// layer runs it on a blocking task.
    pub fn call(&self, cmd: &str, args: Value, timeout: Duration) -> Result<Value, CoreError> {
        let id = self.next_id();
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id.clone(), tx);

        // Write the frame while holding the writer lock only for the write.
        let line = encode_req_line(&id, cmd, &args);
        {
            let mut guard = self.writer.lock().unwrap();
            match guard.as_mut() {
                Some(writer) => {
                    let w = &mut **writer;
                    if let Err(e) = w.write_all(line.as_bytes()).and_then(|_| w.flush()) {
                        drop(guard);
                        self.pending.lock().unwrap().remove(&id);
                        return Err(CoreError::unavailable(format!("write to core failed: {e}")));
                    }
                }
                None => {
                    drop(guard);
                    self.pending.lock().unwrap().remove(&id);
                    return Err(CoreError::unavailable("the automation core is not running"));
                }
            }
        }

        match rx.recv_timeout(timeout) {
            Ok(res) => {
                if res.ok {
                    Ok(res.result.unwrap_or(Value::Null))
                } else {
                    match res.error {
                        Some(body) => Err(CoreError::from_body(body)),
                        None => Err(CoreError::internal("core reported not-ok with no error body")),
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                self.pending.lock().unwrap().remove(&id);
                Err(CoreError::timeout(format!("core did not answer '{cmd}' in time")))
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err(CoreError::unavailable("the automation core stopped before answering"))
            }
        }
    }

    /// Best-effort, never-blocking fire of a command whose reply we do not
    /// await. Used by the kill path as a courtesy nudge for the core's own
    /// in-process freeze (contracts/ipc.md section 5b, kill path 1): if the
    /// writer lock is contended or gone, we skip it and rely on the hard
    /// terminate (path 2). It must never be able to delay a stop.
    pub fn try_fire_and_forget(&self, cmd: &str) -> bool {
        let id = self.next_id();
        let line = encode_req_line(&id, cmd, &Value::Object(Default::default()));
        if let Ok(mut guard) = self.writer.try_lock() {
            if let Some(writer) = guard.as_mut() {
                let w = &mut **writer;
                return w.write_all(line.as_bytes()).and_then(|_| w.flush()).is_ok();
            }
        }
        false
    }
}

/// Drain the core's stdout to end of stream, dispatching each frame. Returns
/// `Ok(())` on a clean EOF (the child closed its stdout, which for stdio means
/// the process is gone) and an error only on an unexpected read failure.
///
/// A line that does not parse is a protocol error: it is logged to stderr and
/// skipped, and the stream continues at the next newline. A malformed line
/// never wedges the reader (contracts/ipc.md section 1).
pub fn read_loop(
    reader: &mut dyn BufRead,
    channel: &Channel,
    mut on_ready: impl FnMut(),
    on_evt: impl Fn(EvtFrame),
) -> std::io::Result<()> {
    let mut buf = String::new();
    loop {
        buf.clear();
        // read_line reads through the next '\n'. The core bounds each frame to
        // 8 MiB (contracts/ipc.md section 1); the shell trusts its own child.
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            return Ok(()); // EOF: the pipe, and thus the process, is gone.
        }
        if buf.trim().is_empty() {
            continue;
        }
        match parse_inbound(&buf) {
            Ok(InboundFrame::Ready { pv }) => {
                // contracts/ipc.md section 2: ready tells the shell which
                // protocol version the core speaks. A mismatch is not fatal
                // (additive changes never bump pv, section 9), but it is worth
                // surfacing in logs.
                if pv != super::protocol::PROTOCOL_VERSION {
                    eprintln!(
                        "operant-shell: core speaks protocol pv={pv}, shell speaks pv={}",
                        super::protocol::PROTOCOL_VERSION
                    );
                }
                on_ready();
            }
            Ok(InboundFrame::Res(res)) => channel.deliver(res),
            Ok(InboundFrame::Evt(evt)) => on_evt(evt),
            Ok(InboundFrame::Req { cmd, .. }) => {
                // The shell never receives a core-to-shell req in this contract.
                eprintln!("operant-shell: ignoring unexpected req frame from core (cmd={cmd})");
            }
            Err(e) => {
                eprintln!("operant-shell: skipping unparseable frame from core: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Read};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;

    fn ipc_fixture(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/ipc")
            .join(rel)
    }

    /// The explore exchange from the recorded session: the res line and the
    /// evt lines that follow it, before the next command.
    fn explore_exchange() -> (String, Vec<String>) {
        let text = std::fs::read_to_string(ipc_fixture("session-explore-compile-replay-undo.jsonl"))
            .unwrap();
        let mut res_line = None;
        let mut evts = Vec::new();
        for line in text.lines() {
            match parse_inbound(line) {
                Ok(InboundFrame::Res(r)) if r.id == "cmd-2" => res_line = Some(line.to_string()),
                Ok(InboundFrame::Evt(_)) if res_line.is_some() && evts.len() < 11 => {
                    evts.push(line.to_string())
                }
                // The next command after start_explore (compile_run) ends the
                // exchange.
                Ok(InboundFrame::Req { cmd }) if cmd == "compile_run" => break,
                _ => {}
            }
        }
        (res_line.expect("start_explore res is in the fixture"), evts)
    }

    #[test]
    fn call_correlates_res_by_id_and_forwards_evts() {
        // Wire the two halves with real OS pipes: the shell writes reqs into
        // one and reads frames out of the other. A tiny fake core sits between
        // them and replies with the fixture's real bytes, rewriting only the
        // correlation id to the one the shell actually generated. This exercises
        // real framing (fixture bytes), real correlation (a runtime id), and
        // evt forwarding, with no core process.
        let (core_in_r, shell_in_w) = std::io::pipe().unwrap();
        let (shell_out_r, core_out_w) = std::io::pipe().unwrap();

        let channel = Arc::new(Channel::new());
        channel.set_writer(Some(Box::new(shell_in_w)));

        let seen_evts = Arc::new(Mutex::new(Vec::<EvtFrame>::new()));

        // Reader loop over the core -> shell pipe.
        let reader_channel = channel.clone();
        let reader_evts = seen_evts.clone();
        let reader = thread::spawn(move || {
            let mut buffered = BufReader::new(shell_out_r);
            read_loop(
                &mut buffered,
                &reader_channel,
                || {},
                move |evt| reader_evts.lock().unwrap().push(evt),
            )
        });

        // The fake core: read one req, reply with the explore res (id rewritten)
        // plus its evts.
        let (res_line, evt_lines) = explore_exchange();
        let core = thread::spawn(move || {
            let mut buffered = BufReader::new(core_in_r);
            let mut line = String::new();
            buffered.read_line(&mut line).unwrap();
            let req: Value = serde_json::from_str(line.trim()).unwrap();
            let incoming_id = req["id"].as_str().unwrap().to_string();
            assert_eq!(req["cmd"], "start_explore");

            let mut res: Value = serde_json::from_str(&res_line).unwrap();
            res["id"] = Value::String(incoming_id);
            let mut out = core_out_w;
            writeln!(out, "{}", serde_json::to_string(&res).unwrap()).unwrap();
            for evt in &evt_lines {
                writeln!(out, "{evt}").unwrap();
            }
            out.flush().unwrap();
            // Dropping `out` here closes the pipe, ending the reader loop.
        });

        let result = channel
            .call(
                "start_explore",
                serde_json::json!({"goal": "x", "window_process": "notepad.exe"}),
                Duration::from_secs(5),
            )
            .expect("explore call resolves");
        assert_eq!(result["run_id"], "run_0", "the correlated res result is returned");

        core.join().unwrap();
        // Release the reader by dropping the shell's write half, then join.
        channel.set_writer(None);
        reader.join().unwrap().unwrap();

        let evts = seen_evts.lock().unwrap();
        assert_eq!(evts.len(), 11, "every evt in the exchange was forwarded");
        assert_eq!(evts[0].env["topic"], "run.started");
        assert_eq!(evts.last().unwrap().env["topic"], "run.completed");
    }

    #[test]
    fn call_without_a_writer_is_unavailable_not_a_hang() {
        let channel = Channel::new();
        let err = channel
            .call("get_capabilities", Value::Null, Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(err.code, "core_unavailable");
    }

    #[test]
    fn call_times_out_when_the_core_is_silent() {
        // A writer that accepts bytes but no reader ever answers.
        let (_core_in_r, shell_in_w) = std::io::pipe().unwrap();
        let channel = Channel::new();
        channel.set_writer(Some(Box::new(shell_in_w)));
        let err = channel
            .call("get_capabilities", Value::Null, Duration::from_millis(50))
            .unwrap_err();
        assert_eq!(err.code, "core_timeout");
    }

    #[test]
    fn fail_all_releases_a_blocked_caller_as_unavailable() {
        let (mut core_in_r, shell_in_w) = std::io::pipe().unwrap();
        let channel = Arc::new(Channel::new());
        channel.set_writer(Some(Box::new(shell_in_w)));

        let waiter_channel = channel.clone();
        let waiter = thread::spawn(move || {
            waiter_channel.call("stop", Value::Null, Duration::from_secs(5))
        });

        // Ensure the req was written (so the waiter is registered and blocked)
        // before we simulate the core dying.
        let mut sink = [0u8; 64];
        let _ = core_in_r.read(&mut sink).unwrap();
        channel.fail_all();

        let err = waiter.join().unwrap().unwrap_err();
        assert_eq!(err.code, "core_unavailable");
    }

    #[test]
    fn a_failure_res_passes_the_core_error_through() {
        let (core_in_r, shell_in_w) = std::io::pipe().unwrap();
        let (shell_out_r, core_out_w) = std::io::pipe().unwrap();
        let channel = Arc::new(Channel::new());
        channel.set_writer(Some(Box::new(shell_in_w)));

        let reader_channel = channel.clone();
        let reader = thread::spawn(move || {
            let mut buffered = BufReader::new(shell_out_r);
            read_loop(&mut buffered, &reader_channel, || {}, |_| {})
        });

        let core = thread::spawn(move || {
            let mut buffered = BufReader::new(core_in_r);
            let mut line = String::new();
            buffered.read_line(&mut line).unwrap();
            let req: Value = serde_json::from_str(line.trim()).unwrap();
            let id = req["id"].as_str().unwrap();
            let mut out = core_out_w;
            let res = serde_json::json!({
                "t": "res", "pv": 1, "id": id, "ok": false,
                "error": {"code": "core_busy", "message": "a run is active", "retryable": true}
            });
            writeln!(out, "{}", serde_json::to_string(&res).unwrap()).unwrap();
            out.flush().unwrap();
        });

        let err = channel
            .call("start_explore", Value::Null, Duration::from_secs(5))
            .unwrap_err();
        assert_eq!(err.code, "core_busy");
        assert!(err.retryable);

        core.join().unwrap();
        channel.set_writer(None);
        reader.join().unwrap().unwrap();
    }
}
