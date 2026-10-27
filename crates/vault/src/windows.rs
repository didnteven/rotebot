use super::*;
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_NOT_FOUND},
    Security::Credentials::*,
};
pub struct CredentialStore;
fn target(r: &SecretRef) -> Vec<u16> {
    format!("{SERVICE}:{}", r.name())
        .encode_utf16()
        .chain(Some(0))
        .collect()
}
impl SecretStore for CredentialStore {
    fn kind(&self) -> SecretStoreKind {
        SecretStoreKind::CredentialManager
    }
    fn get(&self, r: &SecretRef) -> Result<Option<Secret>, VaultError> {
        let name = target(r);
        let mut ptr = std::ptr::null_mut();
        // SAFETY: NUL-terminated target, output pointer owned by CredReadW.
        unsafe {
            if CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut ptr) == 0 {
                return if GetLastError() == ERROR_NOT_FOUND {
                    Ok(None)
                } else {
                    Err(VaultError::Unavailable)
                };
            }
            let data = if (*ptr).CredentialBlobSize == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(
                    (*ptr).CredentialBlob,
                    (*ptr).CredentialBlobSize as usize,
                )
                .to_vec()
            };
            CredFree(ptr.cast());
            Ok(Some(Secret::new(data)))
        }
    }
    fn set(&self, r: &SecretRef, secret: Secret) -> Result<(), VaultError> {
        let mut name = target(r);
        let size = u32::try_from(secret.expose().len()).map_err(|_| VaultError::Unavailable)?;
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            CredentialBlobSize: size,
            CredentialBlob: secret.expose().as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: all referenced storage lives through the call, no pointers retained.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            Err(VaultError::Unavailable)
        } else {
            Ok(())
        }
    }
    fn delete(&self, r: &SecretRef) -> Result<(), VaultError> {
        let name = target(r);
        // SAFETY: NUL-terminated target lives through the call.
        if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0
            || unsafe { GetLastError() } == ERROR_NOT_FOUND
        {
            Ok(())
        } else {
            Err(VaultError::Unavailable)
        }
    }
}
