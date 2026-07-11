//! Registry client (C16): parse a registry manifest, verify its Ed25519
//! signature over canonical JSON against a pinned publisher key, and run a
//! staged install: signature, then DSL hash, then rendered grants, then
//! explicit approval, then store. Unsigned manifests and manifests from a
//! publisher seen for the first time install successfully but are flagged
//! dry-run-only until explicitly promoted, per `docs/specs/registry.md`.
//! R1B implements install; L7B adds publish.
//!
//! ```
//! use operant_registry::{install, parse_publisher_key_hex, Approval, MemoryStore, PinStore};
//!
//! let manifest_json = include_bytes!("../../../contracts/fixtures/registry/manifest.json");
//! let publisher_pub = include_str!("../../../contracts/fixtures/registry/publisher.pub");
//! let dsl = include_bytes!("../../../contracts/fixtures/workflow_notepad/workflow.ts");
//! let key = parse_publisher_key_hex(publisher_pub).unwrap();
//!
//! let mut pins = PinStore::new();
//! let mut store = MemoryStore::default();
//! let installed = install(manifest_json, Some(&key), dsl.to_vec(), &mut pins, Approval::Approved, &mut store).unwrap();
//! // First time this publisher is seen, the install lands dry-run-only.
//! assert!(installed.dry_run);
//! ```

mod canonical;
mod error;
mod hex;
mod install;
mod manifest;
mod pin;
mod verify;

pub use canonical::to_canonical_json;
pub use error::RegistryError;
pub use install::{
    install, Approval, ApprovedInstall, DslVerified, FetchedManifest, FsStore, GrantRendering,
    InstallStore, InstalledWorkflow, MemoryStore, RenderedInstall, SignedManifest, Trust,
};
pub use manifest::{DslPin, ManifestSignature, RegistryManifest};
pub use pin::{PinOutcome, PinStore};
pub use verify::{
    fingerprint, parse_publisher_key_hex, verify_manifest_signature, SignatureOutcome,
};

/// Crate marker used by the workspace smoke test.
pub const CRATE: &str = "operant-registry";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_present() {
        assert_eq!(super::CRATE, "operant-registry");
    }
}
