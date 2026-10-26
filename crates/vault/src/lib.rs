//! App-owned secret storage seam. Memory storage is for tests/development only.
//! Native Keychain integration is deliberately unavailable in this foundation.

use std::collections::HashMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("native vault is not implemented")]
    Unavailable,
}

pub trait SecretStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, VaultError>;
    fn set(&mut self, key: &str, secret: &[u8]) -> Result<(), VaultError>;
    fn delete(&mut self, key: &str) -> Result<(), VaultError>;
}

// No Debug/Serialize: secret contents must not enter logs or the webview.
#[derive(Default)]
pub struct MemorySecretStore {
    secrets: HashMap<String, Vec<u8>>,
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, VaultError> {
        Ok(self.secrets.get(key).cloned())
    }
    fn set(&mut self, key: &str, secret: &[u8]) -> Result<(), VaultError> {
        self.secrets.insert(key.into(), secret.to_vec());
        Ok(())
    }
    fn delete(&mut self, key: &str) -> Result<(), VaultError> {
        self.secrets.remove(key);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub struct KeychainSecretStore;

#[cfg(target_os = "macos")]
impl SecretStore for KeychainSecretStore {
    fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, VaultError> {
        Err(VaultError::Unavailable)
    }
    fn set(&mut self, _key: &str, _secret: &[u8]) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
    fn delete(&mut self, _key: &str) -> Result<(), VaultError> {
        Err(VaultError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_secret_lifecycle() {
        let mut store = MemorySecretStore::default();
        assert_eq!(store.get("test").unwrap(), None);
        store.set("test", b"fabricated").unwrap();
        assert_eq!(store.get("test").unwrap(), Some(b"fabricated".to_vec()));
        store.set("test", b"replaced").unwrap();
        assert_eq!(store.get("test").unwrap(), Some(b"replaced".to_vec()));
        store.delete("test").unwrap();
        store.delete("test").unwrap();
        assert_eq!(store.get("test").unwrap(), None);
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_stub_fails_closed_without_accessing_keychain() {
        let mut store = KeychainSecretStore;
        assert_eq!(store.get("test"), Err(VaultError::Unavailable));
        assert_eq!(
            store.set("test", b"fabricated"),
            Err(VaultError::Unavailable)
        );
        assert_eq!(store.delete("test"), Err(VaultError::Unavailable));
    }
}
