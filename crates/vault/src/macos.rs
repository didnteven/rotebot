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

impl SecretStore for KeychainStore {
    fn get(&self, reference: &SecretRef) -> Result<Option<Secret>, VaultError> {
        match get_in(reference, true) {
            Err(MISSING_ENTITLEMENT) => {
                get_in(reference, false).map_err(|_| VaultError::Unavailable)
            }
            other => other.map_err(|_| VaultError::Unavailable),
        }
    }
    fn set(&self, reference: &SecretRef, secret: Secret) -> Result<(), VaultError> {
        match set_in(reference, &secret, true) {
            Err(MISSING_ENTITLEMENT) => {
                set_in(reference, &secret, false).map_err(|_| VaultError::Unavailable)
            }
            other => other.map_err(|_| VaultError::Unavailable),
        }
    }
    fn delete(&self, reference: &SecretRef) -> Result<(), VaultError> {
        match delete_in(reference, true) {
            Err(MISSING_ENTITLEMENT) => {
                delete_in(reference, false).map_err(|_| VaultError::Unavailable)
            }
            other => other.map_err(|_| VaultError::Unavailable),
        }
    }
}
