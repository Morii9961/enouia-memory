//! Native credential access only. Never exposed to a web page or Vault backup.
use crate::{Result, error};
use enouia_memory_contract::{
    MemoryErrorCode,
    ports::{SecretBytes, SecretStore},
};

#[derive(Default)]
pub struct CredentialStore;
fn target(name: &str) -> Result<Vec<u16>> {
    if !matches!(
        name,
        "Enouia.Memory.Provider.openai" | "Enouia.Memory.Provider.anthropic"
    ) {
        return Err(error(MemoryErrorCode::PermissionDenied));
    }
    Ok(name.encode_utf16().chain(Some(0)).collect())
}
impl SecretStore for CredentialStore {
    fn read(&self, name: &str) -> Result<SecretBytes> {
        let target = target(name)?;
        #[cfg(windows)]
        {
            use windows_sys::Win32::Security::Credentials::{
                CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
            };
            let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY: nul-terminated name, correct out-pointer, buffer freed
            // exactly once after copying its bounded generic credential blob.
            unsafe {
                if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) == 0 {
                    return Err(error(MemoryErrorCode::Unauthenticated));
                }
                let cred = &*credential;
                let result = if cred.CredentialBlobSize == 0
                    || cred.CredentialBlobSize > 4096
                    || cred.CredentialBlob.is_null()
                {
                    Err(error(MemoryErrorCode::Unauthenticated))
                } else {
                    Ok(SecretBytes::new(
                        std::slice::from_raw_parts(
                            cred.CredentialBlob,
                            cred.CredentialBlobSize as usize,
                        )
                        .to_vec(),
                    ))
                };
                CredFree(credential.cast());
                result
            }
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(error(MemoryErrorCode::Unauthenticated))
        }
    }
}
impl CredentialStore {
    /// Trusted native setup only; never call this using page IPC key material.
    pub fn write(&self, name: &str, secret: &SecretBytes) -> Result<()> {
        let mut name = target(name)?;
        if secret.expose().is_empty()
            || secret.expose().len() > 4096
            || secret
                .expose()
                .iter()
                .any(|b| !b.is_ascii() || b.is_ascii_control())
        {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Security::Credentials::{
                CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
            };
            let credential = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: name.as_mut_ptr(),
                CredentialBlobSize: secret.expose().len() as u32,
                CredentialBlob: secret.expose().as_ptr().cast_mut(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                ..Default::default()
            };
            // SAFETY: pointers remain valid for the synchronous Win32 call;
            // credential manager copies bytes rather than taking ownership.
            if unsafe { CredWriteW(&credential, 0) } == 0 {
                Err(error(MemoryErrorCode::StorageFailed))
            } else {
                Ok(())
            }
        }
        #[cfg(not(windows))]
        {
            let _ = &mut name;
            Err(error(MemoryErrorCode::ProviderUnavailable))
        }
    }
}
