use crate::{Account, now_ms};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelSource {
    /// Entered by the user; not yet confirmed by the provider.
    Declared,
    /// Reported by the provider in a session it actually ran.
    Observed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "state", content = "reason")]
pub enum Availability {
    Unknown,
    Available,
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub source: ModelSource,
    pub availability: Availability,
    pub last_seen_ms: Option<u64>,
}

/// Models known for one account. Nothing is assumed available until the
/// provider has run it or rejected it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModelCatalog {
    models: BTreeMap<String, ModelInfo>,
}

impl ModelCatalog {
    pub fn for_account(account: &Account) -> Self {
        let mut catalog = Self::default();
        for id in account.declared_models.iter().chain(&account.default_model) {
            catalog
                .models
                .entry(id.clone())
                .or_insert_with(|| ModelInfo {
                    id: id.clone(),
                    source: ModelSource::Declared,
                    availability: Availability::Unknown,
                    last_seen_ms: None,
                });
        }
        catalog
    }

    pub fn observe(&mut self, id: &str) {
        let info = self
            .models
            .entry(id.to_owned())
            .or_insert_with(|| ModelInfo {
                id: id.to_owned(),
                source: ModelSource::Observed,
                availability: Availability::Unknown,
                last_seen_ms: None,
            });
        info.source = ModelSource::Observed;
        info.availability = Availability::Available;
        info.last_seen_ms = Some(now_ms());
    }

    pub fn mark_unavailable(&mut self, id: &str, reason: impl Into<String>) {
        if let Some(info) = self.models.get_mut(id) {
            info.availability = Availability::Unavailable(reason.into());
        }
    }

    pub fn get(&self, id: &str) -> Option<&ModelInfo> {
        self.models.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModelInfo> {
        self.models.values()
    }
}

/// A usage window exactly as an official provider surface described it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub label: String,
    pub used_ratio: Option<f32>,
    pub resets_at_ms: Option<u64>,
}

/// Quota data is only stored when it comes from an official provider surface.
/// `origin` records which one (e.g. a documented CLI event or API header) so
/// the UI can cite it; there is deliberately no estimated variant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuotaSnapshot {
    pub origin: String,
    pub windows: Vec<QuotaWindow>,
    pub captured_ms: u64,
}

impl QuotaSnapshot {
    pub fn official(origin: impl Into<String>, windows: Vec<QuotaWindow>) -> Self {
        let windows = windows
            .into_iter()
            .map(|mut w| {
                w.used_ratio = w
                    .used_ratio
                    .filter(|r| r.is_finite())
                    .map(|r| r.clamp(0.0, 1.0));
                w
            })
            .collect();
        Self {
            origin: origin.into(),
            windows,
            captured_ms: now_ms(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountId, ConnectionMethod, ProviderKind};

    #[test]
    fn declared_models_start_unknown_and_become_available_when_observed() {
        let mut account = Account::new(
            AccountId::new("a").unwrap(),
            ProviderKind::ClaudeCode,
            "A",
            ConnectionMethod::SystemCli,
        );
        account.declared_models = vec!["m1".into()];
        account.default_model = Some("m1".into());
        let mut catalog = ModelCatalog::for_account(&account);
        assert_eq!(catalog.iter().count(), 1);
        assert_eq!(
            catalog.get("m1").unwrap().availability,
            Availability::Unknown
        );
        catalog.observe("m1");
        catalog.observe("m2");
        assert_eq!(catalog.get("m1").unwrap().source, ModelSource::Observed);
        assert_eq!(
            catalog.get("m2").unwrap().availability,
            Availability::Available
        );
        catalog.mark_unavailable("m2", "not in plan");
        assert!(matches!(
            catalog.get("m2").unwrap().availability,
            Availability::Unavailable(_)
        ));
    }

    #[test]
    fn quota_ratios_are_sanitized() {
        let snapshot = QuotaSnapshot::official(
            "test",
            vec![
                QuotaWindow {
                    label: "a".into(),
                    used_ratio: Some(1.7),
                    resets_at_ms: None,
                },
                QuotaWindow {
                    label: "b".into(),
                    used_ratio: Some(f32::NAN),
                    resets_at_ms: None,
                },
            ],
        );
        assert_eq!(snapshot.windows[0].used_ratio, Some(1.0));
        assert_eq!(snapshot.windows[1].used_ratio, None);
    }
}
