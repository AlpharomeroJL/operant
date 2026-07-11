//! Staged install: parse, verify signature, verify the DSL hash, render
//! grants, require approval, then store. Each stage is its own type so a
//! caller cannot reach `store` without having gone through the earlier
//! stages, and cannot reach `decide` without a rendering already having been
//! produced for the user to read.
//!
//! Unsigned manifests and manifests from a publisher pinned for the first
//! time both install successfully but are flagged dry-run-only, per
//! `docs/specs/registry.md`: "Unsigned or unpinned: install allowed but the
//! workflow is flagged and executes in dry-run only until the user
//! explicitly promotes it after reading the steps."

use operant_ir::RiskClass;
use serde::Serialize;

use crate::error::RegistryError;
use crate::manifest::RegistryManifest;
use crate::pin::{PinOutcome, PinStore};
use crate::verify::{verify_manifest_signature, SignatureOutcome};

/// A manifest that has been parsed but not yet cryptographically checked.
#[derive(Debug)]
pub struct FetchedManifest {
    pub manifest: RegistryManifest,
    raw: serde_json::Value,
}

impl FetchedManifest {
    /// Parse a registry manifest from its wire JSON. Keeps the original
    /// parsed value around so signature verification runs over exactly what
    /// was signed, not a value re-serialized through `RegistryManifest`.
    pub fn parse(manifest_json: &[u8]) -> Result<Self, RegistryError> {
        let raw: serde_json::Value = serde_json::from_slice(manifest_json)?;
        let manifest: RegistryManifest = serde_json::from_value(raw.clone())?;
        Ok(Self { manifest, raw })
    }

    /// Stage 1: verify the Ed25519 signature (or record that the manifest is
    /// unsigned) and run pin-on-first-use bookkeeping for signed manifests.
    pub fn verify_signature(
        self,
        publisher_key: Option<&[u8]>,
        pins: &mut PinStore,
    ) -> Result<SignedManifest, RegistryError> {
        let outcome = verify_manifest_signature(&self.manifest, &self.raw, publisher_key)?;
        let trust = match outcome {
            SignatureOutcome::Unsigned => Trust::Unsigned,
            SignatureOutcome::Valid { fingerprint } => {
                match pins.observe(&self.manifest.publisher, &fingerprint)? {
                    PinOutcome::FirstUse => Trust::FirstUse,
                    PinOutcome::Trusted => Trust::Trusted,
                }
            }
        };
        Ok(SignedManifest {
            manifest: self.manifest,
            trust,
        })
    }
}

/// Trust state of a manifest after stage 1, driving whether the eventual
/// install is forced dry-run-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// No signature block at all.
    Unsigned,
    /// Signed, and this is the first time the publisher has been observed.
    FirstUse,
    /// Signed by a publisher already pinned to a matching fingerprint.
    Trusted,
}

impl Trust {
    pub fn forces_dry_run(self) -> bool {
        matches!(self, Trust::Unsigned | Trust::FirstUse)
    }
}

#[derive(Debug)]
pub struct SignedManifest {
    pub manifest: RegistryManifest,
    pub trust: Trust,
}

impl SignedManifest {
    /// Stage 2: verify the fetched DSL bytes hash (BLAKE3) to `manifest.dsl.hash`.
    pub fn verify_dsl(self, dsl_bytes: Vec<u8>) -> Result<DslVerified, RegistryError> {
        let actual = crate::hex::encode(blake3::hash(&dsl_bytes).as_bytes());
        if actual != self.manifest.dsl.hash {
            return Err(RegistryError::DslHashMismatch {
                name: self.manifest.name.clone(),
                version: self.manifest.version.clone(),
                expected: self.manifest.dsl.hash.clone(),
                actual,
            });
        }
        Ok(DslVerified {
            manifest: self.manifest,
            trust: self.trust,
            dsl_bytes,
        })
    }
}

#[derive(Debug)]
pub struct DslVerified {
    pub manifest: RegistryManifest,
    pub trust: Trust,
    dsl_bytes: Vec<u8>,
}

impl DslVerified {
    /// Stage 3: render the plain-language step summary and capability grants
    /// an approval surface must show before install can proceed.
    pub fn render(self) -> RenderedInstall {
        let rendering = GrantRendering::new(&self.manifest, self.trust);
        RenderedInstall {
            manifest: self.manifest,
            trust: self.trust,
            dsl_bytes: self.dsl_bytes,
            rendering,
        }
    }
}

