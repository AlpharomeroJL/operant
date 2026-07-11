//! Typed, versioned in-process event bus (C1 scaffold).
//!
//! Publishers get monotonic `seq`; subscribers filter by exact topic or `prefix.*`.
//! L1A extends this with cross-process sidecar delivery and a watchdog.

use std::sync::atomic::{AtomicU64, Ordering};

use crossbeam_channel::{unbounded, Receiver, Sender};
use operant_ir::bus::Envelope;
use parking_lot::Mutex;

/// A subscription: a topic pattern plus the receiving end of a channel.
pub struct Subscription {
    pub pattern: String,
    pub rx: Receiver<Envelope>,
}

/// The in-process bus. Cheap to clone the handle via `&`.
pub struct Bus {
    seq: AtomicU64,
    subs: Mutex<Vec<(String, Sender<Envelope>)>>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        Bus { seq: AtomicU64::new(0), subs: Mutex::new(Vec::new()) }
    }

    /// Subscribe to a topic pattern (exact or `prefix.*`).
    pub fn subscribe(&self, pattern: &str) -> Subscription {
        let (tx, rx) = unbounded();
        self.subs.lock().push((pattern.to_string(), tx));
        Subscription { pattern: pattern.to_string(), rx }
    }

    /// Publish a payload under a topic. `seq` and `ts` are stamped here.
    /// Returns the assigned sequence number.
    pub fn publish(&self, topic: &str, payload: serde_json::Value) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let env = Envelope {
            v: 1,
            seq,
            // Wall-clock timestamp is assigned by L1A's clock abstraction; the
            // scaffold uses a monotonic placeholder so the bus is deterministic in tests.
            ts: format!("seq:{seq}"),
            topic: topic.to_string(),
            payload,
        };
        let mut subs = self.subs.lock();
        subs.retain(|(pattern, tx)| {
            if env.matches(pattern) {
                tx.send(env.clone()).is_ok()
            } else {
                true
            }
        });
        seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_filters() {
        let bus = Bus::new();
        let run = bus.subscribe("run.*");
        let sched = bus.subscribe("schedule.enqueued");

        bus.publish("run.started", serde_json::json!({"run_id": "r1"}));
        bus.publish("schedule.enqueued", serde_json::json!({"run_id": "r1"}));
        bus.publish("run.completed", serde_json::json!({"run_id": "r1"}));

        // run.* subscriber sees the two run events, not the schedule one.
        let got: Vec<_> = run.rx.try_iter().collect();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].topic, "run.started");
        assert_eq!(got[0].seq, 0);
        assert_eq!(got[1].topic, "run.completed");

        let s: Vec<_> = sched.rx.try_iter().collect();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].topic, "schedule.enqueued");
    }
}
