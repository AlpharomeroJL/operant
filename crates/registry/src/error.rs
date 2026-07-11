//! Typed errors for the registry client. Every signature or hash mismatch in
//! `crate::install` resolves to one of these variants; nothing aborts with a
//! bare string.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("value is not valid hex: {0}")]
    InvalidHex(String),

    #[error("publisher key must be exactly 32 bytes, got {0}")]
    InvalidPublisherKeyLength(usize),

    #[error("signature is not valid base64: {0}")]
    InvalidSignatureEncoding(String),

    #[error("signature must be exactly 64 bytes")]
    InvalidSignatureLength,

    #[error("manifest for publisher {publisher} is signed but no publisher key was supplied")]
    PublisherKeyMissing { publisher: String },

    #[error(
        "manifest declares pubkey_fingerprint {expected} but the supplied publisher key fingerprints to {actual}"
    )]
    PublisherKeyMismatch { expected: String, actual: String },

    #[error("Ed25519 signature verification failed for {name}@{version}")]
    SignatureInvalid { name: String, version: String },

    #[error(
        "publisher {publisher} is pinned to {pinned} but this manifest presents {presented}; refusing to trust a rotated key silently"
    )]
    PublisherKeyRotated {
        publisher: String,
        pinned: String,
        presented: String,
    },

    #[error(
        "dsl hash mismatch for {name}@{version}: manifest declares {expected}, fetched bytes hash to {actual}"
    )]
    DslHashMismatch {
        name: String,
        version: String,
        expected: String,
        actual: String,
    },

    #[error("install of {name}@{version} was not approved")]
    NotApproved { name: String, version: String },
}
