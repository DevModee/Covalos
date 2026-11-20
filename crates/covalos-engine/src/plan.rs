//! Executable plans. Every state change is a `Record`; the live engine and
//! crash recovery both go through `Plan::apply`, so a replayed journal yields
//! exactly the state the engine had.

use crate::{
    budget::{Budget, Spend},
    journal::Record,
    role::RoleCatalog,
};
use covalos_agents::{AccountId, Error, PlanId, Result, RoleId, SessionId, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_TASKS: usize = 100;
const MAX_TASK_ID: usize = 48;
const MAX_SPEC_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: TaskId,
    pub title: String,
    #[serde(default)]
    pub instructions: String,
    pub role: RoleId,
    #[serde(default)]
    pub depends_on: Vec<TaskId>,
    #[serde(default)]
    pub budget: Option<Budget>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
}

/// A goal broken into tasks, as produced by a planner or by an Architect
/// agent's JSON output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanSpec {
    pub goal: String,
    pub tasks: Vec<TaskSpec>,
}

impl PlanSpec {
    pub fn from_json(json: &str) -> Result<Self> {
        if json.len() > MAX_SPEC_BYTES {
            return Err(Error::Invalid("plan spec is too large".into()));
        }
        Ok(serde_json::from_str(json)?)
    }

    /// Checks the spec against the role catalog, fills in each task's
    /// effective budget and attempt limit, and orders tasks topologically.
    pub fn resolve(self, roles: &RoleCatalog) -> Result<Self> {
        if self.goal.trim().is_empty() || self.tasks.is_empty() {
            return Err(Error::Invalid(
                "a plan needs a goal and at least one task".into(),
            ));
        }
        if self.tasks.len() > MAX_TASKS {
            return Err(Error::Invalid(format!(
                "a plan holds at most {MAX_TASKS} tasks"
            )));
        }
        let ids: BTreeSet<_> = self.tasks.iter().map(|t| &t.id).collect();
        if ids.len() != self.tasks.len() {
            return Err(Error::Invalid("task ids must be unique".into()));
        }
        let mut tasks = Vec::with_capacity(self.tasks.len());
        for mut task in self.tasks.iter().cloned() {
            if task.id.as_str().len() > MAX_TASK_ID {
                return Err(Error::Invalid(format!("task id {} is too long", task.id)));
            }
            if task.title.trim().is_empty() {
                return Err(Error::Invalid(format!("task {} needs a title", task.id)));
            }
            if let Some(dep) = task.depends_on.iter().find(|d| !ids.contains(d)) {
                return Err(Error::NotFound(format!(
                    "dependency {dep} of task {}",
                    task.id
                )));
            }
            let role = roles.get(&task.role)?;
            task.budget = Some(match task.budget {
                Some(own) => role.budget.tighter(&own),
                None => role.budget,
            });
            task.max_attempts = Some(task.max_attempts.unwrap_or(role.max_attempts).clamp(1, 10));
            tasks.push(task);
        }
        let mut ordered: Vec<TaskSpec> = Vec::with_capacity(tasks.len());
        let mut placed = BTreeSet::new();
        while !tasks.is_empty() {
            let before = tasks.len();
            let mut index = 0;
            while index < tasks.len() {
                if tasks[index].depends_on.iter().all(|d| placed.contains(d)) {
                    let task = tasks.remove(index);
                    placed.insert(task.id.clone());
                    ordered.push(task);
                } else {
                    index += 1;
                }
            }
            if tasks.len() == before {
                return Err(Error::Invalid("task dependencies contain a cycle".into()));
            }
        }
        Ok(Self {
            goal: self.goal,
            tasks: ordered,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "status")]
pub enum TaskStatus {
    Pending,
    Running,
    AwaitingApproval,
    Retrying { not_before_ms: u64, reason: String },
    Succeeded,
    Failed { reason: String },
    Cancelled,
    Skipped { because: TaskId },
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed { .. } | Self::Cancelled | Self::Skipped { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "state")]
pub enum PlanState {
    Running,
    Paused,
    Cancelled,
    Succeeded,
    Failed { reason: String },
}

impl PlanState {
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            Self::Cancelled | Self::Succeeded | Self::Failed { .. }
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Task {
    pub spec: TaskSpec,
    pub status: TaskStatus,
    pub attempts: u32,
    pub account: Option<AccountId>,
    pub model: Option<String>,
    pub session: Option<SessionId>,
    pub spend: Spend,
}

impl Task {
    pub fn max_attempts(&self) -> u32 {
        self.spec.max_attempts.unwrap_or(1)
    }

    pub fn budget(&self) -> Budget {
        self.spec.budget.unwrap_or_default()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub id: PlanId,
    pub goal: String,
    pub budget: Budget,
    pub state: PlanState,
    pub spend: Spend,
    pub created_ms: u64,
    order: Vec<TaskId>,
    tasks: BTreeMap<TaskId, Task>,
}

impl Plan {
    pub fn from_record(record: &Record) -> Result<Self> {
        let Record::PlanCreated {
            plan,
            goal,
            budget,
            tasks,
            created_ms,
        } = record
        else {
            return Err(Error::Invalid(
                "journal must start with plan-created".into(),
            ));
        };
        Ok(Self {
            id: plan.clone(),
            goal: goal.clone(),
            budget: *budget,
            state: PlanState::Running,
            spend: Spend::default(),
            created_ms: *created_ms,
            order: tasks.iter().map(|t| t.id.clone()).collect(),
            tasks: tasks
                .iter()
                .map(|spec| {
                    let task = Task {
                        spec: spec.clone(),
                        status: TaskStatus::Pending,
                        attempts: 0,
                        account: None,
                        model: None,
                        session: None,
                        spend: Spend::default(),
                    };
                    (spec.id.clone(), task)
                })
                .collect(),
        })
    }

    /// Tasks in dependency order.
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.order.iter().map(|id| &self.tasks[id])
    }

    pub fn task(&self, id: &TaskId) -> Result<&Task> {
        self.tasks
            .get(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    fn task_mut(&mut self, id: &TaskId) -> Result<&mut Task> {
        self.tasks
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    pub fn all_terminal(&self) -> bool {
        self.tasks.values().all(|t| t.status.is_terminal())
    }

    pub fn apply(&mut self, record: &Record) -> Result<()> {
        match record {
            Record::PlanCreated { .. } => {
                return Err(Error::Invalid("plan-created may only appear once".into()));
            }
            Record::TaskStarted {
                task,
                attempt,
                account,
                model,
                session,
            } => {
                let task = self.task_mut(task)?;
                task.status = TaskStatus::Running;
                task.attempts = *attempt;
                task.account = Some(account.clone());
                task.model.clone_from(model);
                task.session = Some(session.clone());
            }
            Record::TaskSpend { task, spend, .. } => {
                self.task_mut(task)?.spend.add(spend);
                self.spend.add(spend);
            }
            Record::TaskWaiting { task, .. } => {
                self.task_mut(task)?.status = TaskStatus::AwaitingApproval;
            }
            Record::TaskResumed { task, .. } => {
                let task = self.task_mut(task)?;
                if task.status == TaskStatus::AwaitingApproval {
                    task.status = TaskStatus::Running;
                }
            }
            Record::TaskFinished { task, status, .. } => {
                self.task_mut(task)?.status = status.clone();
            }
            Record::TaskRequeued { task } => {
                self.task_mut(task)?.status = TaskStatus::Pending;
            }
            Record::TaskInterrupted { task, .. } => {
                let cancelled = self.state.is_settled();
                let task = self.task_mut(task)?;
                task.status = if cancelled {
                    TaskStatus::Cancelled
                } else if task.attempts >= task.max_attempts() {
                    TaskStatus::Failed {
                        reason: "interrupted on its last attempt".into(),
                    }
                } else {
                    TaskStatus::Pending
                };
            }
            Record::Paused => self.state = PlanState::Paused,
            Record::Resumed => self.state = PlanState::Running,
            Record::Cancelled => {
                self.state = PlanState::Cancelled;
                for task in self.tasks.values_mut() {
                    if matches!(
                        task.status,
                        TaskStatus::Pending | TaskStatus::Retrying { .. }
                    ) {
                        task.status = TaskStatus::Cancelled;
                    }
                }
            }
            Record::Settled { state } => self.state = state.clone(),
            Record::Context { .. } | Record::Decision { .. } | Record::Approval { .. } => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, role: &str, deps: &[&str]) -> TaskSpec {
        TaskSpec {
            id: TaskId::new(id).unwrap(),
            title: id.into(),
            instructions: String::new(),
            role: RoleId::new(role).unwrap(),
            depends_on: deps.iter().map(|d| TaskId::new(*d).unwrap()).collect(),
            budget: None,
            max_attempts: None,
        }
    }

    #[test]
    fn resolve_orders_and_fills_limits() {
        let roles = RoleCatalog::builtin();
        let plan = PlanSpec {
            goal: "g".into(),
            tasks: vec![spec("b", "rust", &["a"]), spec("a", "architect", &[])],
        }
        .resolve(&roles)
        .unwrap();
        assert_eq!(plan.tasks[0].id.as_str(), "a");
        assert_eq!(plan.tasks[1].max_attempts, Some(3));
        assert!(plan.tasks[1].budget.unwrap().wall_ms.is_some());
    }

    #[test]
    fn invalid_specs_are_rejected() {
        let roles = RoleCatalog::builtin();
        let bad = |tasks| {
            PlanSpec {
                goal: "g".into(),
                tasks,
            }
            .resolve(&roles)
            .is_err()
        };
        assert!(bad(vec![]));
        assert!(bad(vec![
            spec("a", "rust", &["b"]),
            spec("b", "rust", &["a"])
        ]));
        assert!(bad(vec![spec("a", "rust", &["zz"])]));
        assert!(bad(vec![spec("a", "nope", &[])]));
        assert!(bad(vec![spec("a", "rust", &[]), spec("a", "rust", &[])]));
        assert!(
            PlanSpec::from_json(
                r#"{"goal":"g","tasks":[{"id":"../x","title":"t","role":"rust"}]}"#
            )
            .is_err()
        );
        let parsed =
            PlanSpec::from_json(r#"{"goal":"g","tasks":[{"id":"x","title":"t","role":"rust"}]}"#)
                .unwrap()
                .resolve(&roles)
                .unwrap();
        assert_eq!(parsed.tasks.len(), 1);
    }
}
