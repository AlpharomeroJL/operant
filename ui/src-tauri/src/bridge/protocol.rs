// The wire protocol for the shell-to-core sidecar bridge.
//
// This module is a pure, IO-free mirror of contracts/ipc.md sections 1-3: the
// four newline-delimited JSON frame types (ready / req / res / evt), the
// capability handshake result, and the error shape. It has no knowledge of
// processes, threads, or Tauri, so every clause of the frozen contract can be
// asserted against the committed fixtures in a plain unit test (see the tests
// at the bottom, which parse every line of
// contracts/fixtures/ipc/session-explore-compile-replay-undo.jsonl and the
// handshake capture byte for byte).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The IPC protocol version this shell speaks (contracts/ipc.md section 2). It
/// is distinct from the bus envelope's own `v`.
pub const PROTOCOL_VERSION: i64 = 1;

/// One frame read from the core's stdout. Internally tagged on `t` exactly as
/// the contract frames it. `req` is included only so that parsing a recorded
/// session (which interleaves the shell's own req frames) never fails; the
/// shell never acts on a core-to-shell `req`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "t")]
pub enum InboundFrame {
    /// contracts/ipc.md 2a: the first unsolicited frame on startup.
    #[serde(rename = "ready")]
    Ready {
        #[serde(default)]
        pv: i64,
    },
    /// contracts/ipc.md 2c: the one response correlated to a req by `id`.
    #[serde(rename = "res")]
    Res(ResFrame),
    /// contracts/ipc.md 2d: an uncorrelated event carrying one bus envelope.
    #[serde(rename = "evt")]
    Evt(EvtFrame),
    /// contracts/ipc.md 2b: a request. Shell-to-core in normal operation; only
    /// parsed here so a recorded session round-trips. The shell never acts on a
    /// core-to-shell req, so only `cmd` is captured (for a diagnostic log); the
    /// rest of the frame is ignored.
    #[serde(rename = "req")]
    Req {
        #[serde(default)]
        cmd: String,
    },
}

/// contracts/ipc.md 2c. Exactly one answers each req, carrying the same `id`.
/// `pv` is ignored on the wire here (additive changes never bump it; section 9).
#[derive(Debug, Clone, Deserialize)]
pub struct ResFrame {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<CoreErrorBody>,
}

/// The fixed error shape inside a failure `res` (contracts/ipc.md 2c).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoreErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

/// contracts/ipc.md 2d. `env` is the bus `Envelope` unchanged (no translation;
/// the shell forwards it to the webview byte for byte). `thumb` is the
/// optional flight-recorder screenshot sidecar (section 7), present only on
/// run-step frames and `null` everywhere else.
#[derive(Debug, Clone, Deserialize)]
pub struct EvtFrame {
    pub env: Value,
    #[serde(default)]
    pub thumb: Option<Value>,
}

/// The capability handshake result (contracts/ipc.md section 3). The four
/// automation booleans follow the core's build cfg and are constant for a
/// process lifetime, so the shell gates on them once per core process.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Capabilities {
    #[serde(default)]
    pub real_uia: bool,
    #[serde(default)]
    pub real_input: bool,
    #[serde(default)]
    pub real_vision: bool,
    #[serde(default)]
    pub mock_planner_only: bool,
    #[serde(default)]
    pub transport_kind: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub git_sha: String,
}

impl Capabilities {
    /// The structural guarantee from contracts/ipc.md section 3: real
    /// automation requires BOTH real perception and real input, and a real
    /// (non-mock) transport. This mirrors the CLI's E4 rule where either
    /// feature alone silently degrades to mock. When this is false the shell
    /// MUST NOT expose any surface that starts a real run or a real teach; B3
    /// renders the blocking screen instead.
    pub fn can_automate(&self) -> bool {
        self.real_uia && self.real_input && self.transport_kind != "mock"
    }

    /// The capability field names, by their contract name, that are missing
    /// for automation. B3's blocking screen enumerates each one so the failure
    /// is legible rather than a generic error (contracts/ipc.md section 3).
    /// B3 mirrors this enumeration in TypeScript over the capability object it
    /// receives from `core_capabilities`; this Rust form is the tested,
    /// executable statement of the same contract rule.
    #[allow(dead_code)]
    pub fn missing_for_automation(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.real_uia {
            missing.push("real_uia");
        }
        if !self.real_input {
            missing.push("real_input");
        }
        if self.transport_kind == "mock" {
            missing.push("transport_kind");
        }
        missing
    }

