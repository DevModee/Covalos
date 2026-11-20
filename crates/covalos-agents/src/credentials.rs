use crate::{AccountId, ProviderKind, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

/// A secret value that never prints and is wiped when dropped.
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // SAFETY: zero bytes are valid UTF-8, so the string stays well-formed.
        unsafe { self.0.as_bytes_mut() }.fill(0);
    }
}

/// Location of a secret inside the platform store. Profiles persist this key,
/// never the secret.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CredentialKey {
    pub service: String,
    pub account: String,
}

impl CredentialKey {
    pub fn api_key(provider: &ProviderKind, account: &AccountId) -> Self {
        Self {
            service: "covalos".into(),
            account: format!("{}/{}/api-key", provider.key(), account),
        }
    }
}

/// Backend for secrets. A production implementation should wrap the OS store
/// (Windows Credential Manager, macOS Keychain, Secret Service on Linux).
pub trait CredentialStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>>;
    fn save(&mut self, key: &CredentialKey, secret: Secret) -> Result<()>;
    fn delete(&mut self, key: &CredentialKey) -> Result<()>;
}

#[derive(Default)]
pub struct MemoryStore {
    secrets: HashMap<CredentialKey, Secret>,
}

impl CredentialStore for MemoryStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>> {
        Ok(self.secrets.get(key).map(|s| Secret::new(s.expose())))
    }

    fn save(&mut self, key: &CredentialKey, secret: Secret) -> Result<()> {
        self.secrets.insert(key.clone(), secret);
        Ok(())
    }

    fn delete(&mut self, key: &CredentialKey) -> Result<()> {
        self.secrets.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_do_not_leak_through_debug() {
        let secret = Secret::new("sk-test-123");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert_eq!(secret.expose(), "sk-test-123");
    }

    #[test]
    fn keys_are_scoped_by_provider_and_account() {
        let account = AccountId::new("work").unwrap();
        let a = CredentialKey::api_key(&ProviderKind::ClaudeCode, &account);
        let b = CredentialKey::api_key(&ProviderKind::Gemini, &account);
        assert_ne!(a, b);
        assert_eq!(a.account, "claude-code/work/api-key");
    }

    #[test]
    fn memory_store_round_trip() {
        let mut store = MemoryStore::default();
        let key = CredentialKey::api_key(&ProviderKind::Gemini, &AccountId::new("me").unwrap());
        store.save(&key, Secret::new("value")).unwrap();
        assert_eq!(store.load(&key).unwrap().unwrap().expose(), "value");
        store.delete(&key).unwrap();
        assert!(store.load(&key).unwrap().is_none());
    }
}
