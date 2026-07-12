//! `agent_bridge`: a filesystem [`ModelBackend`] that hands each planner turn
//! to an external operator (a human, or an agent such as Claude driving the
//! desktop) instead of a model endpoint.
//!
//! This is the P0b proof harness backend: it lets an operator act as the
//! planner "brain" for a REAL explore loop without any network transport. It
//! is compiled ONLY behind the opt-in `dev-agent-bridge` cargo feature and is
//! never part of a default or release build (a release must never wait on a
//! human at a filesystem rendezvous).
//!
//! # Protocol (see `docs/evidence/agent-bridge-protocol.md`)
//! Each `complete()` call is one numbered round `N` (a per-instance counter
//! starting at 1) that rendezvouses through a directory named by the
//! `OPERANT_AGENT_BRIDGE_DIR` environment variable:
//!
//! 1. The backend writes `<dir>/req-<N>.json` atomically (temp file + rename):
//!    `{"seq": N, "prompt": "<the request's concat_text()>"}`.
//! 2. The backend prints one line to STDOUT and flushes it: `AGENT_BRIDGE_AWAIT <N>`.
//! 3. The operator reads `req-<N>.json`, decides the next planner move, and
//!    writes `<dir>/resp-<N>.json`: a JSON ARRAY of [`BackendEvent`] objects
//!    (the same wire shape the explore loop already consumes). The operator
//!    MUST write it atomically (temp file + rename) so the backend never sees
//!    a half-written file.
//! 4. The backend polls every 500ms for `resp-<N>.json`; once it parses as
//!    `Vec<BackendEvent>` it returns those events as the stream, appending a
//!    terminal `Done` if the operator's array did not already end in one
//!    (mirroring [`super::MockPlannerBackend`]).
//!
//! If no response arrives within 20 minutes the round returns a single
//! terminal `Error { error_id: "agent_bridge_timeout", .. }`. A response file
//! that is present but does not parse returns `Error { error_id:
//! "agent_bridge_bad_response", .. }` so the operator sees the mistake instead
//! of silently timing out.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream};
use futures::{FutureExt, StreamExt};

use super::probe::{now_rfc3339, DEFAULT_CONTEXT_LENGTH};
use super::{BackendError, BackendEvent, BackendProfile, CompletionRequest, ModelBackend, Usage};

/// Environment variable naming the rendezvous directory.
pub const AGENT_BRIDGE_DIR_ENV: &str = "OPERANT_AGENT_BRIDGE_DIR";

/// How often the backend re-checks for the operator's response file.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// How long one round waits for a response before giving up.
const ROUND_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// A [`ModelBackend`] that routes each planner turn through a directory of
/// request/response JSON files so an external operator can answer it by hand.
/// See the module docs for the exact protocol.
#[derive(Debug)]
pub struct AgentBridgeBackend {
    dir: PathBuf,
    /// Per-instance round counter. Starts at 1; each `complete()` claims the
    /// next value and never reuses one.
    round: AtomicU64,
}

impl AgentBridgeBackend {
    /// Build a bridge rendezvousing through `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            round: AtomicU64::new(1),
        }
    }

    /// Build a bridge whose directory comes from the `OPERANT_AGENT_BRIDGE_DIR`
    /// environment variable. Fails (never retryable) when the variable is
    /// unset, since the bridge has nowhere to rendezvous.
    pub fn from_env() -> Result<Self, BackendError> {
        match std::env::var(AGENT_BRIDGE_DIR_ENV) {
            Ok(dir) if !dir.is_empty() => Ok(Self::new(dir)),
            _ => Err(BackendError::config(format!(
                "{AGENT_BRIDGE_DIR_ENV} must be set to the agent-bridge rendezvous directory"
            ))),
        }
    }

    /// The rendezvous directory this bridge writes into.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Serialize the request file body. Kept as an explicit shape so the
/// protocol doc and this code cannot drift.
fn request_json(seq: u64, prompt: &str) -> String {
    serde_json::json!({ "seq": seq, "prompt": prompt }).to_string()
}

