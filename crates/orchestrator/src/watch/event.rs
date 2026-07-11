//! The event types that flow into and out of the detector.
//!
//! [`ManualEvent`] is what a caller hands the detector: one manual (non-Operant)
//! user action, plus the on-screen element it touched (when known), so the
//! redaction pass can tell whether that element was a credential field.
//!
//! [`StoredEvent`] is what actually lands in the buffer: a normalized token for
//! n-gram matching plus a redacted copy of the action. It is produced by
//! [`super::normalize::redact_for_storage`] and is the only shape the buffer
//! ever holds, so nothing unredacted can reach storage.

use operant_ir::{Action, Element};

/// One manual user action observed by the detector, together with the element
/// it targeted (if the caller could resolve one). The `target` element is used
/// only to classify sensitivity via [`operant_recorder::redact`]; it is never
/// stored.
#[derive(Debug, Clone)]
pub struct ManualEvent {
    /// The user's action, expressed as Action IR.
    pub action: Action,
    /// The element the action targeted, when known. Drives credential
    /// redaction; not persisted.
    pub target: Option<Element>,
}

impl ManualEvent {
    /// A manual event with no resolved target element.
    pub fn new(action: Action) -> Self {
        ManualEvent { action, target: None }
    }

    /// A manual event carrying the element it targeted, so redaction can
    /// classify it.
    pub fn with_target(action: Action, target: Element) -> Self {
        ManualEvent { action, target: Some(target) }
    }
}

/// A redacted event as stored in the local buffer. `token` is the normalized
/// digest used for pattern matching; `action` is the redacted Action IR kept
/// so an accepted suggestion can seed a supervised run. Neither field ever
/// carries free-typed text or credential-field contents (see
/// [`super::normalize`]).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    /// Normalized action digest, e.g. `click:automation_id=SaveButton`.
    pub token: String,
    /// The redacted action. Sensitive params are scrubbed before this is
    /// built; the raw action never reaches here.
    pub action: Action,
}
