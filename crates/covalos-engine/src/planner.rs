//! Goal decomposition. `TemplatePlanner` is a deterministic, offline planner
//! that recognizes common components in English or Spanish goals and lays
//! out a standard delivery pipeline. For anything richer, run an Architect
//! agent and feed its JSON through `PlanSpec::from_json`.

use crate::{
    plan::{PlanSpec, TaskSpec},
    role::RoleCatalog,
};
use covalos_agents::{Error, Result, RoleId, TaskId};

pub trait Planner {
    fn plan(&self, goal: &str, roles: &RoleCatalog) -> Result<PlanSpec>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Component {
    Design,
    Database,
    Backend,
    Frontend,
    Rust,
    Tests,
    Release,
}

const KEYWORDS: &[(Component, &[&str])] = &[
    (
        Component::Design,
        &["design", "diseño", "ux", "mockup", "wireframe"],
    ),
    (
        Component::Database,
        &[
            "database",
            "base de datos",
            "db",
            "sql",
            "postgres",
            "postgresql",
            "sqlite",
            "mysql",
            "schema",
        ],
    ),
    (
        Component::Backend,
        &[
            "backend", "back-end", "api", "server", "servidor", "rest", "graphql",
        ],
    ),
    (
        Component::Frontend,
        &[
            "frontend",
            "front-end",
            "ui",
            "interfaz",
            "web",
            "react",
            "vue",
            "svelte",
            "página",
        ],
    ),
    (Component::Rust, &["rust", "cargo", "crate"]),
    (
        Component::Tests,
        &["test", "tests", "testing", "pruebas", "prueba", "qa"],
    ),
    (
        Component::Release,
        &[
            "release",
            "deploy",
            "publicar",
            "publicación",
            "lanzamiento",
            "versión",
        ],
    ),
];

fn detect(goal: &str) -> Vec<Component> {
    let lower = goal.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|w| !w.is_empty())
        .collect();
    KEYWORDS
        .iter()
        .filter(|(_, keys)| {
            keys.iter().any(|k| {
                if k.contains(' ') {
                    lower.contains(k)
                } else {
                    words.contains(k)
                }
            })
        })
        .map(|(c, _)| *c)
        .collect()
}

pub struct TemplatePlanner;

impl Planner for TemplatePlanner {
    fn plan(&self, goal: &str, roles: &RoleCatalog) -> Result<PlanSpec> {
        let found = detect(goal);
        let has = |c| found.contains(&c);
        let builds = [
            Component::Database,
            Component::Backend,
            Component::Frontend,
            Component::Rust,
        ];
        if !builds.iter().any(|c| has(*c)) {
            return Err(Error::Invalid(
                "the goal names no recognizable component; provide a plan spec instead".into(),
            ));
        }
        let mut tasks = Vec::new();
        let mut add = |id: &str, role: &str, title: &str, instructions: String, deps: &[&str]| {
            tasks.push(TaskSpec {
                id: TaskId::new(id).expect("static id"),
                title: title.into(),
                instructions,
                role: RoleId::new(role).expect("static id"),
                depends_on: deps
                    .iter()
                    .map(|d| TaskId::new(*d).expect("static id"))
                    .collect(),
                budget: None,
                max_attempts: None,
            });
        };
        add(
            "architecture",
            "architect",
            "Architecture and contracts",
            format!(
                "Goal: {goal}\nDefine components, their interfaces and data contracts. \
                 Publish each decision and contract to shared context."
            ),
            &[],
        );
        let mut implementation: Vec<&str> = Vec::new();
        if has(Component::Design) {
            add(
                "ui-design",
                "ui-designer",
                "Interface design",
                "Specify screens, states and components.".into(),
                &["architecture"],
            );
        }
        if has(Component::Database) {
            add(
                "database",
                "backend",
                "Database schema and migrations",
                "Implement the schema and migrations from the data contract.".into(),
                &["architecture"],
            );
            implementation.push("database");
        }
        if has(Component::Backend) {
            let deps: &[&str] = if has(Component::Database) {
                &["architecture", "database"]
            } else {
                &["architecture"]
            };
            add(
                "backend",
                "backend",
                "Backend services",
                "Implement the API exactly as contracted.".into(),
                deps,
            );
            implementation.push("backend");
        }
        if has(Component::Frontend) {
            let deps: &[&str] = if has(Component::Design) {
                &["architecture", "ui-design"]
            } else {
                &["architecture"]
            };
            add(
                "frontend",
                "frontend",
                "Frontend",
                "Implement the views against the API contract.".into(),
                deps,
            );
            implementation.push("frontend");
        }
        if has(Component::Rust) {
            add(
                "rust",
                "rust",
                "Rust implementation",
                "Implement the Rust components with tests.".into(),
                &["architecture"],
            );
            implementation.push("rust");
        }
        let mut review_deps = implementation.clone();
        if has(Component::Tests) {
            add(
                "tests",
                "qa",
                "Test suite",
                "Write and run tests across the implemented components.".into(),
                &implementation,
            );
            review_deps.push("tests");
        }
        add(
            "review",
            "reviewer",
            "Review",
            "Review all changes against the contracts.".into(),
            &review_deps,
        );
        if has(Component::Release) {
            add(
                "release",
                "release",
                "Release preparation",
                "Prepare the release; a person signs off.".into(),
                &["review"],
            );
        }
        PlanSpec {
            goal: goal.to_owned(),
            tasks,
        }
        .resolve(roles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(spec: &PlanSpec) -> Vec<&str> {
        spec.tasks.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn web_app_goal_in_spanish() {
        let spec = TemplatePlanner
            .plan(
                "Construí una aplicación web con frontend, backend, base de datos y pruebas",
                &RoleCatalog::builtin(),
            )
            .unwrap();
        assert_eq!(
            ids(&spec),
            [
                "architecture",
                "database",
                "backend",
                "frontend",
                "tests",
                "review"
            ]
        );
        let frontend = spec
            .tasks
            .iter()
            .find(|t| t.id.as_str() == "frontend")
            .unwrap();
        assert_eq!(
            frontend.depends_on.len(),
            1,
            "frontend runs in parallel with the backend chain"
        );
        let tests = spec
            .tasks
            .iter()
            .find(|t| t.id.as_str() == "tests")
            .unwrap();
        assert_eq!(tests.depends_on.len(), 3);
    }

    #[test]
    fn english_goal_with_design_and_release() {
        let spec = TemplatePlanner
            .plan(
                "Design and build a React UI, then release it",
                &RoleCatalog::builtin(),
            )
            .unwrap();
        assert_eq!(
            ids(&spec),
            ["architecture", "ui-design", "frontend", "review", "release"]
        );
    }

    #[test]
    fn vague_goals_are_refused() {
        assert!(
            TemplatePlanner
                .plan("make it better", &RoleCatalog::builtin())
                .is_err()
        );
    }
}
