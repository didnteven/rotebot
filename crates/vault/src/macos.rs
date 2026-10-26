use super::*;
use security_framework::{
    access_control::{ProtectionMode, SecAccessControl},
    passwords::{self, PasswordOptions},
};

pub struct KeychainStore;
fn options(reference: &SecretRef) -> PasswordOptions {
    let mut o = PasswordOptions::new_generic_password(SERVICE, reference.name());
    o.set_access_synchronized(Some(false));
    o.use_protected_keychain();
    o
}
impl SecretStore for KeychainStore {
    fn get(&self, reference: &SecretRef) -> Result<Option<Secret>, VaultError> {
        match passwords::generic_password(options(reference)) {
            Ok(v) => Ok(Some(Secret::new(v))),
            Err(e) if e.code() == -25300 => Ok(None),
            Err(_) => Err(VaultError::Unavailable),
        }
    }
    fn set(&self, reference: &SecretRef, secret: Secret) -> Result<(), VaultError> {
        let mut o = options(reference);
        let access = SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            0,
        )
        .map_err(|_| VaultError::Unavailable)?;
        o.set_access_control(access);
        passwords::set_generic_password_options(secret.expose(), o)
            .map_err(|_| VaultError::Unavailable)
    }
    fn delete(&self, reference: &SecretRef) -> Result<(), VaultError> {
        match passwords::delete_generic_password_options(options(reference)) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()),
            Err(_) => Err(VaultError::Unavailable),
        }
    }
}
