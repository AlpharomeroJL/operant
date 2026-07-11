//! Write side: [`Mailer`] is what `email.send` dispatches against.
//! [`RecordingMailer`] is the backend every test in this crate uses (and a
//! safe default for anyone who has not wired a real relay): it never
//! opens a socket. [`SmtpMailer`] is a minimal real SMTP client
//! (`std::net` only, no new dependency) for production use; it is not
//! exercised by `cargo test` (`docs/specs/action.md`: "no real network").

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use super::EmailError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundMessage {
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReceipt {
    pub accepted_to: Vec<String>,
    pub message_id: String,
}

/// `send` is irreversible (`contracts/action_ir.schema.json`'s
/// `irreversible` flag; `docs/specs/action.md`: "send is irreversible,
/// labeled") so this trait's contract is: an `Ok` return means the
/// message left this process. There is deliberately no "undo".
pub trait Mailer: Send + Sync {
    fn send(&self, msg: &OutboundMessage) -> Result<SendReceipt, EmailError>;
}

fn generate_message_id(domain: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("<{nanos:x}-{n:x}@{domain}>")
}

/// In-memory [`Mailer`]: records every send, performs zero I/O. Used by
/// every test that exercises `email.send`, and a safe out-of-the-box
/// default for anyone who has not configured a real relay.
#[derive(Default)]
pub struct RecordingMailer {
    sent: Mutex<Vec<OutboundMessage>>,
}

impl RecordingMailer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every message handed to [`Mailer::send`] so far, in send order.
    pub fn sent(&self) -> Vec<OutboundMessage> {
        self.sent.lock().clone()
    }
}

impl Mailer for RecordingMailer {
    fn send(&self, msg: &OutboundMessage) -> Result<SendReceipt, EmailError> {
        let message_id = generate_message_id("operant.local");
        self.sent.lock().push(msg.clone());
        Ok(SendReceipt {
            accepted_to: msg.to.clone(),
            message_id,
        })
    }
}

/// Minimal plaintext SMTP client (RFC 5321 HELO/EHLO, MAIL FROM, RCPT TO,
/// DATA). No STARTTLS, no AUTH, no connection pooling: enough to talk to
/// an unauthenticated local relay. Bringing this up to a real provider
/// (TLS on 587, AUTH) is a FOLLOWUP; see `RESULT.md`.
pub struct SmtpMailer {
    pub host: String,
    pub port: u16,
    pub helo_domain: String,
    pub timeout: Duration,
}

impl SmtpMailer {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            helo_domain: "operant.local".to_string(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl Mailer for SmtpMailer {
    fn send(&self, msg: &OutboundMessage) -> Result<SendReceipt, EmailError> {
        let mut conn = SmtpConn::connect(&self.host, self.port, self.timeout)?;
        conn.expect(&format!("EHLO {}", self.helo_domain), |c| c == 250)
            .or_else(|_| conn.expect(&format!("HELO {}", self.helo_domain), |c| c == 250))?;
        conn.expect(&format!("MAIL FROM:<{}>", msg.from), |c| c == 250)?;
        for rcpt in &msg.to {
            conn.expect(&format!("RCPT TO:<{rcpt}>"), |c| c == 250 || c == 251)?;
        }
        conn.expect("DATA", |c| c == 354)?;

        let message_id = generate_message_id(&self.helo_domain);
        let mut data = String::new();
        data.push_str(&format!("From: {}\r\n", msg.from));
        data.push_str(&format!("To: {}\r\n", msg.to.join(", ")));
        data.push_str(&format!("Subject: {}\r\n", msg.subject));
        data.push_str(&format!("Message-ID: {message_id}\r\n"));
        data.push_str("\r\n");
        for line in msg.body.lines() {
            // Dot-stuffing (RFC 5321 4.5.2): a line that starts with '.'
            // gets one extra leading '.' so it is not read as the
            // end-of-data marker.
            if line.starts_with('.') {
                data.push('.');
            }
            data.push_str(line);
            data.push_str("\r\n");
        }
        data.push('.');
        conn.expect(&data, |c| c == 250)?;
        let _ = conn.command("QUIT");

        Ok(SendReceipt {
            accepted_to: msg.to.clone(),
            message_id,
        })
    }
}

struct SmtpConn {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl SmtpConn {
    fn connect(host: &str, port: u16, timeout: Duration) -> Result<Self, EmailError> {
        let stream = TcpStream::connect((host, port))
            .map_err(|e| EmailError::Smtp(format!("connect {host}:{port}: {e}")))?;
        stream.set_read_timeout(Some(timeout)).ok();
        stream.set_write_timeout(Some(timeout)).ok();
        let writer = stream
            .try_clone()
            .map_err(|e| EmailError::Smtp(format!("clone socket: {e}")))?;
        let mut conn = Self {
            reader: BufReader::new(stream),
            writer,
        };
        let (code, text) = conn.read_response()?;
        if code != 220 {
            return Err(EmailError::Smtp(format!(
                "no 220 greeting, got {code} {text}"
            )));
        }
        Ok(conn)
    }

    fn read_response(&mut self) -> Result<(u16, String), EmailError> {
        let mut lines = Vec::new();
        let code = loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .map_err(|e| EmailError::Smtp(format!("read: {e}")))?;
            if n == 0 {
                return Err(EmailError::Smtp("connection closed mid-response".into()));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.len() < 4 {
                return Err(EmailError::Smtp(format!(
                    "malformed response line {line:?}"
                )));
            }
            let code: u16 = line[..3]
                .parse()
                .map_err(|_| EmailError::Smtp(format!("bad status code in {line:?}")))?;
            let sep = line.as_bytes()[3];
            lines.push(line[4..].to_string());
            if sep == b' ' {
                break code; // final line; '-' in this position means "more follows"
            }
        };
        Ok((code, lines.join(" ")))
    }

    fn command(&mut self, cmd: &str) -> Result<(u16, String), EmailError> {
        self.writer
            .write_all(cmd.as_bytes())
            .and_then(|_| self.writer.write_all(b"\r\n"))
            .map_err(|e| EmailError::Smtp(format!("write: {e}")))?;
        self.read_response()
    }

    fn expect(&mut self, cmd: &str, ok: impl Fn(u16) -> bool) -> Result<String, EmailError> {
        let (code, text) = self.command(cmd)?;
        if ok(code) {
            Ok(text)
        } else {
            let head: String = cmd.chars().take(40).collect();
            Err(EmailError::Smtp(format!("`{head}` got {code} {text}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg() -> OutboundMessage {
        OutboundMessage {
            from: "owner@operant-fixture.example".into(),
            to: vec!["customer@example.com".into()],
            subject: "Re: your invoice".into(),
            body: "Thanks, paid.".into(),
        }
    }

    #[test]
    fn recording_mailer_never_touches_the_network_and_records_the_send() {
        let mailer = RecordingMailer::new();
        let receipt = mailer.send(&msg()).unwrap();
        assert_eq!(receipt.accepted_to, vec!["customer@example.com"]);
        assert!(receipt.message_id.starts_with('<'));
        assert_eq!(mailer.sent(), vec![msg()]);
    }

    #[test]
    fn recording_mailer_assigns_distinct_ids_per_send() {
        let mailer = RecordingMailer::new();
        let r1 = mailer.send(&msg()).unwrap();
        let r2 = mailer.send(&msg()).unwrap();
        assert_ne!(r1.message_id, r2.message_id);
        assert_eq!(mailer.sent().len(), 2);
    }
}
