//! Action IR: the single action serialization all components speak.
//! Mirrors `contracts/action_ir.schema.json`.

use serde::{Deserialize, Serialize};

use crate::{Grounding, RiskClass};

fn default_v() -> u32 {
    1
}

/// One action step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Action {
    #[serde(default = "default_v")]
    pub v: u32,
    // The step id is internal bookkeeping, not a planner decision. A real model
    // routinely omits it, so tolerate a missing id (defaults to empty) rather
    // than failing to parse; the explore loop fills a deterministic per-run id
    // when it is blank. Fixtures and compiled workflows always carry an explicit
    // id, so the default never fires for them.
    #[serde(default)]
    pub id: String,
    pub kind: ActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub params: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub pace: Pace,
    // risk_class and grounding are metadata a planner should set (the tool schema
    // asks for them) but a real model may still omit. Tolerate that with the
    // enum's own Default (RiskClass::Write, Grounding::Uia) rather than halting;
    // the safety gates still evaluate the resulting action normally.
    #[serde(default)]
    pub risk_class: RiskClass,
    #[serde(default)]
    pub irreversible: bool,
    #[serde(default)]
    pub grounding: Grounding,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub retry: Retry,
}

fn default_timeout() -> u64 {
    5000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Click,
    Type,
    Key,
    Scroll,
    Drag,
    Wait,
    Assert,
    AdapterCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Pace {
    #[default]
    Instant,
    Human,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retry {
    #[serde(default = "default_attempts")]
    pub attempts: u32,
    #[serde(default = "default_backoff")]
    pub backoff_ms: u64,
}

fn default_attempts() -> u32 {
    2
}
fn default_backoff() -> u64 {
    250
}

impl Default for Retry {
    fn default() -> Self {
        Retry { attempts: 2, backoff_ms: 250 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Target {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowMatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<Selector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coords_last_known: Option<Coords>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub img_hash: String,
    pub tolerance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coords {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_scale: Option<f64>,
}

/// A selector alternative. Replay tries selectors in list order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Selector {
    AutomationId { value: String },
    NameRolePath { path: Vec<NameRoleSeg> },
    OrdinalPath { path: Vec<OrdinalSeg> },
    Css { value: String },
}

impl Selector {
    /// Stability score per compiler pass 3 (higher is more stable).
    pub fn score(&self) -> i32 {
        match self {
            Selector::AutomationId { .. } => 100,
            Selector::NameRolePath { path } => (60 - 5 * path.len() as i32).max(5),
            Selector::OrdinalPath { .. } => 20,
            Selector::Css { .. } => 80,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameRoleSeg {
    pub role: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinalSeg {
    pub role: String,
    pub ordinal: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_scores_prefer_automation_id() {
        let a = Selector::AutomationId { value: "x".into() };
        let n = Selector::NameRolePath { path: vec![NameRoleSeg { role: "window".into(), name: "w".into() }] };
        let o = Selector::OrdinalPath { path: vec![OrdinalSeg { role: "window".into(), ordinal: 0 }] };
        assert!(a.score() > n.score());
        assert!(n.score() > o.score());
    }

    #[test]
    fn action_roundtrips_minimal() {
        let json = r#"{"v":1,"id":"s1","kind":"key","params":{"combo":"ctrl+s"},"risk_class":"write","grounding":"uia"}"#;
        let a: Action = serde_json::from_str(json).unwrap();
        assert_eq!(a.kind, ActionKind::Key);
        assert_eq!(a.risk_class, RiskClass::Write);
        assert_eq!(a.timeout_ms, 5000);
        assert_eq!(a.retry.attempts, 2);
        let back = serde_json::to_value(&a).unwrap();
        let reparsed: Action = serde_json::from_value(back).unwrap();
        assert_eq!(a, reparsed);
    }
}