/// Plain-language rendering of what a manifest asks for. Produced before
/// approval is requested so the caller has something to show a human.
#[derive(Debug, Clone, Serialize)]
pub struct GrantRendering {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub step_summary: Vec<String>,
    pub grants: Vec<String>,
    pub trust_note: String,
}

impl GrantRendering {
    fn new(manifest: &RegistryManifest, trust: Trust) -> Self {
        let mut grants = Vec::new();
        for app in &manifest.capabilities.apps {
            grants.push(format!("Run {app}"));
        }
        for path in &manifest.capabilities.paths {
            grants.push(format!("Access files under {path}"));
        }
        if manifest.capabilities.network {
            grants.push("Connect to the network".to_string());
        }
        let risk = match manifest.capabilities.risk_ceiling {
            RiskClass::Read => "read",
            RiskClass::Write => "write",
            RiskClass::Destructive => "destructive",
        };
        grants.push(format!("Risk ceiling: {risk}"));

        let trust_note = match trust {
            Trust::Unsigned => {
                "Unsigned: this workflow installs dry-run-only until you promote it.".to_string()
            }
            Trust::FirstUse => format!(
                "First time seeing publisher '{}': installs dry-run-only until you promote it.",
                manifest.publisher
            ),
            Trust::Trusted => format!("Publisher '{}' is already pinned.", manifest.publisher),
        };

        Self {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            publisher: manifest.publisher.clone(),
            step_summary: manifest.step_summary.clone(),
            grants,
            trust_note,
        }
    }
}

#[derive(Debug)]
pub struct RenderedInstall {
    pub manifest: RegistryManifest,
    pub trust: Trust,
    dsl_bytes: Vec<u8>,
    pub rendering: GrantRendering,
}

/// A human's explicit answer to "install this?" after reading `GrantRendering`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Approved,
    Denied,
}

impl RenderedInstall {
    /// Stage 4: require an explicit approval decision before storing.
    pub fn decide(self, decision: Approval) -> Result<ApprovedInstall, RegistryError> {
        match decision {
            Approval::Approved => Ok(ApprovedInstall {
                dry_run: self.trust.forces_dry_run(),
                manifest: self.manifest,
                dsl_bytes: self.dsl_bytes,
            }),
            Approval::Denied => Err(RegistryError::NotApproved {
                name: self.manifest.name.clone(),
                version: self.manifest.version.clone(),
            }),
        }
    }
}

#[derive(Debug)]
pub struct ApprovedInstall {
    manifest: RegistryManifest,
    dry_run: bool,
    dsl_bytes: Vec<u8>,
}

impl ApprovedInstall {
    /// Stage 5: persist the installed workflow.
    pub fn store(self, store: &mut dyn InstallStore) -> Result<InstalledWorkflow, RegistryError> {
        let record = InstalledWorkflow {
            name: self.manifest.name.clone(),
            version: self.manifest.version.clone(),
            publisher: self.manifest.publisher.clone(),
            dry_run: self.dry_run,
            manifest: self.manifest,
            dsl_bytes: self.dsl_bytes,
        };
        store.save(&record)?;
        Ok(record)
    }
}

#[derive(Debug, Clone)]
pub struct InstalledWorkflow {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub dry_run: bool,
    pub manifest: RegistryManifest,
    pub dsl_bytes: Vec<u8>,
}

/// Where an approved install lands. Deliberately minimal: this crate owns
/// the install pipeline, not the persistence layer a full build ends up
/// using, so callers plug in their own store.
pub trait InstallStore {
    fn save(&mut self, record: &InstalledWorkflow) -> Result<(), RegistryError>;
    fn get(&self, name: &str, version: &str) -> Option<InstalledWorkflow>;
}

/// In-memory store: tests, and any caller that only needs process lifetime.
#[derive(Debug, Default)]
pub struct MemoryStore {
    items: std::collections::HashMap<(String, String), InstalledWorkflow>,
}

impl InstallStore for MemoryStore {
    fn save(&mut self, record: &InstalledWorkflow) -> Result<(), RegistryError> {
        self.items.insert(
            (record.name.clone(), record.version.clone()),
            record.clone(),
        );
        Ok(())
    }

