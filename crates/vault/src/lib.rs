//! App-owned credentials. Secret bytes are never serializable and are zeroed on drop.
use std::{collections::HashMap, sync::Mutex};
use zeroize::Zeroizing;

pub const SERVICE: &str = "dev.macroscope.app";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("invalid app-owned secret reference")]
    InvalidReference,
    #[error("key is empty")]
    Empty,
    #[error("key contains a newline")]
    Newline,
    #[error("credential store unavailable")]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretRef(String);
impl SecretRef {
    pub fn new(name: impl Into<String>) -> Result<Self, VaultError> {
        let name = name.into();
        if name.is_empty()
            || name.len() > 200
            || name == "ai.meta.dev.credentials"
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
        {
            return Err(VaultError::InvalidReference);
        }
        Ok(Self(name))
    }
    pub fn name(&self) -> &str {
        &self.0
    }
}

pub struct Secret(Zeroizing<Vec<u8>>);
impl Secret {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct KeyFingerprint {
    pub length: usize,
    pub last_four: String,
}
pub fn validate_key(raw: &str) -> Result<(Secret, KeyFingerprint), VaultError> {
    if raw.contains(['\r', '\n']) {
        return Err(VaultError::Newline);
    }
    let value = raw.trim();
    if value.is_empty() {
        return Err(VaultError::Empty);
    }
    let last_four: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Ok((
        Secret::new(value.as_bytes().to_vec()),
        KeyFingerprint {
            length: value.chars().count(),
            last_four,
        },
    ))
}

pub trait SecretStore: Send + Sync {
    fn get(&self, reference: &SecretRef) -> Result<Option<Secret>, VaultError>;
    fn set(&self, reference: &SecretRef, secret: Secret) -> Result<(), VaultError>;
    fn delete(&self, reference: &SecretRef) -> Result<(), VaultError>;
}
#[derive(Default)]
pub struct MemoryStore {
    secrets: Mutex<HashMap<SecretRef, Secret>>,
}
impl SecretStore for MemoryStore {
    fn get(&self, reference: &SecretRef) -> Result<Option<Secret>, VaultError> {
        Ok(self
            .secrets
            .lock()
            .map_err(|_| VaultError::Unavailable)?
            .get(reference)
            .map(|s| Secret::new(s.expose().to_vec())))
    }
    fn set(&self, reference: &SecretRef, secret: Secret) -> Result<(), VaultError> {
        self.secrets
            .lock()
            .map_err(|_| VaultError::Unavailable)?
            .insert(reference.clone(), secret);
        Ok(())
    }
    fn delete(&self, reference: &SecretRef) -> Result<(), VaultError> {
        self.secrets
            .lock()
            .map_err(|_| VaultError::Unavailable)?
            .remove(reference);
        Ok(())
    }
}
pub type MemorySecretStore = MemoryStore;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::KeychainStore;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::CredentialStore;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_secret_lifecycle_and_redaction() {
        let store = MemoryStore::default();
        let r = SecretRef::new("meta-api-key").unwrap();
        assert!(store.get(&r).unwrap().is_none());
        store
            .set(&r, Secret::new(b"bogus-canary-1234".to_vec()))
            .unwrap();
        let secret = store.get(&r).unwrap().unwrap();
        assert_eq!(secret.expose(), b"bogus-canary-1234");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        store.set(&r, Secret::new(b"replacement".to_vec())).unwrap();
        assert_eq!(store.get(&r).unwrap().unwrap().expose(), b"replacement");
        store.delete(&r).unwrap();
        store.delete(&r).unwrap();
        assert!(store.get(&r).unwrap().is_none());
    }
    #[test]
    fn rejects_muse_service_and_bad_refs() {
        for name in ["ai.meta.dev.credentials", "", "../meta", "meta\nkey"] {
            assert!(SecretRef::new(name).is_err());
        }
        assert!(SecretRef::new("mcp-token:gmail").is_ok());
        assert_eq!(SERVICE, "dev.macroscope.app");
    }
    #[test]
    fn key_validation_and_fingerprint() {
        let (s, fp) = validate_key("  bogus-canary-1234 \t").unwrap();
        assert_eq!(s.expose(), b"bogus-canary-1234");
        assert_eq!(fp.length, 17);
        assert_eq!(fp.last_four, "1234");
        for raw in ["\ncanary", "canary\n", "a\rb"] {
            assert_eq!(validate_key(raw).unwrap_err(), VaultError::Newline);
        }
        assert_eq!(validate_key(" \t").unwrap_err(), VaultError::Empty);
    }
}
