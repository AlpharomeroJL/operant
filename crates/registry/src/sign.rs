//! Publisher-side signing (L7B, `operant publish`): the mirror image of
//! `verify.rs`. Signs a manifest's canonical JSON (minus its own
//! `signature` key) with the publisher's Ed25519 private key, over exactly
//! the same bytes `verify_manifest_signature` checks.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};

use crate::canonical::{to_canonical_json, without_signature};
use crate::error::RegistryError;
use crate::manifest::ManifestSignature;

/// Fixed 16-byte ASN.1 prefix of a PKCS8 DER-encoded Ed25519 private key
/// (RFC 8410's `OneAsymmetricKey`, algorithm id 1.3.101.112): everything
/// before the trailing 32-byte raw seed. The same fixed shape
/// `contracts/fixtures/registry/publisher.key` is stored in, and what
/// Node's `crypto.createPrivateKey` accepts for an Ed25519 PEM.
const PKCS8_ED25519_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// Parse a PKCS8 PEM-encoded Ed25519 private key
/// (`contracts/fixtures/registry/publisher.key`'s format) into a signing
/// key. No general-purpose ASN.1 parser: Ed25519 PKCS8 keys are always this
/// exact 48-byte shape, so a fixed prefix check is exact and dependency-free.
pub fn parse_publisher_key_pem(pem: &str) -> Result<SigningKey, RegistryError> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .collect();
    let der = STANDARD
        .decode(&body)
        .map_err(|e| RegistryError::PrivateKeyInvalid(e.to_string()))?;
    if der.len() != 48 || der[..16] != PKCS8_ED25519_PREFIX {
        return Err(RegistryError::PrivateKeyInvalid(format!(
            "expected a 48-byte PKCS8 Ed25519 private key, got {} bytes",
            der.len()
        )));
    }
    let seed: [u8; 32] = der[16..]
        .try_into()
        .expect("length checked above: der.len() == 48");
    Ok(SigningKey::from_bytes(&seed))
}

/// Sign `manifest_value` (the manifest JSON, with or without an existing
/// `signature` key: it is stripped before signing either way) with `key`,
/// returning the `signature` block to attach.
pub fn sign_manifest(manifest_value: &serde_json::Value, key: &SigningKey) -> ManifestSignature {
    let message = to_canonical_json(&without_signature(manifest_value));
    let sig = key.sign(message.as_bytes());
    ManifestSignature {
        sig: STANDARD.encode(sig.to_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::{fingerprint, parse_publisher_key_hex, verify_manifest_signature};
    use crate::FetchedManifest;
    use crate::PinStore;

    const MANIFEST_JSON: &str = include_str!("../../../contracts/fixtures/registry/manifest.json");
    const PUBLISHER_KEY_PEM: &str = include_str!("../../../contracts/fixtures/registry/publisher.key");
    const PUBLISHER_PUB: &str = include_str!("../../../contracts/fixtures/registry/publisher.pub");
    const FINGERPRINT: &str = "e7f1a7f9ce2a6110cdc750301d5f47c6";

    #[test]
    fn parses_the_fixture_private_key_to_the_fixture_public_key() {
        let signing_key = parse_publisher_key_pem(PUBLISHER_KEY_PEM).expect("fixture key parses");
        let verifying_key = signing_key.verifying_key();
        let expected = parse_publisher_key_hex(PUBLISHER_PUB).unwrap();
        assert_eq!(verifying_key.to_bytes(), expected);
        assert_eq!(fingerprint(&verifying_key.to_bytes()), FINGERPRINT);
    }

    // BAR: signing the fixture manifest (minus its own signature) with the
    // fixture private key reproduces the exact signature already committed
    // in the fixture. Ed25519 is deterministic, so this is byte-for-byte,
    // not just "verifies".
    #[test]
    fn signing_the_fixture_manifest_reproduces_its_committed_signature() {
        let signing_key = parse_publisher_key_pem(PUBLISHER_KEY_PEM).unwrap();
        let value: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        let expected_sig = value["signature"]["sig"].as_str().unwrap();

        let produced = sign_manifest(&value, &signing_key);
        assert_eq!(produced.sig, expected_sig);
    }

    #[test]
    fn a_freshly_signed_draft_manifest_verifies() {
        let signing_key = parse_publisher_key_pem(PUBLISHER_KEY_PEM).unwrap();
        let verifying_key = signing_key.verifying_key();

        let mut value: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        value.as_object_mut().unwrap().remove("signature");
        let signature = sign_manifest(&value, &signing_key);
        value["signature"] = serde_json::json!({ "sig": signature.sig });

        let manifest = FetchedManifest::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let outcome =
            verify_manifest_signature(&manifest.manifest, &value, Some(&verifying_key.to_bytes()))
                .unwrap();
        assert!(matches!(outcome, crate::verify::SignatureOutcome::Valid { fingerprint } if fingerprint == FINGERPRINT));
    }

    #[test]
    fn a_tampered_field_after_signing_fails_verification() {
        let signing_key = parse_publisher_key_pem(PUBLISHER_KEY_PEM).unwrap();
        let verifying_key = signing_key.verifying_key();

        let mut value: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        value.as_object_mut().unwrap().remove("signature");
        let signature = sign_manifest(&value, &signing_key);
        value["signature"] = serde_json::json!({ "sig": signature.sig });
        value["description"] = serde_json::json!("a different description entirely");

        let manifest = FetchedManifest::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let mut pins = PinStore::new();
        let err = manifest
            .verify_signature(Some(&verifying_key.to_bytes()), &mut pins)
            .expect_err("a field changed after signing must fail verification");
        assert!(matches!(err, RegistryError::SignatureInvalid { .. }));
    }

    #[test]
    fn rejects_garbage_pem() {
        let err = parse_publisher_key_pem("-----BEGIN PRIVATE KEY-----\nbm90IGEga2V5\n-----END PRIVATE KEY-----\n")
            .expect_err("not a real key");
        assert!(matches!(err, RegistryError::PrivateKeyInvalid(_)));
    }
}
