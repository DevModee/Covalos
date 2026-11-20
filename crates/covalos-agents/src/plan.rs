//! Task graph for future autonomous orchestration. The graph only tracks
//! dependencies, assignment and status; scheduling policy stays with the
//! orchestrator that consumes it.

use crate::{AccountId, Error, Result, SessionId, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub depends_on: Vec<TaskId>,
    pub preferred_account: Option<AccountId>,
    pub session: Option<SessionId>,
    pub status: TaskStatus,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskGraph {
    tasks: BTreeMap<TaskId, Task>,
}

impl TaskGraph {
    /// Dependencies must already exist, which also rules out cycles.
    pub fn add(&mut self, id: TaskId, title: &str, depends_on: Vec<TaskId>) -> Result<()> {
        if self.tasks.contains_key(&id) {
            return Err(Error::Duplicate(format!("task {id}")));
        }
        if let Some(missing) = depends_on.iter().find(|d| !self.tasks.contains_key(*d)) {
            return Err(Error::NotFound(format!("dependency {missing}")));
        }
        self.tasks.insert(
            id.clone(),
            Task {
                id,
                title: title.to_owned(),
                depends_on,
                preferred_account: None,
                session: None,
                status: TaskStatus::Pending,
            },
        );
        Ok(())
    }

    pub fn get(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id)
    }

    /// Pending tasks whose dependencies are all done.
    pub fn ready(&self) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .filter(|t| {
                t.depends_on
                    .iter()
                    .all(|d| self.tasks[d].status == TaskStatus::Done)
            })
            .collect()
    }

    pub fn start(&mut self, id: &TaskId, session: SessionId) -> Result<()> {
        if !self.ready().iter().any(|t| &t.id == id) {
            return Err(Error::Invalid(format!("task {id} is not ready")));
        }
        let task = self.tasks.get_mut(id).expect("ready task exists");
        task.status = TaskStatus::Running;
        task.session = Some(session);
        Ok(())
    }

    /// Finishing a task as failed skips everything that depends on it.
    pub fn finish(&mut self, id: &TaskId, success: bool) -> Result<()> {
        let task = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;
        if task.status != TaskStatus::Running {
            return Err(Error::Invalid(format!("task {id} is not running")));
        }
        task.status = if success {
            TaskStatus::Done
        } else {
            TaskStatus::Failed
        };
        if !success {
            let mut blocked = vec![id.clone()];
            while let Some(parent) = blocked.pop() {
                for task in self.tasks.values_mut() {
                    if task.status == TaskStatus::Pending && task.depends_on.contains(&parent) {
                        task.status = TaskStatus::Skipped;
                        blocked.push(task.id.clone());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn is_settled(&self) -> bool {
        self.tasks
            .values()
            .all(|t| !matches!(t.status, TaskStatus::Pending | TaskStatus::Running))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> TaskId {
        TaskId::new(s).unwrap()
    }

    #[test]
    fn dependencies_gate_readiness() {
        let mut graph = TaskGraph::default();
        graph.add(id("a"), "A", vec![]).unwrap();
        graph.add(id("b"), "B", vec![]).unwrap();
        graph.add(id("c"), "C", vec![id("a"), id("b")]).unwrap();
        assert!(graph.add(id("d"), "D", vec![id("zz")]).is_err());
        assert_eq!(graph.ready().len(), 2);
        assert!(graph.start(&id("c"), SessionId::new("s").unwrap()).is_err());
        for t in ["a", "b"] {
            graph.start(&id(t), SessionId::new(t).unwrap()).unwrap();
            graph.finish(&id(t), true).unwrap();
        }
        assert_eq!(graph.ready()[0].id, id("c"));
    }

    #[test]
    fn failures_skip_dependents() {
        let mut graph = TaskGraph::default();
        graph.add(id("a"), "A", vec![]).unwrap();
        graph.add(id("b"), "B", vec![id("a")]).unwrap();
        graph.add(id("c"), "C", vec![id("b")]).unwrap();
        graph.start(&id("a"), SessionId::new("s").unwrap()).unwrap();
        graph.finish(&id("a"), false).unwrap();
        assert_eq!(graph.get(&id("c")).unwrap().status, TaskStatus::Skipped);
        assert!(graph.is_settled());
    }
}
