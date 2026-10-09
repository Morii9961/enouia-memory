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

fn delete_with(name: &str, remove: impl FnOnce(&[u16]) -> Result<()>) -> Result<()> {
    // Validate before the native side effect, including in the offline harness.
    let name = target(name)?;
    remove(&name)
}

#[cfg(windows)]
fn delete_result(os_error: Option<u32>) -> Result<()> {
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
    match os_error {
        None | Some(ERROR_NOT_FOUND) => Ok(()),
        Some(_) => Err(error(MemoryErrorCode::StorageFailed)),
    }
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
    /// Explicit trusted native setup only. Idempotent for an absent entry.
    /// Does not revoke a remote key or stop/join already admitted calls.
    pub fn delete(&self, name: &str) -> Result<()> {
        delete_with(name, |target| {
            #[cfg(windows)]
            {
                use windows_sys::Win32::{
                    Foundation::GetLastError,
                    Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW},
                };
                // SAFETY: restricted nul-terminated target remains alive for
                // the synchronous call. Generic type and zero flags are fixed.
                let removed = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
                let os_error = if removed == 0 {
                    // Capture immediately; no other Win32 call intervenes.
                    Some(unsafe { GetLastError() })
                } else {
                    None
                };
                delete_result(os_error)
            }
            #[cfg(not(windows))]
            {
                let _ = target;
                Err(error(MemoryErrorCode::ProviderUnavailable))
            }
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn credential_deletion_validates_exact_targets_before_any_driver_call() {
        let calls = Cell::new(0);
        for name in [
            "",
            "Enouia.Memory.Provider.*",
            "Enouia.Memory.Provider.openai.extra",
            "enouia.memory.provider.openai",
            "Enouia.Memory.Provider.anthropic ",
            "Enouia.Memory.Provider.openai\0",
            "unrelated.synthetic.credential",
        ] {
            assert_eq!(
                delete_with(name, |_| {
                    calls.set(calls.get() + 1);
                    Ok(())
                })
                .unwrap_err()
                .code,
                MemoryErrorCode::PermissionDenied
            );
            // The public entry point must also refuse before any OS call.
            assert_eq!(
                CredentialStore.delete(name).unwrap_err().code,
                MemoryErrorCode::PermissionDenied
            );
        }
        assert_eq!(calls.get(), 0);
        for name in [
            "Enouia.Memory.Provider.openai",
            "Enouia.Memory.Provider.anthropic",
        ] {
            delete_with(name, |target| {
                calls.set(calls.get() + 1);
                assert_eq!(target.last(), Some(&0));
                assert_eq!(
                    String::from_utf16(&target[..target.len() - 1]).unwrap(),
                    name
                );
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(calls.get(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn credential_deletion_is_idempotent_only_for_confirmed_absence() {
        use windows_sys::Win32::Foundation::{
            ERROR_ACCESS_DENIED, ERROR_NO_SUCH_LOGON_SESSION, ERROR_NOT_FOUND,
        };
        for name in [
            "Enouia.Memory.Provider.openai",
            "Enouia.Memory.Provider.anthropic",
        ] {
            for outcome in [None, Some(ERROR_NOT_FOUND)] {
                // Fake driver only: neither CredDeleteW nor CredReadW is called.
                delete_with(name, |_| delete_result(outcome)).unwrap();
                delete_with(name, |_| delete_result(outcome)).unwrap();
            }
            for outcome in [
                0,
                ERROR_ACCESS_DENIED,
                ERROR_NO_SUCH_LOGON_SESSION,
                u32::MAX,
            ] {
                let failure = delete_with(name, |_| delete_result(Some(outcome))).unwrap_err();
                assert_eq!(failure.code, MemoryErrorCode::StorageFailed);
                assert!(failure.retryable);
            }
        }
    }
}
