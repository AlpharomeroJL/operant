//! Typed, versioned in-process event bus (C1).
//!
//! Publishers get monotonic `seq`; subscribers filter by exact topic or `prefix.*`.
//! [`events`] adds strongly-typed payload constructors for the documented topic
//! families so publishers do not hand-build JSON.

pub mod events;

use std::sync::atomic::{AtomicU64, Ordering};

use crossbeam_channel::{unbounded, Receiver, Sender};
use operant_ir::bus::Envelope;
use parking_lot::Mutex;

use events::BusEvent;

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
        Bus {
            seq: AtomicU64::new(0),
            subs: Mutex::new(Vec::new()),
        }
    }

    /// Subscribe to a topic pattern (exact or `prefix.*`).
    pub fn subscribe(&self, pattern: &str) -> Subscription {
        let (tx, rx) = unbounded();
        self.subs.lock().push((pattern.to_string(), tx));
        Subscription {
            pattern: pattern.to_string(),
            rx,
        }
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

    /// Publish a strongly-typed payload from [`events`]. The topic comes from
    /// `E::TOPIC`, so callers cannot typo a topic string or hand-build JSON that
    /// drifts from `contracts/bus_events.md`. Equivalent to `publish` otherwise.
    pub fn publish_event<E: BusEvent>(&self, event: &E) -> Result<u64, serde_json::Error> {
        let payload = serde_json::to_value(event)?;
        Ok(self.publish(E::TOPIC, payload))
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

    #[test]
    fn publish_event_stamps_the_declared_topic() {
        let bus = Bus::new();
        let sub = bus.subscribe("sidecar.*");
        bus.publish_event(&events::SidecarStarted {
            name: "vision".into(),
            pid: 123,
        })
        .expect("SidecarStarted serializes");

        let env = sub.rx.try_recv().expect("event delivered");
        assert_eq!(env.topic, events::SidecarStarted::TOPIC);
        let payload: events::SidecarStarted =
            serde_json::from_value(env.payload).expect("payload deserializes back");
        assert_eq!(
            payload,
            events::SidecarStarted {
                name: "vision".into(),
                pid: 123
            }
        );
    }

    /// Test (a): every documented topic family in `contracts/bus_events.md`
    /// round-trips through the bus. The four families with typed constructors
    /// (runs, gates/approvals, sidecars/VRAM, guardian) publish via
    /// `publish_event`; the remaining four (perception, workflows, scheduler,
    /// doctor/metrics/suggestions) publish representative raw payloads, since
    /// this lane's typed-constructor scope is the first four. Either way the
    /// bus itself must deliver every family to a matching subscriber and the
    /// envelope must round-trip through JSON unchanged.
    #[test]
    fn all_documented_topic_families_roundtrip() {
        let bus = Bus::new();

        let runs = bus.subscribe("run.*");
        let gates = bus.subscribe("gate.*");
        let approvals = bus.subscribe("approval.*");
        let perception = bus.subscribe("perception.*");
        let sidecars = bus.subscribe("sidecar.*");
        let vram = bus.subscribe("vram.*");
        let workflows = bus.subscribe("workflow.*");
        let triggers = bus.subscribe("trigger.fired");
        let schedule = bus.subscribe("schedule.*");
        let guardian_kill = bus.subscribe("killswitch.*");
        let guardian_undo = bus.subscribe("undo.*");
        let doctor = bus.subscribe("doctor.*");
        let metrics = bus.subscribe("metrics.*");
        let suggestions = bus.subscribe("suggestion.*");

        // Runs (typed).
        bus.publish_event(&events::RunStarted {
            run_id: "r1".into(),
            goal: "demo".into(),
            mode: events::RunMode::Explore,
            workflow_name: None,
        })
        .unwrap();
        bus.publish_event(&events::RunCompleted {
            run_id: "r1".into(),
            outcome: events::RunOutcome::Ok,
            steps: 3,
            wall_ms: 500,
        })
        .unwrap();

        // Gates, approvals, escalations (typed).
        bus.publish_event(&events::GateEscalation {
            run_id: "r1".into(),
            step_id: None,
            sentence: "Confirm this write.".into(),
            requires_approval: true,
        })
        .unwrap();
        bus.publish_event(&events::ApprovalGranted {
            approval_id: "a1".into(),
            approver: "josef".into(),
        })
        .unwrap();

        // Perception (raw; not in this lane's typed-constructor scope).
        bus.publish(
            "perception.snapshot",
            serde_json::json!({"snapshot_digest": "d1", "window": "notepad.exe", "source": "uia", "element_count": 12, "truncated": false}),
        );

        // Sidecars and VRAM (typed).
        bus.publish_event(&events::SidecarStarted {
            name: "vision".into(),
            pid: 1,
        })
        .unwrap();
        bus.publish_event(&events::VramRequest {
            requester: "vision".into(),
            mb: 4000,
        })
        .unwrap();

        // Workflows (raw).
        bus.publish(
            "workflow.compiled",
            serde_json::json!({"name": "wf", "version": "1.0.0", "manifest_path": "m.json", "dsl_path": "wf.ts", "source_run_id": "r1"}),
        );

        // Scheduler (raw).
        bus.publish(
            "trigger.fired",
            serde_json::json!({"trigger_id": "t1", "kind": "cron", "workflow_name": "wf"}),
        );
        bus.publish(
            "schedule.enqueued",
            serde_json::json!({"run_id": "r2", "workflow_name": "wf"}),
        );

        // Guardian (typed).
        bus.publish_event(&events::KillswitchEngaged { at_ms: 42 })
            .unwrap();
        bus.publish_event(&events::UndoPreviewed {
            run_id: "r1".into(),
            entries: 2,
            irreversible: 0,
            items: Vec::new(),
        })
        .unwrap();

        // Doctor, metrics, suggestions (raw).
        bus.publish(
            "doctor.finding",
            serde_json::json!({"finding_id": "f1", "severity": "warn", "what": "x", "why": "y", "action": "z"}),
        );
        bus.publish(
            "metrics.week.rolled",
            serde_json::json!({"week": "2026-W28", "minutes_saved_total": 10}),
        );
        bus.publish(
            "suggestion.offered",
            serde_json::json!({"suggestion_id": "s1", "pattern_digest": "p1", "occurrences": 3}),
        );

        let assert_family = |label: &str, rx: &Receiver<Envelope>, expected: usize| {
            let got: Vec<_> = rx.try_iter().collect();
            assert_eq!(
                got.len(),
                expected,
                "family {label} expected {expected} events, got {got:?}"
            );
            for env in &got {
                // Envelope round-trip: every delivered event survives a JSON
                // encode/decode cycle unchanged (the transport-level contract).
                let encoded = serde_json::to_string(env).expect("envelope serializes");
                let decoded: Envelope =
                    serde_json::from_str(&encoded).expect("envelope deserializes");
                assert_eq!(&decoded, env);
                assert_eq!(decoded.v, 1);
            }
        };

        assert_family("runs", &runs.rx, 2);
        assert_family("gates", &gates.rx, 1);
        assert_family("approvals", &approvals.rx, 1);
        assert_family("perception", &perception.rx, 1);
        assert_family("sidecars", &sidecars.rx, 1);
        assert_family("vram", &vram.rx, 1);
        assert_family("workflows", &workflows.rx, 1);
        assert_family("triggers", &triggers.rx, 1);
        assert_family("schedule", &schedule.rx, 1);
        assert_family("guardian.killswitch", &guardian_kill.rx, 1);
        assert_family("guardian.undo", &guardian_undo.rx, 1);
        assert_family("doctor", &doctor.rx, 1);
        assert_family("metrics", &metrics.rx, 1);
        assert_family("suggestions", &suggestions.rx, 1);
    }
}