/// Atomically write `contents` to `path` (temp sibling file + rename), so a
/// reader polling `path` never observes a partially written file.
fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Run one full rendezvous round and return the events to stream. Never
/// returns an empty vector: on timeout or a malformed response it returns a
/// single terminal `Error`.
async fn run_round(dir: PathBuf, seq: u64, prompt: String) -> Vec<BackendEvent> {
    let req_path = dir.join(format!("req-{seq}.json"));
    let resp_path = dir.join(format!("resp-{seq}.json"));

    if let Err(e) = atomic_write(&req_path, &request_json(seq, &prompt)) {
        return vec![BackendEvent::Error {
            error_id: "agent_bridge_write_failed".to_string(),
            message: format!("could not write {}: {e}", req_path.display()),
            retryable: false,
        }];
    }

    // Signal the operator that round `seq` is waiting. One line, flushed, so a
    // process watching our stdout sees it immediately.
    {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "AGENT_BRIDGE_AWAIT {seq}");
        let _ = out.flush();
    }

    let deadline = std::time::Instant::now() + ROUND_TIMEOUT;
    loop {
        if resp_path.exists() {
            match std::fs::read_to_string(&resp_path) {
                // Tolerate a leading UTF-8 BOM: Windows editors and
                // `Set-Content -Encoding utf8` on Windows PowerShell emit one,
                // and serde_json rejects it otherwise.
                Ok(raw) => match serde_json::from_str::<Vec<BackendEvent>>(
                    raw.trim_start_matches('\u{feff}'),
                ) {
                    Ok(mut events) => {
                        if !events.iter().any(BackendEvent::is_terminal) {
                            events.push(BackendEvent::Done {
                                usage: Usage::default(),
                            });
                        }
                        return events;
                    }
                    Err(e) => {
                        return vec![BackendEvent::Error {
                            error_id: "agent_bridge_bad_response".to_string(),
                            message: format!(
                                "{} did not parse as a JSON array of BackendEvent: {e}",
                                resp_path.display()
                            ),
                            retryable: false,
                        }];
                    }
                },
                // A transient read error (e.g. the operator's rename is mid
                // flight): keep polling until the deadline.
                Err(_) => {}
            }
        }
        if std::time::Instant::now() >= deadline {
            return vec![BackendEvent::Error {
                error_id: "agent_bridge_timeout".to_string(),
                message: format!(
                    "no {} within {}s for agent-bridge round {seq}",
                    resp_path.display(),
                    ROUND_TIMEOUT.as_secs()
                ),
                retryable: false,
            }];
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

impl ModelBackend for AgentBridgeBackend {
    fn complete(&self, request: CompletionRequest) -> BoxStream<'static, BackendEvent> {
        // Claim this round's number synchronously so concurrent calls never
        // collide on a file name, then do the (async) file rendezvous.
        let seq = self.round.fetch_add(1, Ordering::SeqCst);
        let dir = self.dir.clone();
        let prompt = request.concat_text();
        stream::once(run_round(dir, seq, prompt))
            .flat_map(stream::iter)
            .boxed()
    }

    fn probe(&self) -> BoxFuture<'static, Result<BackendProfile, BackendError>> {
        let profile = BackendProfile {
            backend_id: "agent_bridge".to_string(),
            vision: false,
            tool_use: true,
            context_length: DEFAULT_CONTEXT_LENGTH,
            streaming: false,
            probed_at: now_rfc3339(),
        };
        async move { Ok(profile) }.boxed()
    }

    fn id(&self) -> &str {
        "agent_bridge"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn request_json_has_seq_and_prompt() {
        let v: serde_json::Value = serde_json::from_str(&request_json(7, "Goal: x")).unwrap();
        assert_eq!(v["seq"], 7);
        assert_eq!(v["prompt"], "Goal: x");
    }

    #[test]
    fn from_env_is_config_error_when_unset() {
        // Guard against a stray value in the ambient environment.
        std::env::remove_var(AGENT_BRIDGE_DIR_ENV);
        let err = AgentBridgeBackend::from_env().unwrap_err();
        assert_eq!(err.error_id, "config_error");
        assert!(!err.retryable);
    }

    #[tokio::test]
    async fn round_returns_operator_events_and_appends_done() {
        let dir = std::env::temp_dir().join(format!(
            "operant-agent-bridge-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let backend = AgentBridgeBackend::new(&dir);

        // Operator's answer is available before the round starts, so the very
        // first poll finds it: a click proposal followed by `done`.
        let resp = r#"[
            {"event":"tool_call","id":"1","name":"propose_action","arguments":{"id":"s1","kind":"key","params":{"combo":"ctrl+s"},"risk_class":"write","grounding":"uia"}},
            {"event":"tool_call","id":"2","name":"done","arguments":{}}
        ]"#;
        std::fs::write(dir.join("resp-1.json"), resp).unwrap();

        let req = CompletionRequest::text(super::super::RequestRole::Planner, "Goal: save", 64);
        let events: Vec<BackendEvent> = backend.complete(req).collect().await;

        // The request file was written with the claimed round number.
        let written = std::fs::read_to_string(dir.join("req-1.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(parsed["seq"], 1);
        assert_eq!(parsed["prompt"], "Goal: save");

        // Two operator tool calls, plus the appended terminal Done.
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], BackendEvent::ToolCall { .. }));
        assert!(events.last().unwrap().is_terminal());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
