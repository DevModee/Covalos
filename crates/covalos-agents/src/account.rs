use crate::{AccountId, Error, ProviderKind, ProviderSpec, Result, credentials::CredentialKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionMethod {
    /// The CLI's existing default sign-in, exactly as the user configured it.
    SystemCli,
    /// A COVALOS-owned config directory, signed in through the provider's own
    /// login flow. This is what allows several subscription accounts.
    OfficialLogin,
    /// An API key kept in the credential store and passed via the provider's
    /// documented environment variable.
    ApiKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub provider: ProviderKind,
    pub label: String,
    pub method: ConnectionMethod,
    #[serde(default = "enabled")]
    pub enabled: bool,
    /// Models the user says this account may use. Observed models are tracked
    /// at runtime by `models::ModelCatalog`.
    #[serde(default)]
    pub declared_models: Vec<String>,
    #[serde(default)]
    pub default_model: Option<String>,
}

fn enabled() -> bool {
    true
}

impl Account {
    pub fn new(
        id: AccountId,
        provider: ProviderKind,
        label: impl Into<String>,
        method: ConnectionMethod,
    ) -> Self {
        Self {
            id,
            provider,
            label: label.into(),
            method,
            enabled: true,
            declared_models: Vec::new(),
            default_model: None,
        }
    }

    pub fn validate(&self, spec: &ProviderSpec) -> Result<()> {
        if spec.kind != self.provider {
            return Err(Error::Invalid(format!(
                "account {} belongs to {}, not {}",
                self.id,
                self.provider.key(),
                spec.kind.key()
            )));
        }
        if self.label.trim().is_empty() {
            return Err(Error::Invalid(format!("account {} needs a label", self.id)));
        }
        match self.method {
            ConnectionMethod::SystemCli => Ok(()),
            ConnectionMethod::OfficialLogin if !spec.supports_isolated_login() => {
                Err(Error::Unsupported(format!(
                    "{} has no documented way to isolate sign-in state",
                    spec.label
                )))
            }
            ConnectionMethod::ApiKey if spec.api_key_var.is_none() => Err(Error::Unsupported(
                format!("{} has no verified API-key variable", spec.label),
            )),
            _ => Ok(()),
        }
    }

    pub fn credential_key(&self) -> Option<CredentialKey> {
        (self.method == ConnectionMethod::ApiKey)
            .then(|| CredentialKey::api_key(&self.provider, &self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(provider: ProviderKind, method: ConnectionMethod) -> Account {
        Account::new(AccountId::new("a").unwrap(), provider, "A", method)
    }

    #[test]
    fn methods_are_checked_against_provider_capabilities() {
        use ConnectionMethod::*;
        let claude = ProviderSpec::claude_code();
        let codex = ProviderSpec::codex();
        let gemini = ProviderSpec::gemini();
        for method in [SystemCli, OfficialLogin, ApiKey] {
            account(ProviderKind::ClaudeCode, method)
                .validate(&claude)
                .unwrap();
        }
        assert!(
            account(ProviderKind::Codex, OfficialLogin)
                .validate(&codex)
                .is_ok()
        );
        assert!(
            account(ProviderKind::Codex, ApiKey)
                .validate(&codex)
                .is_err()
        );
        assert!(
            account(ProviderKind::Gemini, ApiKey)
                .validate(&gemini)
                .is_ok()
        );
        assert!(
            account(ProviderKind::Gemini, OfficialLogin)
                .validate(&gemini)
                .is_err()
        );
        assert!(
            account(ProviderKind::Gemini, SystemCli)
                .validate(&claude)
                .is_err()
        );
    }

    #[test]
    fn only_api_key_accounts_reference_credentials() {
        assert!(
            account(ProviderKind::ClaudeCode, ConnectionMethod::ApiKey)
                .credential_key()
                .is_some()
        );
        assert!(
            account(ProviderKind::ClaudeCode, ConnectionMethod::OfficialLogin)
                .credential_key()
                .is_none()
        );
    }
}
