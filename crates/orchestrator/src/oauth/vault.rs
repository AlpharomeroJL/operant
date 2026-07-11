//! [`Vault`]: where tokens live. "Tokens stored in the OS credential vault
//! ONLY (Windows Credential Manager) ... NEVER written to config or logs"
//! (`docs/specs/backends.md`). [`MockVault`] is an in-memory stand-in every
//! test in this crate uses; [`WindowsCredentialVault`] is the real thing,
//! behind the off-by-default `real-vault` Cargo feature so the default
//! build never links the `windows` crate, matching `operant-action`'s
//! `real-input` and this crate's own `real-transport` convention
//! (`campaign/checkpoint.md`: "Heavy platform deps (`windows` crate) are
//! added per-crate behind a feature").

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

use super::provider::ProviderId;
use super::token::SecretString;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("vault backend error for key `{key}`: {message}")]
    Backend { key: String, message: String },
}

impl VaultError {
    pub fn backend(key: impl Into<String>, message: impl Into<String>) -> Self {
        VaultError::Backend {
            key: key.into(),
            message: message.into(),
        }
    }
}

/// Which credential a vault key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Access,
    Refresh,
}

impl TokenKind {
    fn as_str(&self) -> &'static str {
        match self {
            TokenKind::Access => "access_token",
            TokenKind::Refresh => "refresh_token",
        }
    }
}

/// The vault key for one provider's one token. Stable and namespaced so a
/// vault implementation backed by a single flat keyspace (Windows
/// Credential Manager's target-name list) never collides with anything
/// else Operant stores there.
pub fn vault_key(provider: ProviderId, kind: TokenKind) -> String {
    format!("operant/oauth/{}/{}", provider.as_str(), kind.as_str())
}

/// A place to store and retrieve secrets by key, with nothing about OAuth
/// in the trait itself -- [`super::flow::Broker`] is the only caller that
/// knows these keys mean "access token" or "refresh token". Implementing
/// this against a new backend (macOS Keychain, a future Linux Secret
/// Service backend) never touches [`super::flow`].
pub trait Vault: Send + Sync {
    fn store(&self, key: &str, secret: &SecretString) -> Result<(), VaultError>;
    fn load(&self, key: &str) -> Result<Option<SecretString>, VaultError>;
    fn delete(&self, key: &str) -> Result<(), VaultError>;
}

/// In-memory [`Vault`] for tests. Never touches the real OS credential
/// store; every oauth test in this crate is built on this, per
/// `docs/specs/backends.md`'s own test bar ("using MockVault for tests").
#[derive(Default)]
pub struct MockVault {
    entries: Mutex<HashMap<String, SecretString>>,
}

impl MockVault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test-only introspection: every key currently stored, for asserting
    /// what a flow did or did not persist without exposing values.
    pub fn keys(&self) -> Vec<String> {
        let mut ks: Vec<String> = self.entries.lock().unwrap().keys().cloned().collect();
        ks.sort();
        ks
    }
}

impl Vault for MockVault {
    fn store(&self, key: &str, secret: &SecretString) -> Result<(), VaultError> {
        self.entries
            .lock()
            .unwrap()
            .insert(key.to_string(), secret.clone());
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Option<SecretString>, VaultError> {
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }

    fn delete(&self, key: &str) -> Result<(), VaultError> {
        self.entries.lock().unwrap().remove(key);
        Ok(())
    }
}

/// The real vault: Windows Credential Manager, via `CredWriteW` /
/// `CredReadW` / `CredDeleteW`. Behind `real-vault`; nothing in this
/// crate's default test suite constructs one. Every target name is
/// namespaced under `Operant:` so the credentials are identifiable (and
/// bulk-removable) in the Windows "Credential Manager" control panel.
#[cfg(all(windows, feature = "real-vault"))]
pub struct WindowsCredentialVault;

#[cfg(all(windows, feature = "real-vault"))]
impl WindowsCredentialVault {
    pub fn new() -> Self {
        WindowsCredentialVault
    }

    fn target_name(key: &str) -> String {
        format!("Operant:{key}")
    }
}

#[cfg(all(windows, feature = "real-vault"))]
impl Default for WindowsCredentialVault {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(windows, feature = "real-vault"))]
mod win {
    use std::ffi::c_void;
    use std::ptr;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    /// A null-terminated UTF-16 buffer plus the [`PCWSTR`] pointing at it.
    /// Kept as one owned value so the buffer outlives every use of the
    /// pointer -- a bare `PCWSTR` alone borrows nothing and is trivially a
    /// dangling-pointer bug waiting to happen if the buffer is a temporary.
    struct WideString {
        buf: Vec<u16>,
    }

    impl WideString {
        fn new(s: &str) -> Self {
            let mut buf: Vec<u16> = s.encode_utf16().collect();
            buf.push(0);
            WideString { buf }
        }

        fn as_pcwstr(&self) -> PCWSTR {
            PCWSTR(self.buf.as_ptr())
        }
    }

