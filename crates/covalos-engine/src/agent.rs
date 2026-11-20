//! Agent profiles and assignment. Selection only uses facts the hub actually
//! has: account state, a real binary probe, user-declared capabilities, the
//! model catalog and current load. Nothing about quotas or prices is assumed.

use crate::role::{Capability, Role};
use covalos_agents::{
    Account, AccountId, ProviderKind, RoleId,
    models::{Availability, ModelCatalog},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct AgentProfile {
    pub account: AccountId,
    pub provider: ProviderKind,
    pub enabled: bool,
    /// Declared by the user for this account; providers do not expose this.
    pub capabilities: BTreeSet<Capability>,
    pub max_parallel: usize,
    /// Result of probing the provider binary: `None` until probed.
    pub installed: Option<bool>,
    pub models: ModelCatalog,
}

impl AgentProfile {
    pub fn from_account(
        account: &Account,
        capabilities: BTreeSet<Capability>,
        installed: Option<bool>,
    ) -> Self {
        Self {
            account: account.id.clone(),
            provider: account.provider.clone(),
            enabled: account.enabled,
            capabilities,
            max_parallel: 1,
            installed,
            models: ModelCatalog::for_account(account),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Preferences {
    /// Earlier providers win ties.
    pub provider_order: Vec<ProviderKind>,
    /// Roles that must run on a specific account.
    pub pinned: BTreeMap<RoleId, AccountId>,
    pub excluded: BTreeSet<AccountId>,
    /// Models to request, most preferred first. Empty uses provider defaults.
    pub models: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub account: AccountId,
    pub provider: ProviderKind,
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unassigned {
    /// Some agent qualifies but is at capacity; retry later.
    Busy,
    /// No configured agent can ever take the role, with reasons per account.
    Impossible(Vec<String>),
}

pub fn select(
    role: &Role,
    agents: &[AgentProfile],
    load: &BTreeMap<AccountId, usize>,
    prefs: &Preferences,
) -> Result<Selection, Unassigned> {
    let pinned = prefs.pinned.get(&role.id);
    let mut reasons = Vec::new();
    let mut candidates = Vec::new();
    for agent in agents {
        let reject = |why: &str| format!("{}: {why}", agent.account);
        if pinned.is_some_and(|p| p != &agent.account) {
            continue;
        }
        if !agent.enabled {
            reasons.push(reject("disabled"));
        } else if prefs.excluded.contains(&agent.account) {
            reasons.push(reject("excluded by preferences"));
        } else if agent.installed != Some(true) {
            reasons.push(reject(match agent.installed {
                None => "provider binary not probed",
                _ => "provider binary not installed",
            }));
        } else if !role.requires.is_subset(&agent.capabilities) {
            reasons.push(reject("missing required capabilities"));
        } else if !prefs.models.is_empty() && model_for(agent, prefs).is_none() {
            reasons.push(reject("none of the preferred models is usable"));
        } else {
            candidates.push(agent);
        }
    }
    if candidates.is_empty() {
        if let Some(account) = pinned
            && reasons.is_empty()
        {
            reasons.push(format!("pinned account {account} is not configured"));
        }
        return Err(Unassigned::Impossible(reasons));
    }
    let rank = |p: &ProviderKind| {
        prefs
            .provider_order
            .iter()
            .position(|o| o == p)
            .unwrap_or(usize::MAX)
    };
    candidates
        .into_iter()
        .filter(|a| load.get(&a.account).copied().unwrap_or(0) < a.max_parallel.max(1))
        .min_by_key(|a| {
            (
                rank(&a.provider),
                load.get(&a.account).copied().unwrap_or(0),
                a.account.clone(),
            )
        })
        .map(|agent| Selection {
            account: agent.account.clone(),
            provider: agent.provider.clone(),
            model: model_for(agent, prefs),
        })
        .ok_or(Unassigned::Busy)
}

/// First preferred model the account has not been told it cannot use.
fn model_for(agent: &AgentProfile, prefs: &Preferences) -> Option<String> {
    prefs
        .models
        .iter()
        .find(|m| {
            agent
                .models
                .get(m)
                .is_some_and(|info| !matches!(info.availability, Availability::Unavailable(_)))
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::role::RoleCatalog;
    use covalos_agents::ConnectionMethod;

    fn agent(id: &str, provider: ProviderKind, caps: &[Capability]) -> AgentProfile {
        let account = Account::new(
            AccountId::new(id).unwrap(),
            provider,
            id,
            ConnectionMethod::SystemCli,
        );
        AgentProfile::from_account(&account, caps.iter().cloned().collect(), Some(true))
    }

    fn rust_role() -> Role {
        RoleCatalog::builtin()
            .get(&RoleId::new("rust").unwrap())
            .unwrap()
            .clone()
    }

    #[test]
    fn capability_preference_and_load() {
        let agents = vec![
            agent("claude", ProviderKind::ClaudeCode, &[Capability::Rust]),
            agent("codex", ProviderKind::Codex, &[Capability::Rust]),
            agent("gemini", ProviderKind::Gemini, &[Capability::Frontend]),
        ];
        let prefs = Preferences {
            provider_order: vec![ProviderKind::Codex],
            ..Preferences::default()
        };
        let mut load = BTreeMap::new();
        assert_eq!(
            select(&rust_role(), &agents, &load, &prefs)
                .unwrap()
                .account
                .as_str(),
            "codex"
        );
        load.insert(AccountId::new("codex").unwrap(), 1);
        assert_eq!(
            select(&rust_role(), &agents, &load, &prefs)
                .unwrap()
                .account
                .as_str(),
            "claude"
        );
        load.insert(AccountId::new("claude").unwrap(), 1);
        assert_eq!(
            select(&rust_role(), &agents, &load, &prefs),
            Err(Unassigned::Busy)
        );
    }

    #[test]
    fn impossible_assignments_explain_themselves() {
        let mut unprobed = agent("a", ProviderKind::ClaudeCode, &[Capability::Rust]);
        unprobed.installed = None;
        let mut disabled = agent("b", ProviderKind::Codex, &[Capability::Rust]);
        disabled.enabled = false;
        let other = agent("c", ProviderKind::Gemini, &[Capability::Review]);
        let Err(Unassigned::Impossible(reasons)) = select(
            &rust_role(),
            &[unprobed, disabled, other],
            &BTreeMap::new(),
            &Preferences::default(),
        ) else {
            panic!("expected impossible");
        };
        assert_eq!(reasons.len(), 3);
        assert!(reasons[0].contains("not probed"));
    }

    #[test]
    fn pins_and_models_are_respected() {
        let mut a = agent("a", ProviderKind::ClaudeCode, &[Capability::Rust]);
        let mut b = agent("b", ProviderKind::ClaudeCode, &[Capability::Rust]);
        a.models.observe("m-large");
        a.models.mark_unavailable("m-large", "not in plan");
        b.models.observe("m-large");
        let prefs = Preferences {
            models: vec!["m-large".into()],
            ..Preferences::default()
        };
        let chosen = select(
            &rust_role(),
            &[a.clone(), b.clone()],
            &BTreeMap::new(),
            &prefs,
        )
        .unwrap();
        assert_eq!(
            (chosen.account.as_str(), chosen.model.as_deref()),
            ("b", Some("m-large"))
        );

        let mut pinned = Preferences::default();
        pinned
            .pinned
            .insert(RoleId::new("rust").unwrap(), AccountId::new("a").unwrap());
        assert_eq!(
            select(&rust_role(), &[a, b], &BTreeMap::new(), &pinned)
                .unwrap()
                .account
                .as_str(),
            "a"
        );
        pinned
            .pinned
            .insert(RoleId::new("rust").unwrap(), AccountId::new("zz").unwrap());
        assert!(matches!(
            select(&rust_role(), &[], &BTreeMap::new(), &pinned),
            Err(Unassigned::Impossible(_))
        ));
    }
}