    /// Teaching requires a real planner (contracts/ipc.md section 3). When this
    /// is true the teach surface must not present itself as producing a real
    /// taught workflow. Replay and real-run surfaces are NOT blocked by this
    /// alone, because replay needs no planner. B3 consumes `mock_planner_only`
    /// directly; this accessor states the rule alongside the tests.
    #[allow(dead_code)]
    pub fn planner_is_mock(&self) -> bool {
        self.mock_planner_only
    }
}

/// The error surfaced to a `core_call` caller. It carries the same shape as the
/// contract's error body so a core-originated failure passes through
/// unchanged, plus a small set of shell-originated codes for transport
/// failures the core never sees (the child is gone, or it did not answer in
/// time). Per the contract's versioning rules, consumers must tolerate
/// unknown error codes, so these shell codes are safe to add.
#[derive(Debug, Clone, Serialize)]
pub struct CoreError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl CoreError {
    /// Shell-originated: no live core to write to (never spawned, crashed, or
    /// killed). Retryable once a core is back up.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "core_unavailable".to_string(),
            message: message.into(),
            retryable: true,
        }
    }

    /// Shell-originated: the core did not answer this request within the
    /// deadline. Retryable.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: "core_timeout".to_string(),
            message: message.into(),
            retryable: true,
        }
    }

    /// Shell-originated: an unexpected shell-side error (for example a write
    /// that failed, or a malformed capability result).
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal".to_string(),
            message: message.into(),
            retryable: true,
        }
    }

    /// A core-originated failure `res`, passed through unchanged.
    pub fn from_body(body: CoreErrorBody) -> Self {
        Self {
            code: body.code,
            message: body.message,
            retryable: body.retryable,
        }
    }
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for CoreError {}

/// Strip a leading UTF-8 BOM (Windows PowerShell producers emit one;
/// contracts/ipc.md section 1 tolerates and strips it) and any trailing
/// carriage return or newline, then parse one frame. A line that does not
/// parse as a JSON object is a protocol error the caller logs and skips; a
/// malformed line never wedges the stream.
pub fn parse_inbound(line: &str) -> Result<InboundFrame, serde_json::Error> {
    let line = line.strip_prefix('\u{feff}').unwrap_or(line);
    let line = line.trim_end_matches(['\r', '\n']);
    serde_json::from_str(line)
}

