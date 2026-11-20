//! Autonomous, provider-neutral orchestration for COVALOS.
//!
//! A goal becomes a validated `PlanSpec`; the `Engine` assigns each task to
//! an agent whose role, capabilities and availability fit, runs independent
//! tasks in parallel, gates every risky action through approvals, enforces
//! budgets and process limits, and journals every decision so it can resume
//! after a crash. Provider specifics live in `covalos-agents`.

pub mod agent;
pub mod budget;
pub mod cli;
pub mod engine;
pub mod executor;
pub mod journal;
pub mod plan;
pub mod planner;
pub mod process;
pub mod role;
pub mod safety;

pub use covalos_agents::{Error, Result};
pub use engine::{Deps, Engine, EngineConfig, Progress, Workspace};
pub use executor::{Assignment, Channel, Executor, Outcome, Report};
pub use plan::{PlanSpec, PlanState, TaskSpec, TaskStatus};
pub use planner::{Planner, TemplatePlanner};
pub use role::{Autonomy, Capability, Criterion, Role, RoleCatalog};
