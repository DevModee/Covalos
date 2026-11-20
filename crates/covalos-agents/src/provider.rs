use crate::{Error, Result, validate_id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    ClaudeCode,
    Codex,
    Gemini,
    Custom(String),
}

impl ProviderKind {
    pub fn key(&self) -> &str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Custom(name) => name,
        }
    }
}

/// How the hub starts the provider's own sign-in. COVALOS never captures the
/// resulting tokens: the CLI writes them to its own config directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoginFlow {
    /// A dedicated subcommand, e.g. `codex login`.
    Command(Vec<String>),
    /// Login happens inside the interactive CLI (e.g. Claude Code's `/login`).
    Interactive {
        hint: String,
    },
    None,
}

/// Whether COVALOS has a tested decoder and argument builder for headless runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Headless {
    ClaudeStreamJson,
    CodexExecJson,
    Unsupported,
}

/// Static facts about a provider. Only documented, officially supported
/// environment variables are listed; unknown capabilities stay `None`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSpec {
    pub kind: ProviderKind,
    pub label: String,
    pub executable: String,
    pub binary_override_var: String,
    /// Environment variable that relocates the CLI's config and login state.
    /// Required for several isolated official-login accounts per provider.
    pub config_dir_var: Option<String>,
    /// Environment variable the CLI reads an API key from.
    pub api_key_var: Option<String>,
    pub login: LoginFlow,
    pub headless: Headless,
}

impl ProviderSpec {
    pub fn claude_code() -> Self {
        Self {
            kind: ProviderKind::ClaudeCode,
            label: "Claude Code".into(),
            executable: "claude".into(),
            binary_override_var: "COVALOS_CLAUDE_BIN".into(),
            config_dir_var: Some("CLAUDE_CONFIG_DIR".into()),
            api_key_var: Some("ANTHROPIC_API_KEY".into()),
            login: LoginFlow::Interactive {
                hint: "Run /login inside Claude Code".into(),
            },
            headless: Headless::ClaudeStreamJson,
        }
    }

    pub fn codex() -> Self {
        Self {
            kind: ProviderKind::Codex,
            label: "OpenAI Codex".into(),
            executable: "codex".into(),
            binary_override_var: "COVALOS_CODEX_BIN".into(),
            config_dir_var: Some("CODEX_HOME".into()),
            api_key_var: None,
            login: LoginFlow::Command(vec!["login".into()]),
            headless: Headless::CodexExecJson,
        }
    }

    pub fn gemini() -> Self {
        Self {
            kind: ProviderKind::Gemini,
            label: "Gemini CLI".into(),
            executable: "gemini".into(),
            binary_override_var: "COVALOS_GEMINI_BIN".into(),
            config_dir_var: None,
            api_key_var: Some("GEMINI_API_KEY".into()),
            login: LoginFlow::Interactive {
                hint: "Choose a sign-in method when Gemini CLI starts".into(),
            },
            headless: Headless::Unsupported,
        }
    }

    pub fn supports_isolated_login(&self) -> bool {
        self.config_dir_var.is_some() && self.login != LoginFlow::None
    }
}

#[derive(Clone, Debug)]
pub struct ProviderRegistry {
    specs: BTreeMap<ProviderKind, ProviderSpec>,
}

impl ProviderRegistry {
    pub fn builtin() -> Self {
        let specs = [
            ProviderSpec::claude_code(),
            ProviderSpec::codex(),
            ProviderSpec::gemini(),
        ];
        Self {
            specs: specs.into_iter().map(|s| (s.kind.clone(), s)).collect(),
        }
    }

    /// Adds a user-defined provider. Built-in providers cannot be shadowed.
    pub fn register(&mut self, spec: ProviderSpec) -> Result<()> {
        let ProviderKind::Custom(name) = &spec.kind else {
            return Err(Error::Invalid(
                "only custom providers can be registered".into(),
            ));
        };
        validate_id(name)?;
        if ["claude-code", "codex", "gemini"].contains(&name.as_str()) {
            return Err(Error::Duplicate(name.clone()));
        }
        if spec.executable.is_empty()
            || spec.executable.contains(['/', '\\'])
            || spec.executable.contains(char::is_whitespace)
        {
            return Err(Error::Invalid(format!(
                "executable must be a bare program name: {:?}",
                spec.executable
            )));
        }
        if self.specs.contains_key(&spec.kind) {
            return Err(Error::Duplicate(name.clone()));
        }
        self.specs.insert(spec.kind.clone(), spec);
        Ok(())
    }

    pub fn get(&self, kind: &ProviderKind) -> Result<&ProviderSpec> {
        self.specs
            .get(kind)
            .ok_or_else(|| Error::NotFound(format!("provider {}", kind.key())))
    }

    pub fn iter(&self) -> impl Iterator<Item = &ProviderSpec> {
        self.specs.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(name: &str, exe: &str) -> ProviderSpec {
        ProviderSpec {
            kind: ProviderKind::Custom(name.into()),
            label: name.into(),
            executable: exe.into(),
            binary_override_var: "COVALOS_CUSTOM_BIN".into(),
            config_dir_var: None,
            api_key_var: None,
            login: LoginFlow::None,
            headless: Headless::Unsupported,
        }
    }

    #[test]
    fn builtins_are_present_and_isolation_is_explicit() {
        let registry = ProviderRegistry::builtin();
        assert_eq!(registry.iter().count(), 3);
        assert!(
            registry
                .get(&ProviderKind::ClaudeCode)
                .unwrap()
                .supports_isolated_login()
        );
        assert!(
            registry
                .get(&ProviderKind::Codex)
                .unwrap()
                .supports_isolated_login()
        );
        assert!(
            !registry
                .get(&ProviderKind::Gemini)
                .unwrap()
                .supports_isolated_login()
        );
    }

    #[test]
    fn custom_providers_are_validated() {
        let mut registry = ProviderRegistry::builtin();
        assert!(registry.register(custom("codex", "x")).is_err());
        assert!(registry.register(custom("local", "../bin/x")).is_err());
        assert!(registry.register(custom("local", "a b")).is_err());
        assert!(registry.register(custom("Local", "x")).is_err());
        registry.register(custom("local", "ollama")).unwrap();
        assert!(registry.register(custom("local", "ollama")).is_err());
        assert!(registry.get(&ProviderKind::Custom("local".into())).is_ok());
    }
}
