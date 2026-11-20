use crate::{
    Account, AccountId, ConnectionMethod, Error, ProviderRegistry, Result,
    credentials::CredentialStore,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const SCHEMA: u32 = 1;

#[derive(Serialize, Deserialize)]
struct ProfilesFile {
    schema: u32,
    accounts: Vec<Account>,
}

/// Accounts persisted as JSON under the hub's data directory. Secrets live in a
/// `CredentialStore`; this file only holds non-sensitive metadata.
pub struct ProfileStore {
    root: PathBuf,
    accounts: Vec<Account>,
}

impl ProfileStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let path = root.join("accounts.json");
        let accounts = match fs::read(&path) {
            Ok(bytes) => {
                let file: ProfilesFile = serde_json::from_slice(&bytes)?;
                if file.schema > SCHEMA {
                    return Err(Error::Unsupported(format!(
                        "accounts.json schema {} is newer than {SCHEMA}",
                        file.schema
                    )));
                }
                file.accounts
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { root, accounts })
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn get(&self, id: &AccountId) -> Result<&Account> {
        self.accounts
            .iter()
            .find(|a| &a.id == id)
            .ok_or_else(|| Error::NotFound(format!("account {id}")))
    }

    pub fn add(&mut self, account: Account, providers: &ProviderRegistry) -> Result<()> {
        account.validate(providers.get(&account.provider)?)?;
        if self.accounts.iter().any(|a| a.id == account.id) {
            return Err(Error::Duplicate(format!("account {}", account.id)));
        }
        if account.method == ConnectionMethod::SystemCli
            && self
                .accounts
                .iter()
                .any(|a| a.provider == account.provider && a.method == ConnectionMethod::SystemCli)
        {
            return Err(Error::Duplicate(format!(
                "{} already has a system CLI account",
                account.provider.key()
            )));
        }
        self.accounts.push(account);
        self.save()
    }

    /// Removes the account, its stored secret and its isolated config directory.
    pub fn remove(&mut self, id: &AccountId, credentials: &mut dyn CredentialStore) -> Result<()> {
        let index = self
            .accounts
            .iter()
            .position(|a| &a.id == id)
            .ok_or_else(|| Error::NotFound(format!("account {id}")))?;
        if let Some(key) = self.accounts[index].credential_key() {
            credentials.delete(&key)?;
        }
        let dir = self.isolated_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        self.accounts.remove(index);
        self.save()
    }

    /// The config directory handed to the provider CLI for `OfficialLogin`.
    pub fn isolated_dir(&self, id: &AccountId) -> PathBuf {
        self.root.join("accounts").join(id.as_str())
    }

    pub fn ensure_isolated_dir(&self, id: &AccountId) -> Result<PathBuf> {
        let dir = self.isolated_dir(id);
        fs::create_dir_all(&dir)?;
        restrict(&dir)?;
        Ok(dir)
    }

    fn save(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer_pretty(
            file.as_file_mut(),
            &ProfilesFile {
                schema: SCHEMA,
                accounts: self.accounts.clone(),
            },
        )?;
        file.as_file_mut().flush()?;
        file.persist(self.root.join("accounts.json"))
            .map_err(|e| Error::Io(e.error))?;
        Ok(())
    }
}

#[cfg(unix)]
fn restrict(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_dir: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ProviderKind,
        credentials::{MemoryStore, Secret},
    };

    fn account(id: &str, provider: ProviderKind, method: ConnectionMethod) -> Account {
        Account::new(AccountId::new(id).unwrap(), provider, id, method)
    }

    #[test]
    fn accounts_persist_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let providers = ProviderRegistry::builtin();
        let mut credentials = MemoryStore::default();
        let mut store = ProfileStore::open(dir.path()).unwrap();
        let api = account("api", ProviderKind::ClaudeCode, ConnectionMethod::ApiKey);
        credentials
            .save(
                &api.credential_key().unwrap(),
                Secret::new("sk-secret-value"),
            )
            .unwrap();
        store.add(api, &providers).unwrap();
        store
            .add(
                account(
                    "work",
                    ProviderKind::ClaudeCode,
                    ConnectionMethod::OfficialLogin,
                ),
                &providers,
            )
            .unwrap();
        store
            .add(
                account(
                    "home",
                    ProviderKind::ClaudeCode,
                    ConnectionMethod::OfficialLogin,
                ),
                &providers,
            )
            .unwrap();

        let raw = fs::read_to_string(dir.path().join("accounts.json")).unwrap();
        assert!(!raw.contains("sk-secret-value"));
        let reopened = ProfileStore::open(dir.path()).unwrap();
        assert_eq!(reopened.accounts().len(), 3);
    }

    #[test]
    fn duplicates_and_second_system_cli_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let providers = ProviderRegistry::builtin();
        let mut store = ProfileStore::open(dir.path()).unwrap();
        store
            .add(
                account("a", ProviderKind::Codex, ConnectionMethod::SystemCli),
                &providers,
            )
            .unwrap();
        assert!(
            store
                .add(
                    account("a", ProviderKind::Codex, ConnectionMethod::OfficialLogin),
                    &providers
                )
                .is_err()
        );
        assert!(
            store
                .add(
                    account("b", ProviderKind::Codex, ConnectionMethod::SystemCli),
                    &providers
                )
                .is_err()
        );
        store
            .add(
                account("c", ProviderKind::ClaudeCode, ConnectionMethod::SystemCli),
                &providers,
            )
            .unwrap();
    }

    #[test]
    fn removal_cleans_secret_and_isolated_dir() {
        let dir = tempfile::tempdir().unwrap();
        let providers = ProviderRegistry::builtin();
        let mut credentials = MemoryStore::default();
        let mut store = ProfileStore::open(dir.path()).unwrap();
        let api = account("g", ProviderKind::Gemini, ConnectionMethod::ApiKey);
        let key = api.credential_key().unwrap();
        credentials.save(&key, Secret::new("x")).unwrap();
        store.add(api.clone(), &providers).unwrap();
        let isolated = store.ensure_isolated_dir(&api.id).unwrap();
        assert!(isolated.is_dir());
        store.remove(&api.id, &mut credentials).unwrap();
        assert!(credentials.load(&key).unwrap().is_none());
        assert!(!isolated.exists());
        assert!(store.accounts().is_empty());
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("accounts.json"),
            r#"{"schema":99,"accounts":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            ProfileStore::open(dir.path()),
            Err(Error::Unsupported(_))
        ));
    }
}
