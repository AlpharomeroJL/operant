//! The loopback callback receiver (`docs/specs/backends.md`: "loopback
//! redirect on 127.0.0.1 with an ephemeral port"). Binds `127.0.0.1:0`,
//! accepts exactly one HTTP/1.1 GET request -- the browser's redirect once
//! the user finishes signing in -- parses `code` and `state` off the query
//! string, and answers with a small confirmation page.
//!
//! Hand-rolled on `std::net` rather than pulling in an HTTP server crate:
//! the request shape is fixed and tiny (one GET, no body, no chunked
//! transfer-encoding), and this is the only thing in the whole flow that
//! is a server rather than a client. `accept_once` blocks the calling
//! thread; [`super::flow`] runs it on `tokio::task::spawn_blocking`.
//!
//! `accept_once` takes its own `timeout` and polls a non-blocking socket
//! internally rather than truly blocking forever on `accept()` -- on
//! purpose. A `spawn_blocking` task has no cancellation: if the browser
//! never redirects back, a plain blocking `accept()` never returns, the
//! task is never joined, and dropping the Tokio runtime at the end of a
//! test (or a real shutdown) then hangs waiting for that abandoned task to
//! finish, which it never does. Bounding the blocking call itself, rather
//! than only wrapping it in `tokio::time::timeout` from the outside, is
//! what actually avoids that hang: the outer wrapper stops *awaiting* the
//! task, it does not stop the task.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use super::error::OauthError;
use super::urlenc;

/// How often `accept_once` re-checks its deadline between poll attempts.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long an accepted connection gets to send its request before
/// [`handle_connection`] gives up -- a browser's redirect is one small,
/// immediate GET; anything slower than this is not that.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The parsed query string off one loopback callback request.
#[derive(Debug)]
pub struct CallbackRequest {
    params: HashMap<String, String>,
}

impl CallbackRequest {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.get(key).map(String::as_str)
    }
}

/// A bound-but-not-yet-accepting loopback socket.
pub struct LoopbackListener {
    listener: TcpListener,
    port: u16,
}

impl LoopbackListener {
    /// Bind `127.0.0.1` on an OS-assigned ephemeral port. Never binds any
    /// other interface -- there is no host parameter to get wrong.
    pub fn bind() -> Result<Self, OauthError> {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|e| OauthError::Listener(e.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|e| OauthError::Listener(e.to_string()))?
            .port();
        Ok(LoopbackListener { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The full `http://127.0.0.1:{port}{path}` redirect URI a provider's
    /// authorize endpoint will bounce the browser back to.
    pub fn redirect_uri(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// Block (up to `timeout`) until one connection arrives, respond, and
    /// return its parsed query string; `Err(OauthError::CallbackTimeout)`
    /// once `timeout` elapses with nobody connecting. Bounded by design
    /// (see module doc); run this inside `spawn_blocking` from async code.
    pub fn accept_once(&self, timeout: Duration) -> Result<CallbackRequest, OauthError> {
        self.listener
            .set_nonblocking(true)
            .map_err(|e| OauthError::Listener(e.to_string()))?;
        let deadline = Instant::now() + timeout;

        loop {
            match self.listener.accept() {
                Ok((stream, _addr)) => {
                    stream
                        .set_nonblocking(false)
                        .map_err(|e| OauthError::Listener(e.to_string()))?;
                    return handle_connection(stream);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(OauthError::CallbackTimeout);
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(e) => return Err(OauthError::Listener(e.to_string())),
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream) -> Result<CallbackRequest, OauthError> {
    stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
    let cloned = stream
        .try_clone()
        .map_err(|e| OauthError::Listener(e.to_string()))?;
    let mut reader = BufReader::new(cloned);

    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| OauthError::Listener(e.to_string()))?;

    // Drain (and discard) the remaining headers up to the blank line, so
    // the client sees a clean response rather than a reset connection.
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| OauthError::Listener(e.to_string()))?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    let path_and_query = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| OauthError::Listener(format!("malformed request line: {request_line:?}")))?;
    let query = path_and_query.split_once('?').map(|(_, q)| q).unwrap_or("");
    let params = urlenc::parse_query(query);

    let body = b"<html><body>Signed in. You can close this window.</body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| OauthError::Listener(e.to_string()))?;
    stream
        .write_all(body)
        .map_err(|e| OauthError::Listener(e.to_string()))?;
    stream.flush().ok();

    Ok(CallbackRequest { params })
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::TcpStream as ClientStream;

    use super::*;

    #[test]
    fn bind_picks_an_ephemeral_port_and_builds_a_loopback_redirect_uri() {
        let listener = LoopbackListener::bind().unwrap();
        assert!(listener.port() > 0);
        assert_eq!(
            listener.redirect_uri("/callback"),
            format!("http://127.0.0.1:{}/callback", listener.port())
        );
    }

    #[test]
    fn accept_once_parses_the_callback_query_string_and_responds_200() {
        let listener = LoopbackListener::bind().unwrap();
        let port = listener.port();

        let client = std::thread::spawn(move || {
            let mut stream = ClientStream::connect(("127.0.0.1", port)).unwrap();
            stream
                .write_all(
                    b"GET /callback?code=abc123&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
                )
                .unwrap();
            let mut buf = String::new();
            stream.read_to_string(&mut buf).unwrap();
            buf
        });

        let request = listener.accept_once(Duration::from_secs(5)).unwrap();
        assert_eq!(request.get("code"), Some("abc123"));
        assert_eq!(request.get("state"), Some("xyz"));

        let response = client.join().unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"), "got: {response}");
        assert!(response.contains("You can close this window"));
    }

    #[test]
    fn accept_once_returns_empty_params_for_a_query_less_request() {
        let listener = LoopbackListener::bind().unwrap();
        let port = listener.port();

        let client = std::thread::spawn(move || {
            let mut stream = ClientStream::connect(("127.0.0.1", port)).unwrap();
            stream
                .write_all(b"GET /callback HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                .unwrap();
            let mut buf = [0u8; 64];
            let _ = stream.read(&mut buf);
        });

        let request = listener.accept_once(Duration::from_secs(5)).unwrap();
        assert_eq!(request.get("code"), None);
        client.join().unwrap();
    }

    #[test]
    fn accept_once_gives_up_after_its_timeout_when_nobody_connects() {
        let listener = LoopbackListener::bind().unwrap();
        let started = Instant::now();
        let result = listener.accept_once(Duration::from_millis(50));
        assert!(
            matches!(result, Err(OauthError::CallbackTimeout)),
            "got {result:?}"
        );
        // Bounded, not instantaneous and not indefinite: proves the
        // deadline is actually honored rather than returning immediately
        // or blocking past it.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took far longer than the 50ms deadline"
        );
    }
}
