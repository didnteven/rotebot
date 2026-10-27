use super::*;
use security_framework::{
    access_control::{ProtectionMode, SecAccessControl},
    passwords::{self, PasswordOptions},
};

pub struct KeychainStore;

/// errSecMissingEntitlement: the data-protection keychain needs a signed app with a
/// keychain-access-groups entitlement. Unsigned development builds fall back to the
/// login keychain under the same app-owned service (still non-synchronizing).
const MISSING_ENTITLEMENT: i32 = -34018;
const NOT_FOUND: i32 = -25300;

fn options(reference: &SecretRef, protected: bool) -> PasswordOptions {
    let mut o = PasswordOptions::new_generic_password(SERVICE, reference.name());
    o.set_access_synchronized(Some(false));
    if protected {
        o.use_protected_keychain();
    }
    o
}

fn get_in(reference: &SecretRef, protected: bool) -> Result<Option<Secret>, i32> {
    match passwords::generic_password(options(reference, protected)) {
        Ok(v) => Ok(Some(Secret::new(v))),
        Err(e) if e.code() == NOT_FOUND => Ok(None),
        Err(e) => Err(e.code()),
    }
}

fn set_in(reference: &SecretRef, secret: &Secret, protected: bool) -> Result<(), i32> {
    let mut o = options(reference, protected);
    if protected {
        let access = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            0,
        )
        .map_err(|e| e.code())?;
        o.set_access_control(access);
    }
    passwords::set_generic_password_options(secret.expose(), o).map_err(|e| e.code())
}

fn delete_in(reference: &SecretRef, protected: bool) -> Result<(), i32> {
    match passwords::delete_generic_password_options(options(reference, protected)) {
        Ok(()) => Ok(()),
        Err(e) if e.code() == NOT_FOUND => Ok(()),
        Err(e) => Err(e.code()),
    }
}

fn read_keychain(
    mut get: impl FnMut(bool) -> Result<Option<Secret>, i32>,
    mut promote: impl FnMut(&Secret) -> Result<(), i32>,
    mut remove_legacy: impl FnMut() -> Result<(), i32>,
) -> Result<Option<Secret>, i32> {
    match get(true) {
        Err(MISSING_ENTITLEMENT) => get(false),
        Ok(None) => {
            let legacy = get(false)?;
            if let Some(secret) = &legacy {
                promote(secret)?;
                remove_legacy()?;
            }
            Ok(legacy)
        }
        other => other,
    }
}

fn delete_keychains(mut delete: impl FnMut(bool) -> Result<(), i32>) -> Result<(), i32> {
    let protected = delete(true);
    let legacy = delete(false);
    match protected {
        Ok(()) | Err(MISSING_ENTITLEMENT) => legacy,
        Err(e) => Err(e),
    }
}

impl SecretStore for KeychainStore {
    fn kind(&self) -> SecretStoreKind {
        SecretStoreKind::Keychain
    }
    fn get(&self, reference: &SecretRef) -> Result<Option<Secret>, VaultError> {
        read_keychain(
            |protected| get_in(reference, protected),
            |secret| set_in(reference, secret, true),
            || delete_in(reference, false),
        )
        .map_err(|_| VaultError::Unavailable)
    }
    fn set(&self, reference: &SecretRef, secret: Secret) -> Result<(), VaultError> {
        match set_in(reference, &secret, true) {
            Err(MISSING_ENTITLEMENT) => {
                set_in(reference, &secret, false).map_err(|_| VaultError::Unavailable)
            }
            Ok(()) => delete_in(reference, false).map_err(|_| VaultError::Unavailable),
            Err(_) => Err(VaultError::Unavailable),
        }
    }
    fn delete(&self, reference: &SecretRef) -> Result<(), VaultError> {
        delete_keychains(|protected| delete_in(reference, protected))
            .map_err(|_| VaultError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_upgrade_migrates_the_development_key_instead_of_generating_a_new_one() {
        let promoted = std::cell::Cell::new(false);
        let removed = std::cell::Cell::new(false);
        let key = read_keychain(
            |protected| {
                Ok(if protected {
                    None
                } else {
                    Some(Secret::new(b"existing-db-key".to_vec()))
                })
            },
            |s| {
                assert_eq!(s.expose(), b"existing-db-key");
                promoted.set(true);
                Ok(())
            },
            || {
                assert!(promoted.get());
                removed.set(true);
                Ok(())
            },
        )
        .unwrap()
        .expect("must find the existing development key");
        assert_eq!(key.expose(), b"existing-db-key");
        assert!(promoted.get() && removed.get());
    }

    #[test]
    fn deletion_removes_both_app_owned_keychain_variants() {
        let mut deleted = Vec::new();
        delete_keychains(|protected| {
            deleted.push(protected);
            Ok(())
        })
        .unwrap();
        assert_eq!(deleted, [true, false]);
    }
}
