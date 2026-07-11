//! PKCE (RFC 7636), S256 only. `contracts/fixtures/oauth/config.json`'s
//! rule is unconditional -- "S256 required; plain rejected" -- so this
//! module does not expose a way to construct a `plain` challenge at all;
//! there is no enum variant, no flag, nothing for a caller to flip. The
//! "PKCE plain is rejected" test bar is satisfied on the client side by
//! this simply not existing, and on the server side by
//! `oauth::mock_server` refusing any `code_challenge_method` other than
//! `S256`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

use super::token::SecretString;

/// A PKCE code verifier/challenge pair. The verifier never leaves the
/// process except inside the token-exchange request body (over the
/// provider's TLS/loopback transport); only the challenge goes into the
/// authorize URL.
pub struct Pkce {
    pub verifier: SecretString,
    pub challenge: String,
}

impl Pkce {
    /// Generate a fresh verifier (32 random bytes, base64url-nopad --
    /// RFC 7636 section 4.1 requires 43-128 characters from the unreserved
    /// set; 32 bytes encodes to exactly 43) and its S256 challenge.
    pub fn generate() -> Self {
        let mut raw = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut raw);
        let verifier = URL_SAFE_NO_PAD.encode(raw);
        let challenge = challenge_for(&verifier);
        Pkce {
            verifier: SecretString::new(verifier),
            challenge,
        }
    }

    /// The fixed method name this broker ever sends. Not configurable.
    pub const METHOD: &'static str = "S256";
}

/// `BASE64URL-ENCODE(SHA256(ASCII(verifier)))`, exactly RFC 7636's S256
/// transform. Used by the client to build [`Pkce::challenge`] and by
/// `oauth::mock_server` to check a submitted `code_verifier` against the
/// `code_challenge` it recorded at authorize time.
pub fn challenge_for(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// A random, URL-safe `state` (CSRF token) or `nonce`. Both are the same
/// shape (opaque random bytes, echoed back and compared), so one generator
/// serves both.
pub fn random_token(byte_len: usize) -> String {
    let mut raw = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut raw);
    URL_SAFE_NO_PAD.encode(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_a_43_char_verifier_and_matching_challenge() {
        let pkce = Pkce::generate();
        assert_eq!(pkce.verifier.expose_secret().len(), 43);
        assert_eq!(pkce.challenge, challenge_for(pkce.verifier.expose_secret()));
        assert_eq!(Pkce::METHOD, "S256");
    }

    #[test]
    fn two_generated_verifiers_are_not_equal() {
        let a = Pkce::generate();
        let b = Pkce::generate();
        assert_ne!(a.verifier.expose_secret(), b.verifier.expose_secret());
    }

    #[test]
    fn challenge_for_is_deterministic_for_a_fixed_verifier() {
        // A fixed, seeded-fake verifier -- not a real credential, just a
        // stable input to pin the S256 transform down.
        let verifier = "seeded-fake-verifier-0000000000000000000000";
        let c1 = challenge_for(verifier);
        let c2 = challenge_for(verifier);
        assert_eq!(c1, c2);
        // Challenge is base64url: no padding, no '+' or '/'.
        assert!(!c1.contains('+'));
        assert!(!c1.contains('/'));
        assert!(!c1.contains('='));
    }

    #[test]
    fn random_token_lengths_and_uniqueness() {
        let a = random_token(16);
        let b = random_token(16);
        assert_ne!(a, b);
        assert!(!a.is_empty());
    }
}
