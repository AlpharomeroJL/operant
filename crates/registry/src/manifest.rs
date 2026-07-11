//! Registry manifest types. Mirrors `contracts/registry_manifest.schema.json`
//! (a superset of `workflow_manifest.schema.json` with publisher metadata and
//! a hash-pinned DSL fetch URL).
//!
//! `signature` is modeled as optional even though the schema marks it
//! required for a published index entry: a compiled workflow manifest with
//! no signature at all (`contracts/workflow_manifest.schema.json`, where
//! `signature` is explicitly nullable "for unsigned workflows, which run
//! dry-run only when installed from outside") is a legitimate, supported
//! install target for this crate. See `crate::install`.

use serde::{Deserialize, Serialize};

use operant_ir::Capabilities;

fn default_v() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryManifest {
    #[serde(default = "default_v")]
    pub v: u32,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub pubkey_fingerprint: String,
    pub description: String,
    pub step_summary: Vec<String>,
    pub inputs_schema: serde_json::Value,
    pub capabilities: Capabilities,
    pub min_operant_version: String,
    pub dsl: DslPin,
    #[serde(default)]
    pub signature: Option<ManifestSignature>,
}

/// Hash-pinned DSL fetch location, per `registry_manifest.schema.json#/properties/dsl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DslPin {
    pub url: String,
    pub hash: String,
}

/// Base64 Ed25519 signature by the publisher key over canonical JSON minus
/// this block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSignature {
    pub sig: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../contracts/fixtures/registry/manifest.json");

    #[test]
    fn fixture_manifest_parses() {
        let m: RegistryManifest = serde_json::from_str(FIXTURE).expect("manifest parses");
        assert_eq!(m.name, "notepad-invoice-note");
        assert_eq!(m.publisher, "operant-fixtures");
        assert_eq!(m.pubkey_fingerprint, "e7f1a7f9ce2a6110cdc750301d5f47c6");
        assert_eq!(m.dsl.url, "workflow_notepad/workflow.ts");
        assert_eq!(m.dsl.hash.len(), 64);
        assert!(m.signature.is_some());
    }

    #[test]
    fn missing_signature_parses_as_unsigned() {
        let mut value: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        value.as_object_mut().unwrap().remove("signature");
        let m: RegistryManifest = serde_json::from_value(value).expect("still parses");
        assert!(m.signature.is_none());
    }
}
