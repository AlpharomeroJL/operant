//! The local-only event buffer behind the watch-and-suggest detector.
//!
//! Two jobs, both privacy-load-bearing:
//!
//! 1. It is the single [`EventSink`] write path. Every byte the detector ever
//!    persists goes through [`EventSink::write`]; nothing in this module writes
//!    around it. That makes "off means off" a property of one call site: when
//!    the feature is disabled the [`super::Watcher`] returns before it reaches
//!    `write`, so the sink is provably never touched (see [`SpySink`] and the
//!    `disabled_watcher_never_writes` test). The counter that proves this
//!    (`writes`) is monotonic and is NOT reset by [`EventSink::purge`]: a purge
//!    empties stored data but must never erase the evidence that a write
//!    happened.
//! 2. It is capped and purgeable. [`CappedBuffer`] holds at most `capacity`
//!    events (oldest dropped first) and [`CappedBuffer::purge`] clears stored
//!    events in one call (the one-click purge the brief requires), while
//!    leaving the lifetime write count intact.

use std::collections::VecDeque;

use super::event::StoredEvent;

/// The one write path for the detector's local buffer. Implemented by the
/// real [`CappedBuffer`] and by the test [`SpySink`]. Keeping this a trait is
/// what lets a test inject a spy and assert the write path is never entered
/// while the feature is off, rather than only checking the buffer reads empty
/// after the fact.
pub trait EventSink {
    /// Persist one already-redacted event. This is the ONLY method that may
    /// store event data; a disabled detector must never call it.
    fn write(&mut self, event: StoredEvent);

    /// Every stored event, oldest first. Read-only; never a write path.
    fn events(&self) -> Vec<StoredEvent>;

    /// Number of events currently stored (after capping).
    fn len(&self) -> usize;

    /// True when no events are stored.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop all stored events (the one-click purge). Does not reset the
    /// lifetime write count: purging data must not erase the proof that data
    /// was ever written.
    fn purge(&mut self);

    /// Lifetime count of [`EventSink::write`] calls, never decremented, never
    /// reset by [`EventSink::purge`]. The "provably never written" signal:
    /// `total_writes() == 0` means the write path was never entered.
    fn total_writes(&self) -> u64;
}

/// A fixed-capacity ring of redacted events. Oldest events are dropped once
/// `capacity` is reached, so a long-running session cannot grow the local
/// buffer without bound.
#[derive(Debug)]
pub struct CappedBuffer {
    capacity: usize,
    events: VecDeque<StoredEvent>,
    writes: u64,
}

impl CappedBuffer {
    /// Build a buffer holding at most `capacity` events. A `capacity` of 0 is
    /// clamped to 1 so the ring always has room for the event just written
    /// (a zero-capacity buffer that silently discards every write would be a
    /// confusing footgun, not a useful configuration).
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        CappedBuffer {
            capacity,
            events: VecDeque::with_capacity(capacity),
            writes: 0,
        }
    }

    /// The configured maximum number of stored events.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl EventSink for CappedBuffer {
    fn write(&mut self, event: StoredEvent) {
        self.writes += 1;
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    fn events(&self) -> Vec<StoredEvent> {
        self.events.iter().cloned().collect()
    }

    fn len(&self) -> usize {
        self.events.len()
    }

    fn purge(&mut self) {
        self.events.clear();
    }

    fn total_writes(&self) -> u64 {
        self.writes
    }
}

/// A test-only sink that records every [`EventSink::write`] call so a test can
/// prove the write path was (or was not) entered. Lives outside `#[cfg(test)]`
/// so integration tests in `tests/` can use it too; it is not part of the
/// shipping detector's own wiring.
#[derive(Debug, Default)]
pub struct SpySink {
    events: Vec<StoredEvent>,
    writes: u64,
    purges: u64,
}

impl SpySink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of [`EventSink::purge`] calls, for tests that assert purge ran.
    pub fn purge_calls(&self) -> u64 {
        self.purges
    }
}

impl EventSink for SpySink {
    fn write(&mut self, event: StoredEvent) {
        self.writes += 1;
        self.events.push(event);
    }

    fn events(&self) -> Vec<StoredEvent> {
        self.events.clone()
    }

    fn len(&self) -> usize {
        self.events.len()
    }

    fn purge(&mut self) {
        self.purges += 1;
        self.events.clear();
    }

    fn total_writes(&self) -> u64 {
        self.writes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::event::StoredEvent;
    use operant_ir::{Action, ActionKind, Grounding, RiskClass};

    fn action(id: &str) -> Action {
        Action {
            v: 1,
            id: id.to_string(),
            kind: ActionKind::Key,
            intent: None,
            target: None,
            params: serde_json::Map::new(),
            pace: Default::default(),
            risk_class: RiskClass::Read,
            irreversible: false,
            grounding: Grounding::Uia,
            timeout_ms: 5000,
            retry: Default::default(),
        }
    }

    fn stored(id: &str) -> StoredEvent {
        StoredEvent {
            token: format!("key:{id}"),
            action: action(id),
        }
    }

    #[test]
    fn capped_buffer_drops_oldest_beyond_capacity() {
        let mut buf = CappedBuffer::new(2);
        buf.write(stored("a"));
        buf.write(stored("b"));
        buf.write(stored("c"));
        let ids: Vec<_> = buf.events().into_iter().map(|e| e.action.id).collect();
        assert_eq!(ids, vec!["b".to_string(), "c".to_string()]);
        assert_eq!(buf.len(), 2);
        // Three writes happened even though only two are retained.
        assert_eq!(buf.total_writes(), 3);
    }

    #[test]
    fn zero_capacity_is_clamped_to_one() {
        let mut buf = CappedBuffer::new(0);
        assert_eq!(buf.capacity(), 1);
        buf.write(stored("a"));
        buf.write(stored("b"));
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.events()[0].action.id, "b");
    }

    #[test]
    fn purge_empties_events_but_keeps_the_lifetime_write_count() {
        let mut buf = CappedBuffer::new(8);
        buf.write(stored("a"));
        buf.write(stored("b"));
        assert_eq!(buf.len(), 2);
        buf.purge();
        assert!(buf.is_empty());
        assert!(buf.events().is_empty());
        // Purge clears data but must not erase the proof writes occurred.
        assert_eq!(buf.total_writes(), 2);
    }

    #[test]
    fn spy_sink_counts_writes_and_purges() {
        let mut spy = SpySink::new();
        assert_eq!(spy.total_writes(), 0);
        spy.write(stored("a"));
        assert_eq!(spy.total_writes(), 1);
        spy.purge();
        assert_eq!(spy.purge_calls(), 1);
        assert_eq!(spy.total_writes(), 1);
        assert!(spy.is_empty());
    }
}
