//! Workflow manifest types. Mirrors `contracts/workflow_manifest.schema.json`.

use serde::{Deserialize, Serialize};

use crate::{Gate, RiskClass};

fn default_v() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default = "default_v")]
    pub v: u32,
    pub name: String,
    pub version: String,
    pub description: String,
    pub step_summary: Vec<String>,
    pub inputs_schema: serde_json::Value,
    pub capabilities: Capabilities,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<Gate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_operant_version: Option<String>,
    #[serde(default)]
    pub source_run_id: Option<String>,
    pub dsl: DslRef,
    #[serde(default)]
    pub signature: Option<Signature>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default)]
    pub network: bool,
    pub risk_ceiling: RiskClass,
}

impl Capabilities {
    /// The runtime-enforced intersection used by workflow composition (X6):
    /// the effective capability set of a parent calling a child is the intersection.
    pub fn intersect(&self, other: &Capabilities) -> Capabilities {
        let apps = self.apps.iter().filter(|a| other.apps.contains(a)).cloned().collect();
        let paths = self.paths.iter().filter(|p| other.paths.contains(p)).cloned().collect();
        Capabilities {
            apps,
            paths,
            network: self.network && other.network,
            risk_ceiling: self.risk_ceiling.min(other.risk_ceiling),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DslRef {
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub publisher: String,
    pub pubkey_fingerprint: String,
    pub sig: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_fixture_parses() {
        let raw = include_str!("../../../contracts/fixtures/workflow_notepad/manifest.json");
        let m: Manifest = serde_json::from_str(raw).expect("manifest parses");
        assert_eq!(m.name, "notepad-invoice-note");
        assert_eq!(m.capabilities.risk_ceiling, RiskClass::Write);
        assert_eq!(m.step_summary.len(), 6);
        assert_eq!(m.dsl.hash.len(), 64);
    }

    #[test]
    fn capability_intersection_narrows() {
        let parent = Capabilities {
            apps: vec!["notepad.exe".into(), "chrome.exe".into()],
            paths: vec!["C:/Downloads".into()],
            network: true,
            risk_ceiling: RiskClass::Destructive,
        };
        let child = Capabilities {
            apps: vec!["notepad.exe".into()],
            paths: vec![],
            network: false,
            risk_ceiling: RiskClass::Write,
        };
        let eff = parent.intersect(&child);
        assert_eq!(eff.apps, vec!["notepad.exe".to_string()]);
        assert!(eff.paths.is_empty());
        assert!(!eff.network);
        assert_eq!(eff.risk_ceiling, RiskClass::Write);
    }
}