    fn get(&self, name: &str, version: &str) -> Option<InstalledWorkflow> {
        self.items
            .get(&(name.to_string(), version.to_string()))
            .cloned()
    }
}

/// Filesystem store: `<root>/<name>/<version>/{manifest.json,dsl,state.json}`.
pub struct FsStore {
    root: std::path::PathBuf,
}

impl FsStore {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn dir_for(&self, name: &str, version: &str) -> std::path::PathBuf {
        self.root.join(name).join(version)
    }
}

impl InstallStore for FsStore {
    fn save(&mut self, record: &InstalledWorkflow) -> Result<(), RegistryError> {
        let dir = self.dir_for(&record.name, &record.version);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&record.manifest)?,
        )?;
        std::fs::write(dir.join("dsl"), &record.dsl_bytes)?;
        let state = serde_json::json!({ "dry_run": record.dry_run, "publisher": record.publisher });
        std::fs::write(dir.join("state.json"), serde_json::to_vec_pretty(&state)?)?;
        Ok(())
    }

    fn get(&self, name: &str, version: &str) -> Option<InstalledWorkflow> {
        let dir = self.dir_for(name, version);
        let manifest: RegistryManifest =
            serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).ok()?).ok()?;
        let dsl_bytes = std::fs::read(dir.join("dsl")).ok()?;
        let state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("state.json")).ok()?).ok()?;
        Some(InstalledWorkflow {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            publisher: manifest.publisher.clone(),
            dry_run: state.get("dry_run")?.as_bool()?,
            manifest,
            dsl_bytes,
        })
    }
}

