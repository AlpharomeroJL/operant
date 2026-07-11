//! Ed25519 signature verification over canonical JSON, and the pin-on-first-use
//! fingerprint used to identify a publisher key without shipping the raw key
//! inside the manifest itself.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::canonical::{to_canonical_json, without_signature};
use crate::error::RegistryError;
use crate::manifest::RegistryManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureOutcome {
    /// `manifest.signature` was absent or null. Not a rejection: unsigned
    /// workflows are a supported install target, always dry-run-only.
    Unsigned,
    /// The signature verified against the supplied publisher key. Carries
    /// the key's fingerprint so the caller can drive pin-on-first-use.
    Valid { fingerprint: String },
}

/// BLAKE3 of the raw Ed25519 public key, truncated to 16 bytes, hex-encoded.
/// Matches `contracts/fixtures/generate.mjs`: `bytesToHex(blake3(rawPub, { dkLen: 16 }))`.
/// BLAKE3's extendable output is a prefix of its default hash, so truncating
/// the standard 32-byte hash is equivalent to asking the XOF for 16 bytes.
pub fn fingerprint(pubkey: &[u8]) -> String {
    let hash = blake3::hash(pubkey);
    crate::hex::encode(&hash.as_bytes()[..16])
}

/// Parse a publisher public key from its on-disk representation
/// (`contracts/fixtures/registry/publisher.pub`: raw 32-byte Ed25519 key,
/// lowercase hex, one line).
pub fn parse_publisher_key_hex(s: &str) -> Result<[u8; 32], RegistryError> {
    let bytes = crate::hex::decode(s)?;
    let len = bytes.len();
    bytes
        .try_into()
        .map_err(|_| RegistryError::InvalidPublisherKeyLength(len))
}

/// Verify `manifest`'s signature.
///
/// `raw` is the manifest as originally parsed (before it was mapped into
/// `RegistryManifest`): verification must run over exactly the bytes the
/// publisher signed, not a value re-serialized through our own struct, so a
/// field our schema does not know about cannot silently escape the check.
///
/// Returns `Ok(SignatureOutcome::Unsigned)` when there is nothing to verify.
/// Returns `Err` for every mismatch: missing key material, a supplied key
/// that does not fingerprint to `pubkey_fingerprint`, or a signature that
/// does not verify. Never falls back to "probably fine".
pub fn verify_manifest_signature(
    manifest: &RegistryManifest,
    raw: &serde_json::Value,
    publisher_key: Option<&[u8]>,
) -> Result<SignatureOutcome, RegistryError> {
    let Some(signature) = &manifest.signature else {
        return Ok(SignatureOutcome::Unsigned);
    };

    let key_bytes = publisher_key.ok_or_else(|| RegistryError::PublisherKeyMissing {
        publisher: manifest.publisher.clone(),
    })?;

    let actual_fingerprint = fingerprint(key_bytes);
    if actual_fingerprint != manifest.pubkey_fingerprint {
        return Err(RegistryError::PublisherKeyMismatch {
            expected: manifest.pubkey_fingerprint.clone(),
            actual: actual_fingerprint,
        });
    }

    let key_len = key_bytes.len();
    let key_arr: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| RegistryError::InvalidPublisherKeyLength(key_len))?;
    let verifying_key = VerifyingKey::from_bytes(&key_arr)
        .map_err(|_| RegistryError::InvalidPublisherKeyLength(key_len))?;

    let sig_bytes = STANDARD
        .decode(&signature.sig)
        .map_err(|e| RegistryError::InvalidSignatureEncoding(e.to_string()))?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| RegistryError::InvalidSignatureLength)?;
    let sig = Signature::from_bytes(&sig_arr);

    let message = to_canonical_json(&without_signature(raw));
    verifying_key
        .verify(message.as_bytes(), &sig)
        .map_err(|_| RegistryError::SignatureInvalid {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
        })?;

    Ok(SignatureOutcome::Valid {
        fingerprint: actual_fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST_JSON: &str = include_str!("../../../contracts/fixtures/registry/manifest.json");
    const PUBLISHER_PUB: &str = include_str!("../../../contracts/fixtures/registry/publisher.pub");

    fn publisher_key() -> [u8; 32] {
        parse_publisher_key_hex(PUBLISHER_PUB).expect("fixture key parses")
    }

    #[test]
    fn fingerprint_matches_fixture() {
        assert_eq!(
            fingerprint(&publisher_key()),
            "e7f1a7f9ce2a6110cdc750301d5f47c6"
        );
    }

    #[test]
    fn fixture_signature_is_valid() {
        let raw: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        let manifest: RegistryManifest = serde_json::from_value(raw.clone()).unwrap();
        let outcome = verify_manifest_signature(&manifest, &raw, Some(&publisher_key())).unwrap();
        assert!(
            matches!(outcome, SignatureOutcome::Valid { fingerprint } if fingerprint == "e7f1a7f9ce2a6110cdc750301d5f47c6")
        );
    }

    #[test]
    fn wrong_key_is_rejected_before_touching_the_signature() {
        let raw: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        let manifest: RegistryManifest = serde_json::from_value(raw.clone()).unwrap();
        let wrong_key = [7u8; 32];
        let err = verify_manifest_signature(&manifest, &raw, Some(&wrong_key)).unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyMismatch { .. }));
    }

    #[test]
    fn missing_key_for_a_signed_manifest_is_an_error() {
        let raw: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        let manifest: RegistryManifest = serde_json::from_value(raw.clone()).unwrap();
        let err = verify_manifest_signature(&manifest, &raw, None).unwrap_err();
        assert!(matches!(err, RegistryError::PublisherKeyMissing { .. }));
    }

    #[test]
    fn no_signature_block_is_unsigned_not_an_error() {
        let mut raw: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        raw.as_object_mut().unwrap().remove("signature");
        let manifest: RegistryManifest = serde_json::from_value(raw.clone()).unwrap();
        let outcome = verify_manifest_signature(&manifest, &raw, None).unwrap();
        assert_eq!(outcome, SignatureOutcome::Unsigned);
    }
}
