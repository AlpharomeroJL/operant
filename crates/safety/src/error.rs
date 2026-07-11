//! Typed errors for the safety crate.

use thiserror::Error;

/// An error raised while loading a workflow manifest under the safety guard.
#[derive(Debug, Error)]
pub enum SafetyError {
    /// The manifest declares a gate with `kind: "safety"`. Safety gates are
    /// runtime-owned; a workflow may never declare one.
    #[error("workflow manifest declares a runtime-owned safety gate; safety gates cannot live in a workflow file")]
    SafetyGateInManifest,

    /// The manifest carries a directive that tries to switch off the hard
    /// invariants (e.g. `disable_safety: true`).
    #[error("workflow manifest attempts to disable runtime-owned safety invariants")]
    AttemptToDisableSafety,

    /// The manifest is not valid JSON / not a valid manifest.
    #[error("workflow manifest failed to parse: {0}")]
    Parse(#[from] serde_json::Error),
}

/// An error returned by [`crate::AuditLog::verify`] when the chain is broken.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuditError {
    /// The event's `seq` does not match its position in the chain.
    #[error("audit event {index} is out of sequence")]
    OutOfOrder {
        /// Zero-based position in the chain.
        index: usize,
    },

    /// The event's `prev_hash` does not equal the previous event's hash.
    #[error("audit event {index} breaks the chain: prev_hash does not match the prior head")]
    BrokenLink {
        /// Zero-based position in the chain.
        index: usize,
    },

    /// The event's stored hash does not match a recomputation over its contents.
    #[error("audit event {index} has been tampered with: content hash mismatch")]
    HashMismatch {
        /// Zero-based position in the chain.
        index: usize,
    },
}
