//! Config store (C1): in-memory access, JSON file persistence, and a
//! `config.changed` bus event on every `set`.
//!
//! `config.changed` is not yet in `contracts/bus_events.md`; it is a new topic,
//! which the contract's versioning rules allow ("new topics may be added
//! freely"). Documenting it in the contract is out of this lane's owned paths
//! (`crates/core` only); flagged as a followup for whoever owns `contracts/`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::bus::events::BusEvent;
use crate::bus::Bus;

/// `config.changed`: key, value, old_value?. Published by [`Config::set`]
/// whenever a bus is attached via [`Config::with_bus`] or [`Config::attach_bus`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigChanged {
    pub key: String,
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_value: Option<serde_json::Value>,
}
impl BusEvent for ConfigChanged {
    const TOPIC: &'static str = "config.changed";
}

/// Config load/save/set errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Default)]
pub struct Config {
    map: RwLock<BTreeMap<String, serde_json::Value>>,
    /// Set via [`Config::with_bus`]/[`Config::attach_bus`]; `None` means `set`
    /// only updates memory, matching the original scaffold behavior.
    bus: RwLock<Option<Arc<Bus>>>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    /// A config store that publishes `config.changed` on every `set`.
    pub fn with_bus(bus: Arc<Bus>) -> Self {
        let cfg = Self::new();
        cfg.attach_bus(bus);
        cfg
    }

    /// Attach (or replace) the bus used to publish `config.changed`. Additive:
    /// existing callers that never attach a bus keep the original in-memory-only
    /// behavior.
    pub fn attach_bus(&self, bus: Arc<Bus>) {
        *self.bus.write() = Some(bus);
    }

    /// Set `key`, replacing any existing value. Publishes `config.changed`
    /// (with `old_value` set if one existed) when a bus is attached.
    pub fn set(&self, key: &str, value: serde_json::Value) {
        let old_value = self.map.write().insert(key.to_string(), value.clone());
        if let Some(bus) = self.bus.read().as_ref() {
            bus.publish_event(&ConfigChanged {
                key: key.to_string(),
                value,
                old_value,
            })
            .expect("ConfigChanged always serializes");
        }
    }

    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.map.read().get(key).cloned()
    }

    /// All keys currently set. Mainly for persistence and diagnostics.
    pub fn snapshot(&self) -> BTreeMap<String, serde_json::Value> {
        self.map.read().clone()
    }

    /// Load a config store from a JSON file (an object of key/value pairs). No
    /// bus is attached; call [`Config::attach_bus`] afterward if needed.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let data = std::fs::read_to_string(path)?;
        let map: BTreeMap<String, serde_json::Value> = serde_json::from_str(&data)?;
        Ok(Config {
            map: RwLock::new(map),
            bus: RwLock::new(None),
        })
    }

    /// Load `path` if it exists, else start with an empty store. Convenient for
    /// first-run: a missing config file is not an error.
    pub fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        match Self::load(path) {
            Ok(cfg) => Ok(cfg),
            Err(ConfigError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(e),
        }
    }

    /// Save the current in-memory store to `path` as pretty JSON, creating parent
    /// directories if needed. Does not persist the attached bus (a `Bus` is a
    /// runtime handle, not config data).
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let data = serde_json::to_string_pretty(&*self.map.read())?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get() {
        let c = Config::new();
        c.set("model.planner", serde_json::json!("mock_planner"));
        assert_eq!(
            c.get("model.planner"),
            Some(serde_json::json!("mock_planner"))
        );
        assert_eq!(c.get("missing"), None);
    }

    #[test]
    fn set_without_bus_does_not_publish() {
        // Regression guard: attaching no bus must not panic and must leave the
        // original scaffold behavior (in-memory only) intact.
        let c = Config::new();
        c.set("k", serde_json::json!(1));
        assert_eq!(c.get("k"), Some(serde_json::json!(1)));
    }

    #[test]
    fn set_with_bus_publishes_config_changed() {
        let bus = Arc::new(Bus::new());
        let c = Config::with_bus(bus.clone());
        let sub = bus.subscribe("config.changed");

        c.set("model.planner", serde_json::json!("mock_planner"));
        let env = sub.rx.try_recv().expect("config.changed published");
        assert_eq!(env.topic, "config.changed");
        let payload: ConfigChanged = serde_json::from_value(env.payload).unwrap();
        assert_eq!(payload.key, "model.planner");
        assert_eq!(payload.value, serde_json::json!("mock_planner"));
        assert_eq!(payload.old_value, None);

        c.set("model.planner", serde_json::json!("real_planner"));
        let env2 = sub.rx.try_recv().expect("second config.changed published");
        let payload2: ConfigChanged = serde_json::from_value(env2.payload).unwrap();
        assert_eq!(payload2.old_value, Some(serde_json::json!("mock_planner")));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir =
            std::env::temp_dir().join(format!("operant-core-config-test-{}", std::process::id()));
        let path = dir.join("config.json");

        let c = Config::new();
        c.set("model.planner", serde_json::json!("mock_planner"));
        c.set("voice.enabled", serde_json::json!(false));
        c.save(&path).expect("save succeeds, creating parent dirs");

        let loaded = Config::load(&path).expect("load succeeds");
        assert_eq!(
            loaded.get("model.planner"),
            Some(serde_json::json!("mock_planner"))
        );
        assert_eq!(loaded.get("voice.enabled"), Some(serde_json::json!(false)));
        assert_eq!(loaded.snapshot().len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_or_default_tolerates_missing_file() {
        let path =
            std::env::temp_dir().join(format!("operant-core-missing-{}.json", std::process::id()));
        std::fs::remove_file(&path).ok();
        let cfg = Config::load_or_default(&path).expect("missing file is not an error");
        assert_eq!(cfg.snapshot().len(), 0);
    }
}
