//! Whatever carries JSON-RPC envelopes between two MCP peers.
//!
//! [`Transport`] is the seam: [`StdioTransport`] is the real one (newline-
//! delimited JSON, generic over any reader/writer so it is itself testable
//! against in-memory buffers, not just real stdio); [`InProcessTransport`]
//! wires a [`client::McpClient`](super::client::McpClient) directly to an
//! in-process peer function with no socket, pipe, or child process at all
//! -- what `docs/specs/mcp.md`'s "scripted handshake test... without a
//! real external server (use an in-process mock peer)" is built on.

use std::collections::VecDeque;
use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Send and receive JSON-RPC envelopes. `Send` because a real server's
/// `serve` loop and a real client's connection are expected to live behind
/// a thread or an async task in a full deployment, even though nothing in
/// this crate spawns one itself yet.
pub trait Transport: Send {
    fn send(&mut self, msg: &Value) -> io::Result<()>;

    /// The next message, or `Ok(None)` on a clean end of stream. Blocks
    /// (for a real transport) until one is available.
    fn recv(&mut self) -> io::Result<Option<Value>>;
}

/// The real MCP stdio transport: one JSON value per line, blank lines
/// ignored. Generic over the reader/writer pair so production wires
/// `Stdin`/`Stdout` (or a spawned child process's piped handles) and a test
/// wires an in-memory buffer to exercise the exact same framing code.
pub struct StdioTransport<R, W> {
    reader: R,
    writer: W,
}

impl<R: BufRead, W: Write> StdioTransport<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl<R: BufRead + Send, W: Write + Send> Transport for StdioTransport<R, W> {
    fn send(&mut self, msg: &Value) -> io::Result<()> {
        let line = serde_json::to_string(msg).map_err(io::Error::other)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    fn recv(&mut self) -> io::Result<Option<Value>> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.reader.read_line(&mut line)?;
            if n == 0 {
                return Ok(None); // EOF: the peer closed the connection.
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed).map_err(io::Error::other)?;
            return Ok(Some(value));
        }
    }
}

/// A [`Transport`] wired directly to an in-process peer function: no
/// sockets, pipes, or real process boundary. `peer` receives exactly the
/// JSON-RPC envelope a real transport would have written to the wire, and
/// returns exactly what a real peer would have written back (`None` for a
/// notification or anything else that gets no reply). `send` hands the
/// message to `peer` immediately and queues any reply; `recv` drains that
/// queue. This models a fully synchronous request/response peer, which is
/// all the handshake sequence (`initialize` -> `tools/list` -> `tools/call`)
/// needs.
pub struct InProcessTransport<F: FnMut(Value) -> Option<Value> + Send> {
    peer: F,
    queue: VecDeque<Value>,
}

impl<F: FnMut(Value) -> Option<Value> + Send> InProcessTransport<F> {
    pub fn new(peer: F) -> Self {
        Self {
            peer,
            queue: VecDeque::new(),
        }
    }
}

impl<F: FnMut(Value) -> Option<Value> + Send> Transport for InProcessTransport<F> {
    fn send(&mut self, msg: &Value) -> io::Result<()> {
        if let Some(reply) = (self.peer)(msg.clone()) {
            self.queue.push_back(reply);
        }
        Ok(())
    }

    fn recv(&mut self) -> io::Result<Option<Value>> {
        Ok(self.queue.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn stdio_transport_round_trips_one_message_per_line() {
        let input =
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n".to_vec();
        let mut out = Vec::new();
        {
            let mut t = StdioTransport::new(Cursor::new(input), &mut out);
            let msg = t.recv().unwrap().expect("one message on the line");
            assert_eq!(msg["method"], json!("tools/list"));
            t.send(&json!({ "jsonrpc": "2.0", "id": 1, "result": {} }))
                .unwrap();
        }
        let written = String::from_utf8(out).unwrap();
        // Exactly one line, newline-terminated, round-tripping to the sent
        // value; key order is not asserted (serde_json's `Value::Object`
        // ordering is a build-feature detail, not part of this contract).
        assert_eq!(written.matches('\n').count(), 1);
        assert!(written.ends_with('\n'));
        let sent: Value = serde_json::from_str(written.trim_end()).unwrap();
        assert_eq!(sent, json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
    }

    #[test]
    fn stdio_transport_skips_blank_lines_and_reports_clean_eof() {
        let input =
            b"\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}\n\n"
                .to_vec();
        let mut out = Vec::new();
        let mut t = StdioTransport::new(Cursor::new(input), &mut out);
        let msg = t.recv().unwrap().expect("blank lines are skipped");
        assert_eq!(msg["method"], json!("notifications/initialized"));
        assert!(t.recv().unwrap().is_none(), "EOF after the last real line");
    }

    #[test]
    fn in_process_transport_answers_a_request_and_stays_silent_on_a_notification() {
        let mut t = InProcessTransport::new(|msg: Value| {
            if msg.get("id").is_some() {
                Some(
                    json!({ "jsonrpc": "2.0", "id": msg["id"], "result": { "echoed": msg["method"] } }),
                )
            } else {
                None
            }
        });

        t.send(&json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": {} }))
            .unwrap();
        let reply = t.recv().unwrap().expect("the request got a reply");
        assert_eq!(reply["result"]["echoed"], json!("ping"));

        t.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }))
            .unwrap();
        assert!(
            t.recv().unwrap().is_none(),
            "a notification must never queue a reply"
        );
    }
}
