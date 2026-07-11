//! Pin-on-first-use publisher trust. The store maps a publisher name to the
//! fingerprint of the key it has been trusted with; the first observation of
//! a publisher pins it and is reported back so the caller can surface the
//! fingerprint, exactly as `docs/specs/registry.md` describes: "first
//! install of a publisher shows the fingerprint and asks once."

use std::collections::HashMap;

use crate::error::RegistryError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinOutcome {
    /// This publisher has not been seen before; it is now pinned to the
    /// presented fingerprint.
    FirstUse,
    /// This publisher was already pinned and the presented fingerprint
    /// matches.
    Trusted,
}

#[derive(Debug, Default, Clone)]
pub struct PinStore {
    pins: HashMap<String, String>,
}

impl PinStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a store from previously-persisted pins (publisher -> fingerprint).
    pub fn from_pins(pins: HashMap<String, String>) -> Self {
        Self { pins }
    }

    /// Expose the current pins so a caller can persist them.
    pub fn pins(&self) -> &HashMap<String, String> {
        &self.pins
    }

    pub fn fingerprint_for(&self, publisher: &str) -> Option<&str> {
        self.pins.get(publisher).map(String::as_str)
    }

    /// Record a verified `(publisher, fingerprint)` pair.
    ///
    /// A publisher pins to the first fingerprint it is ever observed with.
    /// A later call with a different fingerprint under the same publisher
    /// name is a possible key rotation or impersonation attempt and is
    /// rejected rather than silently re-pinned.
    pub fn observe(
        &mut self,
        publisher: &str,
        fingerprint: &str,
    ) -> Result<PinOutcome, RegistryError> {
        match self.pins.get(publisher) {
            None => {
                self.pins
                    .insert(publisher.to_string(), fingerprint.to_string());
                Ok(PinOutcome::FirstUse)
            }
            Some(pinned) if pinned == fingerprint => Ok(PinOutcome::Trusted),
            Some(pinned) => Err(RegistryError::PublisherKeyRotated {
                publisher: publisher.to_string(),
                pinned: pinned.clone(),
                presented: fingerprint.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_use_then_trusted() {
        let mut pins = PinStore::new();
        assert_eq!(
            pins.observe("acme", "abc123").unwrap(),
            PinOutcome::FirstUse
        );
        assert_eq!(pins.observe("acme", "abc123").unwrap(), PinOutcome::Trusted);
        assert_eq!(pins.fingerprint_for("acme"), Some("abc123"));
    }

    #[test]
    fn rotated_key_is_rejected() {
        let mut pins = PinStore::new();
        pins.observe("acme", "abc123").unwrap();
        let err = pins.observe("acme", "def456").unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyRotated { .. }));
        // The original pin is untouched by the rejected attempt.
        assert_eq!(pins.fingerprint_for("acme"), Some("abc123"));
    }

    #[test]
    fn unknown_publisher_has_no_pin() {
        let pins = PinStore::new();
        assert_eq!(pins.fingerprint_for("nobody"), None);
    }
}
