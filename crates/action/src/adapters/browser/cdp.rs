//! Minimal real C5 backend: attach to a live Chrome/Edge tab over the
//! DevTools Protocol's WebSocket channel. Behind the `cdp` cargo feature
//! so the default build (and every lane that only needs
//! [`super::FixtureBrowser`]) never compiles a WebSocket client. Not
//! exercised by `cargo test` (no real browser in CI) -- the same
//! "compiles, unverified against the real thing, FOLLOWUPS track
//! hardening" posture `super::office::com`'s real COM backend documents
//! for its own feature gate.
//!
//! Deliberately thin per this lane's brief ("fine to leave minimal"):
//! `attach` takes a `ws://host:port/devtools/page/<id>` DevTools target
//! URL directly (as printed by Chrome/Edge's `GET /json/list` HTTP
//! endpoint when launched with `--remote-debugging-port`; that HTTP
//! discovery step itself is not implemented here) and performs a real
//! WebSocket upgrade handshake against it, then a live-session check
//! (`Browser.getVersion`). `snapshot`/`act` need the CDP DOM and Input
//! domains layered on top of that socket (`DOM.getDocument`/
//! `DOM.querySelector`/an accessibility walk for the read side,
//! `Input.dispatchMouseEvent`/`Input.insertText` for the write side) --
//! real, but a substantially larger surface than this lane's bar asks
//! for, so both return [`BrowserError::Unsupported`] until a follow-up
//! lane wires the domain calls up (see FOLLOWUPS in `RESULT.md`).

use std::net::TcpStream;

use parking_lot::Mutex;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use operant_ir::snapshot::Snapshot;
use operant_ir::Action;

use super::{Browser, BrowserAct, BrowserError};

type CdpSocket = WebSocket<MaybeTlsStream<TcpStream>>;

/// A [`Browser`] backed by a real DevTools Protocol WebSocket session.
#[derive(Default)]
pub struct CdpBrowser {
    socket: Mutex<Option<CdpSocket>>,
}

impl CdpBrowser {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Browser for CdpBrowser {
    /// `target`: a `ws://` DevTools target URL. Performs the real
    /// WebSocket upgrade handshake and, on success, sends
    /// `Browser.getVersion` as a live-session check before returning.
    fn attach(&self, target: &str) -> Result<(), BrowserError> {
        let (mut socket, _response) = tungstenite::connect(target)
            .map_err(|e| BrowserError::Cdp(format!("connect {target}: {e}")))?;
        socket
            .send(Message::Text(
                r#"{"id":1,"method":"Browser.getVersion"}"#.into(),
            ))
            .map_err(|e| BrowserError::Cdp(format!("Browser.getVersion: {e}")))?;
        *self.socket.lock() = Some(socket);
        Ok(())
    }

    fn snapshot(&self) -> Result<Snapshot, BrowserError> {
        if self.socket.lock().is_none() {
            return Err(BrowserError::NotAttached);
        }
        Err(BrowserError::Unsupported(
            "CdpBrowser::snapshot needs the DOM domain (DOM.getDocument plus an \
             accessibility walk) layered on the attached socket; see FOLLOWUPS",
        ))
    }

    fn act(&self, _act: &BrowserAct) -> Result<Action, BrowserError> {
        if self.socket.lock().is_none() {
            return Err(BrowserError::NotAttached);
        }
        Err(BrowserError::Unsupported(
            "CdpBrowser::act needs the Input domain (dispatchMouseEvent/insertText) \
             layered on the attached socket; see FOLLOWUPS",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // No real browser in CI: this only proves the typed-error paths this
    // minimal backend documents, not real DevTools connectivity.

    #[test]
    fn snapshot_before_attach_is_not_attached() {
        let cdp = CdpBrowser::new();
        assert!(matches!(
            cdp.snapshot().unwrap_err(),
            BrowserError::NotAttached
        ));
    }

    #[test]
    fn act_before_attach_is_not_attached() {
        let cdp = CdpBrowser::new();
        let act = BrowserAct {
            id: "a1".into(),
            kind: operant_ir::ActionKind::Click,
            selector: operant_ir::Selector::Css {
                value: "#save-btn".into(),
            },
            params: serde_json::Map::new(),
        };
        assert!(matches!(
            cdp.act(&act).unwrap_err(),
            BrowserError::NotAttached
        ));
    }

    #[test]
    fn attach_to_an_unreachable_target_is_a_typed_cdp_error() {
        let cdp = CdpBrowser::new();
        // Nothing listens here; the point is a typed error, not a panic.
        let err = cdp
            .attach("ws://127.0.0.1:1/devtools/page/none")
            .unwrap_err();
        assert!(matches!(err, BrowserError::Cdp(_)));
    }
}
