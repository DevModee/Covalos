//! Specialized roles. A role is an executable contract: what the agent must
//! deliver, which capabilities it needs, what it may do without asking, how
//! autonomous it is, how completion is verified and what it may spend.

use crate::{
    budget::Budget,
    safety::{Risk, classify},
};
use covalos_agents::{
    Error, Result, RoleId,
    context::EntryKind,
    launch::Sandbox,
    permission::{Action, Preset},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Planning,
    Frontend,
    Backend,
    Database,
    Rust,
    UiDesign,
    Review,
    Testing,
    Release,
    Custom(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Autonomy {
    /// Reads and reports; never edits files.
    Advisory,
    /// Edits need approval.
    Supervised,
    /// Edits inside its own worktree without asking.
    Worktree,
}

impl Autonomy {
    pub fn preset(self) -> Preset {
        match self {
            Self::Advisory => Preset::ReadOnly,
            Self::Supervised => Preset::Supervised,
            Self::Worktree => Preset::Worktree,
        }
    }

    /// The provider-side sandbox matching this autonomy level.
    pub fn sandbox(self) -> Sandbox {
        match self {
            Self::Advisory => Sandbox::ReadOnly,
            Self::Supervised | Self::Worktree => Sandbox::WorkspaceWrite,
        }
    }
}

/// Conditions the engine checks itself before accepting a task as done.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "check", content = "arg")]
pub enum Criterion {
    /// A workspace-relative path exists after the task.
    PathExists(PathBuf),
    /// The task left changes in its workspace.
    FilesChanged,
    /// The task's session published at least one entry of this kind.
    Published(EntryKind),
    /// The command exits successfully in the workspace. It must be one of the
    /// role's granted commands, so it never needs an ad-hoc approval.
    Command(String),
    /// A person accepts the result.
    HumanSignOff,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: RoleId,
    pub name: String,
    pub responsibilities: Vec<String>,
    pub requires: BTreeSet<Capability>,
    /// Actions pre-approved for this role. Only `Safe` commands qualify.
    pub grants: Vec<Action>,
    pub autonomy: Autonomy,
    pub done_when: Vec<Criterion>,
    pub budget: Budget,
    pub max_attempts: u32,
}

impl Role {
    pub fn validate(&self) -> Result<()> {
        let fail = |reason: String| Err(Error::Invalid(format!("role {}: {reason}", self.id)));
        if self.name.trim().is_empty() || self.responsibilities.is_empty() {
            return fail("needs a name and at least one responsibility".into());
        }
        if self.requires.is_empty() {
            return fail("must require at least one capability".into());
        }
        if !(1..=10).contains(&self.max_attempts) {
            return fail("max_attempts must be between 1 and 10".into());
        }
        for grant in &self.grants {
            match grant {
                Action::Execute(cmd) if classify(cmd) != Risk::Safe => {
                    return fail(format!("cannot pre-approve non-safe command {cmd:?}"));
                }
                Action::Write(_) | Action::Read(_) | Action::Network(_) => {
                    return fail(format!(
                        "only commands and tools can be granted, not {grant:?}"
                    ));
                }
                _ => {}
            }
        }
        for criterion in &self.done_when {
            match criterion {
                Criterion::Command(cmd) if !self.grants.contains(&Action::Execute(cmd.clone())) => {
                    return fail(format!("completion command {cmd:?} must also be granted"));
                }
                Criterion::PathExists(path)
                    if path.as_os_str().is_empty()
                        || !path.components().all(|c| matches!(c, Component::Normal(_))) =>
                {
                    return fail(format!("{} is not workspace-relative", path.display()));
                }
                Criterion::FilesChanged if self.autonomy == Autonomy::Advisory => {
                    return fail("advisory roles cannot be required to change files".into());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct RoleCatalog {
    roles: BTreeMap<RoleId, Role>,
}

impl RoleCatalog {
    pub fn builtin() -> Self {
        let mut catalog = Self::default();
        for role in builtin_roles() {
            catalog.register(role).expect("built-in roles are valid");
        }
        catalog
    }

    pub fn register(&mut self, role: Role) -> Result<()> {
        role.validate()?;
        if self.roles.contains_key(&role.id) {
            return Err(Error::Duplicate(format!("role {}", role.id)));
        }
        self.roles.insert(role.id.clone(), role);
        Ok(())
    }

    /// Replaces an existing role, e.g. after the user edited a built-in.
    pub fn update(&mut self, role: Role) -> Result<()> {
        role.validate()?;
        let slot = self
            .roles
            .get_mut(&role.id)
            .ok_or_else(|| Error::NotFound(format!("role {}", role.id)))?;
        *slot = role;
        Ok(())
    }

    pub fn get(&self, id: &RoleId) -> Result<&Role> {
        self.roles
            .get(id)
            .ok_or_else(|| Error::NotFound(format!("role {id}")))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Role> {
        self.roles.values()
    }
}

fn base(
    id: &str,
    name: &str,
    responsibilities: &[&str],
    requires: Capability,
    autonomy: Autonomy,
) -> Role {
    Role {
        id: RoleId::new(id).expect("static id"),
        name: name.into(),
        responsibilities: responsibilities.iter().map(|r| (*r).into()).collect(),
        requires: [requires].into(),
        grants: Vec::new(),
        autonomy,
        done_when: Vec::new(),
        budget: minutes(30),
        max_attempts: 2,
    }
}

fn commands(list: &[&str]) -> Vec<Action> {
    list.iter().map(|c| Action::Execute((*c).into())).collect()
}

fn minutes(m: u64) -> Budget {
    Budget {
        wall_ms: Some(m * 60_000),
        ..Budget::default()
    }
}

/// Defaults are starting points; users edit them with `RoleCatalog::update`.
/// Budgets cover wall time only: token and cost limits depend on the user's
/// plan and are left for them to set.
fn builtin_roles() -> Vec<Role> {
    use {Autonomy::*, Capability::*, Criterion::*};
    const TESTS: &[&str] = &[
        "npm test",
        "cargo test",
        "go test ./...",
        "python -m pytest",
    ];
    vec![
        Role {
            grants: commands(&["git status", "git log --oneline"]),
            done_when: vec![Published(EntryKind::Decision)],
            budget: minutes(20),
            ..base(
                "architect",
                "Architect",
                &[
                    "Turn the goal into components, boundaries and interfaces",
                    "Record architecture decisions and module contracts in shared context",
                    "Flag risks and open questions as blockers",
                ],
                Planning,
                Advisory,
            )
        },
        Role {
            grants: commands(&["npm test", "npm run build", "npm run lint"]),
            done_when: vec![FilesChanged],
            budget: minutes(45),
            max_attempts: 3,
            ..base(
                "frontend",
                "Frontend Engineer",
                &[
                    "Implement user-facing views against the published contracts",
                    "Keep components accessible and responsive",
                    "Add tests for interactive behaviour",
                ],
                Frontend,
                Worktree,
            )
        },
        Role {
            grants: commands(TESTS),
            done_when: vec![FilesChanged],
            budget: minutes(45),
            max_attempts: 3,
            ..base(
                "backend",
                "Backend Engineer",
                &[
                    "Implement services, APIs and data access exactly as contracted",
                    "Own validation and error handling",
                    "Document any contract change as a new decision",
                ],
                Backend,
                Worktree,
            )
        },
        Role {
            grants: commands(&[
                "cargo fmt --check",
                "cargo clippy --all-targets",
                "cargo test",
            ]),
            done_when: vec![FilesChanged, Command("cargo test".into())],
            budget: minutes(45),
            max_attempts: 3,
            ..base(
                "rust",
                "Rust Engineer",
                &[
                    "Write idiomatic, safe Rust with explicit error handling",
                    "Keep clippy clean and the build warning-free",
                    "Cover new behaviour with unit tests",
                ],
                Rust,
                Worktree,
            )
        },
        Role {
            done_when: vec![Published(EntryKind::Handoff)],
            ..base(
                "ui-designer",
                "UI Designer",
                &[
                    "Define layout, hierarchy, spacing and states for each screen",
                    "Keep the design system consistent",
                    "Hand specifications to frontend through shared context",
                ],
                UiDesign,
                Supervised,
            )
        },
        Role {
            grants: commands(&["git diff", "git log --oneline"]),
            done_when: vec![Published(EntryKind::Review)],
            budget: minutes(20),
            ..base(
                "reviewer",
                "Reviewer",
                &[
                    "Review changes for correctness, security and contract compliance",
                    "Report findings with file references; never edit code",
                ],
                Review,
                Advisory,
            )
        },
        Role {
            grants: commands(TESTS),
            done_when: vec![Published(EntryKind::TestResult)],
            budget: minutes(40),
            max_attempts: 3,
            ..base(
                "qa",
                "QA Engineer",
                &[
                    "Write and run tests against the contracts",
                    "Report results, including failures, as test results",
                ],
                Testing,
                Worktree,
            )
        },
        Role {
            grants: commands(&["cargo build --release", "npm run build"]),
            done_when: vec![HumanSignOff],
            max_attempts: 1,
            ..base(
                "release",
                "Release Engineer",
                &[
                    "Prepare changelog, version bump and release notes",
                    "Verify the build; never publish without a person signing off",
                ],
                Release,
                Supervised,
            )
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_cover_the_eight_roles() {
        let catalog = RoleCatalog::builtin();
        let ids: Vec<_> = catalog.iter().map(|r| r.id.as_str().to_owned()).collect();
        assert_eq!(
            ids,
            [
                "architect",
                "backend",
                "frontend",
                "qa",
                "release",
                "reviewer",
                "rust",
                "ui-designer"
            ]
        );
        let release = catalog.get(&RoleId::new("release").unwrap()).unwrap();
        assert!(release.done_when.contains(&Criterion::HumanSignOff));
    }

    #[test]
    fn custom_roles_are_validated() {
        let mut catalog = RoleCatalog::builtin();
        let mut custom = catalog.get(&RoleId::new("rust").unwrap()).unwrap().clone();
        assert!(catalog.register(custom.clone()).is_err());
        custom.id = RoleId::new("docs").unwrap();
        custom.requires = [Capability::Custom("docs".into())].into();
        catalog.register(custom.clone()).unwrap();

        let mut bad = custom.clone();
        bad.id = RoleId::new("bad").unwrap();
        bad.grants.push(Action::Execute("git push".into()));
        assert!(catalog.register(bad).is_err());

        let mut bad = custom.clone();
        bad.id = RoleId::new("bad").unwrap();
        bad.done_when = vec![Criterion::Command("make deploy".into())];
        assert!(catalog.register(bad).is_err());

        let mut bad = custom.clone();
        bad.id = RoleId::new("bad").unwrap();
        bad.autonomy = Autonomy::Advisory;
        assert!(catalog.register(bad).is_err());

        let mut bad = custom;
        bad.id = RoleId::new("bad").unwrap();
        bad.done_when = vec![Criterion::PathExists("../escape".into())];
        assert!(catalog.register(bad).is_err());
    }
}
