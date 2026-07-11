//! Errors surfaced by the model backend layer.

use thiserror::Error;

/// A structured backend error. Fields mirror the `error` `BackendEvent`
/// variant exactly, so a failure can be reported the same way whether it
/// happens before the stream starts (`probe`, request building) or
/// mid-stream (as a terminal `BackendEvent::Error`).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct BackendError {
    pub error_id: String,
    pub message: String,
    pub retryable: bool,
}

impl BackendError {
    pub fn new(error_id: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            error_id: error_id.into(),
            message: message.into(),
            retryable,
        }
    }

    /// A failure in the transport itself (connect, timeout, I/O). Treated
    /// as retryable: these are almost always transient.
    pub fn transport(message: impl Into<String>) -> Self {
        Self::new("transport_error", message, true)
    }

    /// The request never left the client: bad config, unknown provider,
    /// missing credential. Never retryable without a config change.
    pub fn config(message: impl Into<String>) -> Self {
        Self::new("config_error", message, false)
    }

    /// A response body that did not parse as the dialect expected.
    pub fn parse(message: impl Into<String>) -> Self {
        Self::new("parse_error", message, false)
    }
}

/// Errors from the [`crate::backends::HttpTransport`] seam. Kept separate
/// from [`BackendError`] because a transport has no notion of provider or
/// dialect; the client layer maps a `TransportError` into a `BackendError`
/// once it knows that context.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("request timed out after {0}ms")]
    Timeout(u64),
    #[error("transport failure: {0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_expected_error_ids_and_retryability() {
        assert_eq!(BackendError::transport("boom").error_id, "transport_error");
        assert!(BackendError::transport("boom").retryable);

        assert_eq!(BackendError::config("bad config").error_id, "config_error");
        assert!(!BackendError::config("bad config").retryable);

        assert_eq!(BackendError::parse("bad json").error_id, "parse_error");
        assert!(!BackendError::parse("bad json").retryable);
    }

    #[test]
    fn display_renders_the_message() {
        let e = BackendError::new("x", "human readable text", false);
        assert_eq!(e.to_string(), "human readable text");
    }
}
