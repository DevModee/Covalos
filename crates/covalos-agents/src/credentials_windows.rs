//! `CredentialStore` backed by Windows Credential Manager (generic
//! credentials, persisted for the current user on this machine). Secrets are
//! encrypted at rest by the OS (DPAPI) and never written to COVALOS files.

use crate::{
    Error, Result,
    credentials::{CredentialKey, CredentialStore, Secret},
};
use std::ptr;
use windows_sys::Win32::{
    Foundation::{ERROR_NOT_FOUND, FILETIME, GetLastError},
    Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    },
};

/// Largest blob Credential Manager accepts for generic credentials.
const MAX_BLOB: usize = 5 * 512;

#[derive(Debug)]
pub struct WindowsCredentialStore {
    /// Prefix for target names, so tests can use a disposable namespace.
    namespace: String,
}

impl Default for WindowsCredentialStore {
    fn default() -> Self {
        Self::with_namespace("covalos")
    }
}

impl WindowsCredentialStore {
    pub fn with_namespace(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
        }
    }

    fn target(&self, key: &CredentialKey) -> Vec<u16> {
        wide(&format!(
            "{}:{}/{}",
            self.namespace, key.service, key.account
        ))
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error(what: &str) -> Error {
    // SAFETY: reads the calling thread's last-error value.
    let code = unsafe { GetLastError() };
    Error::Io(std::io::Error::other(format!(
        "Windows Credential Manager {what} failed (error {code})"
    )))
}

impl CredentialStore for WindowsCredentialStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>> {
        let target = self.target(key);
        let mut found: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: `target` is NUL-terminated and `found` receives a buffer
        // owned by the OS that is released with `CredFree` below.
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) };
        if ok == 0 {
            // SAFETY: reads the calling thread's last-error value.
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err(last_error("read"));
        }
        // SAFETY: on success `found` points to a valid CREDENTIALW whose blob
        // holds `CredentialBlobSize` bytes.
        let secret = unsafe {
            let credential = &*found;
            let bytes = std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            );
            let text = String::from_utf8(bytes.to_vec());
            CredFree(found.cast());
            text
        };
        secret
            .map(|text| Some(Secret::new(text)))
            .map_err(|_| Error::Invalid("stored credential is not UTF-8".into()))
    }

    fn save(&mut self, key: &CredentialKey, secret: Secret) -> Result<()> {
        let blob = secret.expose().as_bytes();
        if blob.is_empty() || blob.len() > MAX_BLOB {
            return Err(Error::Invalid(format!(
                "a credential must be between 1 and {MAX_BLOB} bytes"
            )));
        }
        let mut target = self.target(key);
        let mut user = wide(&key.account);
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            Comment: ptr::null_mut(),
            LastWritten: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: ptr::null_mut(),
            UserName: user.as_mut_ptr(),
        };
        // SAFETY: every pointer in `credential` outlives the call; the OS
        // copies the blob and strings.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(last_error("write"));
        }
        Ok(())
    }

    fn delete(&mut self, key: &CredentialKey) -> Result<()> {
        let target = self.target(key);
        // SAFETY: `target` is NUL-terminated.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            // SAFETY: reads the calling thread's last-error value.
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                return Ok(());
            }
            return Err(last_error("delete"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountId, ProviderKind};

    #[test]
    fn round_trip_through_credential_manager() {
        let namespace = format!("covalos-test-{}", std::process::id());
        let mut store = WindowsCredentialStore::with_namespace(namespace);
        let key = CredentialKey::api_key(&ProviderKind::Gemini, &AccountId::new("t").unwrap());
        store.delete(&key).unwrap();
        assert!(store.load(&key).unwrap().is_none());
        store.save(&key, Secret::new("sk-covalos-test")).unwrap();
        assert_eq!(
            store.load(&key).unwrap().unwrap().expose(),
            "sk-covalos-test"
        );
        store.save(&key, Secret::new("rotated")).unwrap();
        assert_eq!(store.load(&key).unwrap().unwrap().expose(), "rotated");
        store.delete(&key).unwrap();
        assert!(store.load(&key).unwrap().is_none());
    }
}
