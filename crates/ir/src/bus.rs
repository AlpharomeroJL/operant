//! Bus envelope. Mirrors `contracts/bus_events.md`.
//!
//! The typed vocabulary of topic payloads lives with the bus implementation in
//! `operant-core`; here we define only the versioned envelope so every crate can
//! construct and match events without depending on core.

use serde::{Deserialize, Serialize};

fn default_v() -> u32 {
    1
}

/// A published event. `seq` and `ts` are assigned by the bus at publish time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default = "default_v")]
    pub v: u32,
    pub seq: u64,
    pub ts: String,
    pub topic: String,
    pub payload: serde_json::Value,
}

impl Envelope {
    /// True when this event's topic matches `pattern` exactly or by `prefix.*`.
    pub fn matches(&self, pattern: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix(".*") {
            self.topic == prefix || self.topic.starts_with(&format!("{prefix}."))
        } else {
            self.topic == pattern
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(topic: &str) -> Envelope {
        Envelope { v: 1, seq: 1, ts: "t".into(), topic: topic.into(), payload: serde_json::Value::Null }
    }

    #[test]
    fn prefix_matching() {
        assert!(ev("run.step.executed").matches("run.*"));
        assert!(ev("run.step.executed").matches("run.step.executed"));
        assert!(ev("run").matches("run.*"));
        assert!(!ev("schedule.enqueued").matches("run.*"));
        assert!(!ev("runner.x").matches("run.*"));
    }
}
