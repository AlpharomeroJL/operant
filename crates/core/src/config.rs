//! Minimal config store (C1 scaffold). L1A backs this with a persisted store and
//! change events on the bus.

use std::collections::BTreeMap;

use parking_lot::RwLock;

#[derive(Default)]
pub struct Config {
    map: RwLock<BTreeMap<String, serde_json::Value>>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, key: &str, value: serde_json::Value) {
        self.map.write().insert(key.to_string(), value);
    }

    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.map.read().get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get() {
        let c = Config::new();
        c.set("model.planner", serde_json::json!("mock_planner"));
        assert_eq!(c.get("model.planner"), Some(serde_json::json!("mock_planner")));
        assert_eq!(c.get("missing"), None);
    }
}