/// One-shot install matching the spec's flow end to end: verify, hash-check,
/// render, and (if `approval` is granted) store. Real approval surfaces
/// should call the staged API directly instead, so `rendering` can be shown
/// to a human between the hash check and `decide`; this exists for
/// programmatic callers that already have an approval decision in hand.
pub fn install(
    manifest_json: &[u8],
    publisher_key: Option<&[u8]>,
    dsl_bytes: Vec<u8>,
    pins: &mut PinStore,
    approval: Approval,
    store: &mut dyn InstallStore,
) -> Result<InstalledWorkflow, RegistryError> {
    FetchedManifest::parse(manifest_json)?
        .verify_signature(publisher_key, pins)?
        .verify_dsl(dsl_bytes)?
        .render()
        .decide(approval)?
        .store(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::parse_publisher_key_hex;

    const MANIFEST_JSON: &str = include_str!("../../../contracts/fixtures/registry/manifest.json");
    const PUBLISHER_PUB: &str = include_str!("../../../contracts/fixtures/registry/publisher.pub");
    const DSL_BYTES: &[u8] =
        include_bytes!("../../../contracts/fixtures/workflow_notepad/workflow.ts");
    const FINGERPRINT: &str = "e7f1a7f9ce2a6110cdc750301d5f47c6";

    fn publisher_key() -> [u8; 32] {
        parse_publisher_key_hex(PUBLISHER_PUB).expect("fixture key parses")
    }

    // BAR (a): the fixture manifest verifies against publisher.pub. A fresh
    // pin store has never seen `operant-fixtures` before, so pin-on-first-use
    // pins it now and the install lands dry-run-only.
    #[test]
    fn fixture_manifest_verifies_first_use_is_dry_run() {
        let key = publisher_key();
        let mut pins = PinStore::new();
        let installed = install(
            MANIFEST_JSON.as_bytes(),
            Some(&key),
            DSL_BYTES.to_vec(),
            &mut pins,
            Approval::Approved,
            &mut MemoryStore::default(),
        )
        .expect("fixture install verifies");
        assert!(
            installed.dry_run,
            "first use of a publisher must be dry-run-only"
        );
        assert_eq!(pins.fingerprint_for("operant-fixtures"), Some(FINGERPRINT));
    }

    #[test]
    fn already_pinned_publisher_installs_live() {
        let key = publisher_key();
        let mut pins = PinStore::new();
        pins.observe("operant-fixtures", FINGERPRINT).unwrap();
        let installed = install(
            MANIFEST_JSON.as_bytes(),
            Some(&key),
            DSL_BYTES.to_vec(),
            &mut pins,
            Approval::Approved,
            &mut MemoryStore::default(),
        )
        .expect("fixture install verifies");
        assert!(
            !installed.dry_run,
            "an already-pinned publisher is not forced dry-run"
        );
    }

    // BAR (b): a tampered manifest (flip a byte) is rejected.
    #[test]
    fn tampered_manifest_is_rejected() {
        let key = publisher_key();
        let mut pins = PinStore::new();
        let tampered = MANIFEST_JSON.replacen(
            "Writes a dated invoice note",
            "Xrites a dated invoice note",
            1,
        );
        assert_ne!(
            tampered, MANIFEST_JSON,
            "sanity check: the byte was actually flipped"
        );
        let err = FetchedManifest::parse(tampered.as_bytes())
            .expect("still valid JSON")
            .verify_signature(Some(&key), &mut pins)
            .expect_err("tampered manifest must fail verification");
        assert!(matches!(err, RegistryError::SignatureInvalid { .. }));
    }

    // BAR (c): an unsigned manifest installs as dry-run-only.
    #[test]
    fn unsigned_manifest_installs_dry_run_only() {
        let mut value: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        value.as_object_mut().unwrap().remove("signature");
        let unsigned = serde_json::to_vec(&value).unwrap();

        let mut pins = PinStore::new();
        let installed = install(
            &unsigned,
            None,
            DSL_BYTES.to_vec(),
            &mut pins,
            Approval::Approved,
            &mut MemoryStore::default(),
        )
        .expect("unsigned manifests install");
        assert!(
            installed.dry_run,
            "unsigned workflows are always dry-run-only"
        );
        assert!(
            pins.fingerprint_for("operant-fixtures").is_none(),
            "no pin activity for an unsigned install"
        );
    }

    // BAR (d): a DSL whose bytes do not match dsl.hash is rejected.
    #[test]
    fn dsl_hash_mismatch_is_rejected() {
        let key = publisher_key();
        let mut pins = PinStore::new();
        let signed = FetchedManifest::parse(MANIFEST_JSON.as_bytes())
            .unwrap()
            .verify_signature(Some(&key), &mut pins)
            .expect("fixture signature verifies");
        let err = signed
            .verify_dsl(b"not the real workflow bytes".to_vec())
            .expect_err("wrong bytes must fail the hash check");
        assert!(matches!(err, RegistryError::DslHashMismatch { .. }));
    }

    #[test]
    fn install_without_approval_is_rejected() {
        let key = publisher_key();
        let mut pins = PinStore::new();
        let err = FetchedManifest::parse(MANIFEST_JSON.as_bytes())
            .unwrap()
            .verify_signature(Some(&key), &mut pins)
            .unwrap()
            .verify_dsl(DSL_BYTES.to_vec())
            .unwrap()
            .render()
            .decide(Approval::Denied)
            .expect_err("denied approval must not proceed");
        assert!(matches!(err, RegistryError::NotApproved { .. }));
    }

    #[test]
    fn rendering_lists_capabilities_and_trust_note() {
        let key = publisher_key();
        let mut pins = PinStore::new();
        let rendered = FetchedManifest::parse(MANIFEST_JSON.as_bytes())
            .unwrap()
            .verify_signature(Some(&key), &mut pins)
            .unwrap()
            .verify_dsl(DSL_BYTES.to_vec())
            .unwrap()
            .render();
        assert!(rendered
            .rendering
            .grants
            .iter()
            .any(|g| g.contains("notepad.exe")));
        assert!(rendered
            .rendering
            .grants
            .iter()
            .any(|g| g.contains("write")));
        assert!(rendered.rendering.trust_note.contains("First time"));
        assert_eq!(rendered.rendering.step_summary.len(), 6);
    }

    #[test]
    fn fs_store_round_trips() {
        let key = publisher_key();
        let mut pins = PinStore::new();
        let dir = std::env::temp_dir().join("operant-registry-fs-store-test");
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = FsStore::new(&dir);

        let installed = install(
            MANIFEST_JSON.as_bytes(),
            Some(&key),
            DSL_BYTES.to_vec(),
            &mut pins,
            Approval::Approved,
            &mut store,
        )
        .expect("fixture install verifies");

        let reloaded = store
            .get(&installed.name, &installed.version)
            .expect("stored record reloads");
        assert_eq!(reloaded.name, installed.name);
        assert_eq!(reloaded.dry_run, installed.dry_run);
        assert_eq!(reloaded.dsl_bytes, DSL_BYTES);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
