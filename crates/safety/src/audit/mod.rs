//! Hash-chained audit log.
//!
//! Every run event (action executed, gate result, approval, grant change,
//! workflow install, drift merge) is appended as a JSONL record whose `hash`
//! chains the previous record's hash with the record's own contents (BLAKE3).
//! [`AuditLog::verify`] walks the chain and detects any tampering; the log
//! exports as JSONL, and as a human-readable PDF via [`AuditLog::export_pdf`]
//! (see [`pdf`]).

use serde::{Deserialize, Serialize};

use crate::error::AuditError;

/// Human-readable PDF export of the chain (L6B). Adds the [`AuditLog::export_pdf`]
/// and [`AuditLog::write_pdf`] methods; pulls in no new dependency.
mod pdf;

/// The genesis `prev_hash`: 64 hex zeros. The first event chains from this.
pub const GENESIS: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// One append-only audit record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Monotonic sequence number (0-based), equal to the record's position.
    pub seq: u64,
    /// Timestamp string as supplied by the caller (opaque to the chain math,
    /// but bound into the hash so it cannot be altered after the fact).
    pub ts: String,
    /// The hash of the previous record (or [`GENESIS`] for the first).
    pub prev_hash: String,
    /// The event body.
    pub payload: serde_json::Value,
    /// `BLAKE3(seq || prev_hash || ts || canonical(payload))`, hex.
    pub hash: String,
}

/// An append-only, hash-chained log.
#[derive(Debug, Clone, Default)]
pub struct AuditLog {
    events: Vec<AuditEvent>,
}

impl AuditLog {
    /// A new, empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current chain head hash (or [`GENESIS`] when empty).
    pub fn head(&self) -> String {
        self.events.last().map(|e| e.hash.clone()).unwrap_or_else(|| GENESIS.to_string())
    }

    /// The recorded events, in order.
    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    /// Append an event with the given timestamp and payload, returning the new
    /// record's hash (the new chain head).
    pub fn append(&mut self, ts: impl Into<String>, payload: serde_json::Value) -> String {
        let prev_hash = self.head();
        let seq = self.events.len() as u64;
        let ts = ts.into();
        let hash = compute_hash(seq, &prev_hash, &ts, &payload);
        self.events.push(AuditEvent { seq, ts, prev_hash, payload, hash: hash.clone() });
        hash
    }

    /// Walk the chain and verify integrity. Returns the first break found.
    pub fn verify(&self) -> Result<(), AuditError> {
        let mut prev = GENESIS.to_string();
        for (i, e) in self.events.iter().enumerate() {
            if e.seq != i as u64 {
                return Err(AuditError::OutOfOrder { index: i });
            }
            if e.prev_hash != prev {
                return Err(AuditError::BrokenLink { index: i });
            }
            let recomputed = compute_hash(e.seq, &e.prev_hash, &e.ts, &e.payload);
            if recomputed != e.hash {
                return Err(AuditError::HashMismatch { index: i });
            }
            prev = e.hash.clone();
        }
        Ok(())
    }

    /// Export the chain as JSONL (one record per line).
    pub fn export_jsonl(&self) -> String {
        self.events
            .iter()
            .map(|e| serde_json::to_string(e).expect("audit event serializes"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Reload a log from JSONL text (does not verify; call [`Self::verify`]).
    pub fn from_jsonl(text: &str) -> Result<Self, serde_json::Error> {
        let mut events = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            events.push(serde_json::from_str::<AuditEvent>(line)?);
        }
        Ok(AuditLog { events })
    }
}

/// `BLAKE3(seq_be || prev_hash || ts || canonical_json(payload))`, hex-encoded.
///
/// `serde_json` serializes object keys in sorted order (its map is a `BTreeMap`),
/// so the payload encoding is canonical and the hash is stable.
fn compute_hash(seq: u64, prev_hash: &str, ts: &str, payload: &serde_json::Value) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&seq.to_be_bytes());
    hasher.update(prev_hash.as_bytes());
    hasher.update(ts.as_bytes());
    let canonical = serde_json::to_string(payload).expect("payload serializes");
    hasher.update(canonical.as_bytes());
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn built_chain() -> AuditLog {
        let mut log = AuditLog::new();
        log.append("t0", json!({ "event": "workflow.install", "name": "notepad-invoice-note" }));
        log.append("t1", json!({ "event": "action.executed", "id": "s1", "outcome": "ok" }));
        log.append("t2", json!({ "event": "gate.result", "gate": 2, "result": "pass" }));
        log.append("t3", json!({ "event": "approval", "reason": "credential_field" }));
        log
    }

    #[test]
    fn verify_passes_on_a_built_chain() {
        let log = built_chain();
        assert_eq!(log.events().len(), 4);
        assert!(log.verify().is_ok());
        // First record chains from genesis.
        assert_eq!(log.events()[0].prev_hash, GENESIS);
        // Each record chains from the previous head.
        assert_eq!(log.events()[1].prev_hash, log.events()[0].hash);
    }

    #[test]
    fn verify_fails_on_a_tampered_payload() {
        let mut log = built_chain();
        // Tamper: rewrite the outcome of event 1 without recomputing its hash.
        log.events[1].payload["outcome"] = json!("tampered");
        match log.verify() {
            Err(AuditError::HashMismatch { index }) => assert_eq!(index, 1),
            other => panic!("expected HashMismatch at 1, got {other:?}"),
        }
    }

    #[test]
    fn verify_fails_on_a_tampered_hash_that_breaks_the_link() {
        let mut log = built_chain();
        // Tamper: rewrite event 1's stored hash to a plausible-looking value.
        log.events[1].hash =
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string();
        // Event 1's own recomputed hash no longer matches -> caught at index 1.
        assert!(log.verify().is_err());
    }

    #[test]
    fn jsonl_roundtrip_preserves_verification() {
        let log = built_chain();
        let jsonl = log.export_jsonl();
        assert_eq!(jsonl.lines().count(), 4);
        let reloaded = AuditLog::from_jsonl(&jsonl).unwrap();
        assert!(reloaded.verify().is_ok());
        assert_eq!(reloaded.head(), log.head());
    }
}