    pub fn write(target: &str, secret_utf8: &[u8]) -> Result<(), String> {
        let wide_target = WideString::new(target);
        let mut blob = secret_utf8.to_vec();

        let credential = CREDENTIALW {
            Flags: CRED_FLAGS(0),
            Type: CRED_TYPE_GENERIC,
            TargetName: windows::core::PWSTR(wide_target.buf.as_ptr() as *mut u16),
            Comment: windows::core::PWSTR::null(),
            LastWritten: FILETIME::default(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: windows::core::PWSTR::null(),
            UserName: windows::core::PWSTR::null(),
        };

        // Safety: `credential` is a valid, fully-initialized `CREDENTIALW`
        // whose pointer fields (`TargetName`, `CredentialBlob`) point into
        // buffers (`wide_target.buf`, `blob`) that outlive this call --
        // both are local bindings still in scope when `CredWriteW` runs,
        // and it does not retain either pointer past its own return.
        unsafe { CredWriteW(&credential, 0) }.map_err(|e| e.to_string())
    }

    /// `Ok(None)` when the target simply does not exist yet (not signed
    /// in, or already revoked) -- distinct from a real backend failure.
    pub fn read(target: &str) -> Result<Option<Vec<u8>>, String> {
        let wide_target = WideString::new(target);
        let mut raw: *mut CREDENTIALW = ptr::null_mut();

        // Safety: `raw` is an out-param `CredReadW` fills in on success;
        // on error it is left untouched and we never read it. The pointer
        // `CredReadW` returns must be freed with `CredFree`, done below
        // before returning.
        let result = unsafe { CredReadW(wide_target.as_pcwstr(), CRED_TYPE_GENERIC, 0, &mut raw) };

        match result {
            Ok(()) => {
                // Safety: `raw` is non-null and valid per `CredReadW`
                // succeeding; it stays valid until the `CredFree` call
                // below, after which it is never dereferenced again.
                let cred = unsafe { &*raw };
                let bytes = if cred.CredentialBlob.is_null() || cred.CredentialBlobSize == 0 {
                    Vec::new()
                } else {
                    unsafe {
                        std::slice::from_raw_parts(
                            cred.CredentialBlob,
                            cred.CredentialBlobSize as usize,
                        )
                        .to_vec()
                    }
                };
                unsafe { CredFree(raw as *const c_void) };
                Ok(Some(bytes))
            }
            Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn delete(target: &str) -> Result<(), String> {
        let wide_target = WideString::new(target);
        // Safety: `wide_target` outlives this call.
        match unsafe { CredDeleteW(wide_target.as_pcwstr(), CRED_TYPE_GENERIC, 0) } {
            Ok(()) => Ok(()),
            // Deleting something already absent is not an error for our
            // `Vault::delete` contract (idempotent delete).
            Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(all(windows, feature = "real-vault"))]
impl Vault for WindowsCredentialVault {
    fn store(&self, key: &str, secret: &SecretString) -> Result<(), VaultError> {
        win::write(&Self::target_name(key), secret.expose_secret().as_bytes())
            .map_err(|message| VaultError::backend(key, message))
    }

    fn load(&self, key: &str) -> Result<Option<SecretString>, VaultError> {
        let bytes = win::read(&Self::target_name(key))
            .map_err(|message| VaultError::backend(key, message))?;
        Ok(bytes.map(|b| SecretString::new(String::from_utf8_lossy(&b).into_owned())))
    }

    fn delete(&self, key: &str) -> Result<(), VaultError> {
        win::delete(&Self::target_name(key)).map_err(|message| VaultError::backend(key, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_key_is_namespaced_and_stable() {
        assert_eq!(
            vault_key(ProviderId::ChatgptPlan, TokenKind::Access),
            "operant/oauth/chatgpt_plan/access_token"
        );
        assert_eq!(
            vault_key(ProviderId::ClaudePlan, TokenKind::Refresh),
            "operant/oauth/claude_plan/refresh_token"
        );
    }

    #[test]
    fn mock_vault_round_trips_store_load_delete() {
        let vault = MockVault::new();
        let key = vault_key(ProviderId::ChatgptPlan, TokenKind::Access);
        assert!(vault.load(&key).unwrap().is_none());

        vault
            .store(&key, &SecretString::new("seeded-fake-access-token"))
            .unwrap();
        let loaded = vault.load(&key).unwrap().unwrap();
        assert_eq!(loaded.expose_secret(), "seeded-fake-access-token");

        vault.delete(&key).unwrap();
        assert!(vault.load(&key).unwrap().is_none());
    }

    #[test]
    fn mock_vault_store_overwrites_the_previous_value() {
        let vault = MockVault::new();
        let key = vault_key(ProviderId::ClaudePlan, TokenKind::Refresh);
        vault.store(&key, &SecretString::new("first")).unwrap();
        vault.store(&key, &SecretString::new("second")).unwrap();
        assert_eq!(vault.load(&key).unwrap().unwrap().expose_secret(), "second");
    }

    #[test]
    fn mock_vault_keys_lists_only_stored_entries_sorted() {
        let vault = MockVault::new();
        vault
            .store(
                &vault_key(ProviderId::ClaudePlan, TokenKind::Access),
                &SecretString::new("a"),
            )
            .unwrap();
        vault
            .store(
                &vault_key(ProviderId::ChatgptPlan, TokenKind::Access),
                &SecretString::new("b"),
            )
            .unwrap();
        assert_eq!(
            vault.keys(),
            vec![
                "operant/oauth/chatgpt_plan/access_token".to_string(),
                "operant/oauth/claude_plan/access_token".to_string(),
            ]
        );
    }

    #[test]
    fn deleting_a_missing_key_is_not_an_error() {
        let vault = MockVault::new();
        assert!(vault.delete("never-stored").is_ok());
    }
}