/// Encode one `req` frame as a single compact line terminated by `\n`
/// (contracts/ipc.md sections 1 and 2b). `args` is the command's argument
/// object; pass `Value::Object` / `{}` when the command takes none.
pub fn encode_req_line(id: &str, cmd: &str, args: &Value) -> String {
    // Built through serde_json so escaping is correct and the object never
    // contains a literal newline that would split one logical frame.
    let frame = serde_json::json!({
        "t": "req",
        "pv": PROTOCOL_VERSION,
        "id": id,
        "cmd": cmd,
        "args": args,
    });
    let mut line = serde_json::to_string(&frame).expect("a req frame always serializes");
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ipc_fixture(rel: &str) -> PathBuf {
        // The frozen fixtures live at repo-root contracts/fixtures/ipc; this
        // crate is ui/src-tauri, two levels down. Testing against the exact
        // committed bytes is the point (contracts/fixtures/ipc/README.md).
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/fixtures/ipc")
            .join(rel)
    }

    #[test]
    fn every_session_fixture_line_parses() {
        let text = std::fs::read_to_string(ipc_fixture("session-explore-compile-replay-undo.jsonl"))
            .expect("session fixture is readable");
        let mut ready = 0;
        let mut req = 0;
        let mut res = 0;
        let mut evt = 0;
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let frame = parse_inbound(line)
                .unwrap_or_else(|e| panic!("fixture line {} did not parse: {e}\n{line}", i + 1));
            match frame {
                InboundFrame::Ready { pv } => {
                    assert_eq!(pv, PROTOCOL_VERSION);
                    ready += 1;
                }
                InboundFrame::Req { .. } => req += 1,
                InboundFrame::Res(_) => res += 1,
                InboundFrame::Evt(_) => evt += 1,
            }
        }
        // The recorded explore -> compile -> replay -> undo session: one ready,
        // six req/res pairs, and the real bus event stream in between.
        assert_eq!(ready, 1, "exactly one ready frame");
        assert_eq!(req, res, "each req has exactly one res");
        assert_eq!(req, 6, "six commands in the recorded session");
        assert!(evt >= 20, "the real bus event stream is forwarded as evt frames");
    }

    #[test]
    fn evt_frames_carry_an_envelope_and_null_thumb() {
        let text = std::fs::read_to_string(ipc_fixture("session-explore-compile-replay-undo.jsonl"))
            .unwrap();
        let mut checked = 0;
        for line in text.lines() {
            if let Ok(InboundFrame::Evt(evt)) = parse_inbound(line) {
                // The envelope is passed through untouched; it always has a
                // topic and a seq (crates/ir/src/bus.rs).
                assert!(evt.env.get("topic").is_some(), "evt env has a topic");
                assert!(evt.env.get("seq").is_some(), "evt env has a seq");
                // Headless mock recorder: thumb is present and null throughout
                // (contracts/fixtures/ipc/README.md).
                assert!(evt.thumb.is_none(), "mock recorder emits a null thumb");
                checked += 1;
            }
        }
        assert!(checked >= 20);
    }

    #[test]
    fn handshake_fixture_is_the_blocking_case() {
        let text = std::fs::read_to_string(ipc_fixture("handshake.json")).unwrap();
        let doc: Value = serde_json::from_str(&text).unwrap();

        // The ready frame and the get_capabilities req/res, framed per section 3.
        assert_eq!(doc["ready"]["t"], "ready");
        assert_eq!(doc["request"]["cmd"], "get_capabilities");

        let caps: Capabilities = serde_json::from_value(doc["response"]["result"].clone()).unwrap();
        // A default (mock) recorder build: the BLOCKING case that must force
        // the shell's blocking screen.
        assert!(!caps.can_automate(), "mock build cannot automate");
        assert_eq!(
            caps.missing_for_automation(),
            vec!["real_uia", "real_input"],
            "the blocking screen enumerates each false capability by contract name",
        );
        assert!(caps.planner_is_mock());
        assert_eq!(caps.transport_kind, "stdio");
    }

    #[test]
    fn real_capable_core_is_not_blocked() {
        // A real-capable core reports the same shape with the booleans true
        // (contracts/ipc.md section 3).
        let caps = Capabilities {
            real_uia: true,
            real_input: true,
            real_vision: true,
            mock_planner_only: false,
            transport_kind: "stdio".to_string(),
            version: "1.0.0".to_string(),
            git_sha: "abc123".to_string(),
        };
        assert!(caps.can_automate());
        assert!(caps.missing_for_automation().is_empty());
        assert!(!caps.planner_is_mock());
    }

    #[test]
    fn mock_transport_alone_blocks_automation() {
        let caps = Capabilities {
            real_uia: true,
            real_input: true,
            transport_kind: "mock".to_string(),
            ..Default::default()
        };
        assert!(!caps.can_automate(), "a mock transport blocks even with real uia and input");
        assert_eq!(caps.missing_for_automation(), vec!["transport_kind"]);
    }

    #[test]
    fn req_line_round_trips_and_is_one_line() {
        let args = serde_json::json!({"goal": "x", "window_process": "notepad.exe"});
        let line = encode_req_line("id-7", "start_explore", &args);
        assert!(line.ends_with('\n'), "a frame is terminated by exactly one newline");
        assert_eq!(line.matches('\n').count(), 1, "no embedded raw newline");

        // The exact wire fields (contracts/ipc.md section 2b).
        let wire: Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(wire["t"], "req");
        assert_eq!(wire["pv"], PROTOCOL_VERSION);
        assert_eq!(wire["id"], "id-7");
        assert_eq!(wire["cmd"], "start_explore");
        assert_eq!(wire["args"]["window_process"], "notepad.exe");

        // And it classifies as a req frame for the reader's dispatch.
        assert!(matches!(
            parse_inbound(&line).unwrap(),
            InboundFrame::Req { cmd } if cmd == "start_explore"
        ));
    }

    #[test]
    fn leading_bom_is_tolerated() {
        let framed = format!("\u{feff}{}", r#"{"pv":1,"t":"ready"}"#);
        assert!(matches!(parse_inbound(&framed), Ok(InboundFrame::Ready { .. })));
    }

    #[test]
    fn failure_res_carries_the_fixed_error_shape() {
        let line = r#"{"t":"res","pv":1,"id":"x","ok":false,"error":{"code":"invalid_args","message":"window_process is required","retryable":false}}"#;
        match parse_inbound(line).unwrap() {
            InboundFrame::Res(res) => {
                assert!(!res.ok);
                let err = CoreError::from_body(res.error.unwrap());
                assert_eq!(err.code, "invalid_args");
                assert!(!err.retryable);
            }
            _ => panic!("expected res"),
        }
    }
}
