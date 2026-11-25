//! End-to-end engine behaviour with deterministic agents.

use covalos_agents::{
    Account, AccountId, AgentEvent, ConnectionMethod, PlanId, ProviderKind, RoleId, TaskId,
    context::{Draft, EntryKind},
    event::Usage,
    permission::{Action, Resolution},
    worktree::WorktreeManager,
};
use covalos_engine::{
    Assignment, Capability, Channel, Deps, Engine, EngineConfig, Executor, Outcome, PlanSpec,
    PlanState, Planner, Progress, Report, RoleCatalog, TaskSpec, TaskStatus, TemplatePlanner,
    Workspace,
    agent::{AgentProfile, Preferences},
    budget::Budget,
    journal::{self, Record},
};
use std::{
    collections::HashMap,
    path::Path,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

type Handler = Box<dyn Fn(&Assignment, &Channel) -> Outcome + Send + Sync>;

#[derive(Default)]
struct Scripted {
    handlers: HashMap<String, Handler>,
    runs: Mutex<Vec<(String, u32, String)>>,
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Scripted {
    fn on(
        mut self,
        task: &str,
        handler: impl Fn(&Assignment, &Channel) -> Outcome + Send + Sync + 'static,
    ) -> Self {
        self.handlers.insert(task.into(), Box::new(handler));
        self
    }

    fn runs_of(&self, task: &str) -> usize {
        self.runs
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.0 == task)
            .count()
    }

    fn order(&self) -> Vec<String> {
        self.runs
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.0.clone())
            .collect()
    }
}

/// Does what each built-in role's completion criteria expect.
fn deliver(a: &Assignment, ch: &Channel) -> Outcome {
    let publish = |kind, text: &str| ch.report(Report::Publish(Draft::new(kind, text)));
    match a.role.id.as_str() {
        "architect" => publish(EntryKind::Decision, "REST API with SQLite"),
        "ui-designer" => publish(EntryKind::Handoff, "screens specified"),
        "reviewer" => publish(EntryKind::Review, "looks correct"),
        "qa" => publish(EntryKind::TestResult, "12 passed"),
        "frontend" | "backend" => {
            let name = format!("{}.txt", a.task.id);
            std::fs::write(a.workdir.join(&name), "work").unwrap();
            ch.report(Report::FileChanged(name.into()));
        }
        _ => {}
    }
    ch.report(Report::Event(AgentEvent::Usage(Usage {
        input_tokens: 10,
        output_tokens: 5,
    })));
    Outcome::Succeeded {
        summary: format!("{} done", a.task.id),
    }
}

impl Executor for Scripted {
    fn execute(&self, a: &Assignment, ch: &Channel) -> Outcome {
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        self.runs
            .lock()
            .unwrap()
            .push((a.task.id.to_string(), a.attempt, a.account.to_string()));
        let outcome = match self.handlers.get(a.task.id.as_str()) {
            Some(handler) => handler(a, ch),
            None => deliver(a, ch),
        };
        self.active.fetch_sub(1, Ordering::SeqCst);
        outcome
    }
}

fn agents() -> Vec<AgentProfile> {
    let all: Vec<Capability> = vec![
        Capability::Planning,
        Capability::Frontend,
        Capability::Backend,
        Capability::Database,
        Capability::Rust,
        Capability::UiDesign,
        Capability::Review,
        Capability::Testing,
        Capability::Release,
    ];
    [
        ("claude", ProviderKind::ClaudeCode),
        ("codex", ProviderKind::Codex),
        ("gemini", ProviderKind::Gemini),
    ]
    .into_iter()
    .map(|(id, provider)| {
        let account = Account::new(
            AccountId::new(id).unwrap(),
            provider,
            id,
            ConnectionMethod::SystemCli,
        );
        let mut profile =
            AgentProfile::from_account(&account, all.iter().cloned().collect(), Some(true));
        profile.max_parallel = 2;
        profile
    })
    .collect()
}

fn deps(executor: Arc<dyn Executor>, dir: &Path) -> Deps {
    Deps {
        roles: RoleCatalog::builtin(),
        agents: agents(),
        prefs: Preferences::default(),
        executor,
        workspace: Workspace::Directory(dir.to_path_buf()),
        config: EngineConfig {
            retry_base_ms: 5,
            retry_max_ms: 20,
            ..EngineConfig::default()
        },
    }
}

fn task(id: &str, role: &str, deps: &[&str]) -> TaskSpec {
    TaskSpec {
        id: TaskId::new(id).unwrap(),
        title: format!("Task {id}"),
        instructions: String::new(),
        role: RoleId::new(role).unwrap(),
        depends_on: deps.iter().map(|d| TaskId::new(*d).unwrap()).collect(),
        budget: None,
        max_attempts: None,
    }
}

fn spec(tasks: Vec<TaskSpec>) -> PlanSpec {
    PlanSpec {
        goal: "test goal".into(),
        tasks,
    }
}

fn status(engine: &Engine, id: &str) -> TaskStatus {
    engine
        .plan()
        .task(&TaskId::new(id).unwrap())
        .unwrap()
        .status
        .clone()
}

fn start(spec: PlanSpec, budget: Budget, executor: Arc<Scripted>, dir: &Path) -> Engine {
    Engine::start(
        PlanId::new("plan").unwrap(),
        spec,
        budget,
        &dir.join("journal.jsonl"),
        deps(executor, dir),
    )
    .unwrap()
}

const LONG: Duration = Duration::from_secs(20);

#[test]
fn web_app_goal_runs_end_to_end_and_replays_identically() {
    let dir = tempfile::tempdir().unwrap();
    let goal = "Construí una aplicación web con frontend, backend, base de datos y pruebas";
    let plan = TemplatePlanner.plan(goal, &RoleCatalog::builtin()).unwrap();
    let executor = Arc::new(Scripted::default().on("database", |a, ch| {
        ch.sleep(Duration::from_millis(150));
        deliver(a, ch)
    }));
    let mut engine = start(plan, Budget::default(), executor.clone(), dir.path());
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );

    let order = executor.order();
    let position = |id: &str| order.iter().position(|t| t == id).unwrap();
    assert_eq!(order[0], "architecture");
    assert!(
        position("frontend") < position("backend"),
        "frontend overlaps the database task"
    );
    assert!(executor.peak.load(Ordering::SeqCst) >= 2);
    assert!(position("tests") < position("review"));
    let accounts: std::collections::BTreeSet<_> = executor
        .runs
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.2.clone())
        .collect();
    assert!(accounts.len() > 1, "work is spread across providers");
    assert_eq!(
        engine.context().of_kind(EntryKind::TaskCompleted).count(),
        6
    );
    assert_eq!(engine.plan().spend.tokens, 6 * 15);

    let live_statuses: Vec<_> = engine.plan().tasks().map(|t| t.status.clone()).collect();
    let live_entries = engine.context().latest_seq();
    let journal = engine.journal_path().to_path_buf();
    drop(engine);
    let recovered = Engine::recover(&journal, deps(executor.clone(), dir.path())).unwrap();
    assert_eq!(
        recovered
            .plan()
            .tasks()
            .map(|t| t.status.clone())
            .collect::<Vec<_>>(),
        live_statuses
    );
    assert_eq!(recovered.plan().state, PlanState::Succeeded);
    assert_eq!(recovered.plan().spend.tokens, 6 * 15);
    assert_eq!(recovered.context().latest_seq(), live_entries);
}

#[test]
fn parallelism_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    // Long enough that all three slots fill before the first task ends, even on a
    // loaded CI runner; at 120 ms the peak was sometimes 2.
    let slow = |a: &Assignment, ch: &Channel| {
        ch.sleep(Duration::from_millis(400));
        deliver(a, ch)
    };
    let mut executor = Scripted::default();
    for id in ["a", "b", "c", "d", "e"] {
        executor = executor.on(id, slow);
    }
    let executor = Arc::new(executor);
    let mut deps = deps(executor.clone(), dir.path());
    deps.config.max_parallel = 3;
    let tasks = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|id| task(id, "backend", &[]))
        .collect();
    let mut engine = Engine::start(
        PlanId::new("p").unwrap(),
        spec(tasks),
        Budget::default(),
        &dir.path().join("j"),
        deps,
    )
    .unwrap();
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );
    assert_eq!(executor.peak.load(Ordering::SeqCst), 3);
}

#[test]
fn retries_recover_transient_failures() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default().on("a", |a, ch| {
        if a.attempt < 3 {
            Outcome::Failed {
                reason: "network hiccup".into(),
                retryable: true,
            }
        } else {
            deliver(a, ch)
        }
    }));
    let mut engine = start(
        spec(vec![
            task("a", "backend", &[]),
            task("b", "backend", &["a"]),
        ]),
        Budget::default(),
        executor.clone(),
        dir.path(),
    );
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );
    assert_eq!(executor.runs_of("a"), 3);
    assert_eq!(
        engine
            .plan()
            .task(&TaskId::new("a").unwrap())
            .unwrap()
            .attempts,
        3
    );
}

#[test]
fn permanent_failures_skip_dependents() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default().on("a", |_, _| Outcome::Failed {
        reason: "spec is contradictory".into(),
        retryable: false,
    }));
    let tasks = vec![
        task("a", "backend", &[]),
        task("b", "backend", &["a"]),
        task("c", "qa", &["b"]),
        task("d", "qa", &[]),
    ];
    let mut engine = start(spec(tasks), Budget::default(), executor.clone(), dir.path());
    let Progress::Settled(PlanState::Failed { reason }) = engine.run_until(LONG).unwrap() else {
        panic!("the plan should fail");
    };
    assert!(
        reason.starts_with("3 of 4 task(s) did not succeed"),
        "{reason}"
    );
    assert!(reason.contains("(a): spec is contradictory"), "{reason}");
    assert!(reason.contains("(b): skipped because a failed"), "{reason}");
    assert_eq!(executor.runs_of("a"), 1);
    assert!(matches!(status(&engine, "b"), TaskStatus::Skipped { .. }));
    assert!(matches!(status(&engine, "c"), TaskStatus::Skipped { .. }));
    assert_eq!(status(&engine, "d"), TaskStatus::Succeeded);
    assert_eq!(engine.context().of_kind(EntryKind::Blocker).count(), 1);
}

#[test]
fn completion_criteria_are_verified_not_trusted() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default().on("a", |_, _| Outcome::Succeeded {
        summary: "trust me".into(),
    }));
    let mut engine = start(
        spec(vec![task("a", "backend", &[])]),
        Budget::default(),
        executor.clone(),
        dir.path(),
    );
    engine.run_until(LONG).unwrap();
    assert_eq!(executor.runs_of("a"), 3, "retried up to the role's limit");
    let TaskStatus::Failed { reason } = status(&engine, "a") else {
        panic!()
    };
    assert!(reason.contains("no files were changed"), "{reason}");
}

#[test]
fn task_budgets_stop_runaway_work() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(
        Scripted::default()
            .on("tokens", |_, ch| {
                ch.report(Report::Event(AgentEvent::Usage(Usage {
                    input_tokens: 100,
                    output_tokens: 50,
                })));
                ch.sleep(Duration::from_secs(10));
                Outcome::Cancelled
            })
            .on("clock", |_, ch| {
                ch.sleep(Duration::from_secs(10));
                Outcome::Cancelled
            }),
    );
    let mut tokens = task("tokens", "backend", &[]);
    tokens.budget = Some(Budget {
        tokens: Some(100),
        ..Budget::default()
    });
    let mut clock = task("clock", "backend", &[]);
    clock.budget = Some(Budget {
        wall_ms: Some(150),
        ..Budget::default()
    });
    let started = Instant::now();
    let mut engine = start(
        spec(vec![tokens, clock]),
        Budget::default(),
        executor.clone(),
        dir.path(),
    );
    engine.run_until(LONG).unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(
        status(&engine, "tokens"),
        TaskStatus::Failed {
            reason: "Tokens budget exceeded".into()
        }
    );
    assert_eq!(
        status(&engine, "clock"),
        TaskStatus::Failed {
            reason: "Wall budget exceeded".into()
        }
    );
    assert_eq!(
        executor.runs_of("tokens"),
        1,
        "budget failures are not retried"
    );
}

#[test]
fn plan_budget_aborts_the_whole_plan() {
    let dir = tempfile::tempdir().unwrap();
    let spend = |_: &Assignment, ch: &Channel| {
        ch.report(Report::Event(AgentEvent::Usage(Usage {
            input_tokens: 40,
            output_tokens: 0,
        })));
        ch.sleep(Duration::from_secs(10));
        Outcome::Cancelled
    };
    let executor = Arc::new(Scripted::default().on("a", spend).on("b", spend));
    let tasks = vec![
        task("a", "backend", &[]),
        task("b", "backend", &[]),
        task("c", "backend", &["a"]),
    ];
    let budget = Budget {
        tokens: Some(50),
        ..Budget::default()
    };
    let mut engine = start(spec(tasks), budget, executor, dir.path());
    let progress = engine.run_until(LONG).unwrap();
    assert!(
        matches!(progress, Progress::Settled(PlanState::Failed { ref reason }) if reason.contains("budget")),
        "{progress:?}"
    );
    assert_eq!(status(&engine, "c"), TaskStatus::Cancelled);
}

#[test]
fn risky_actions_go_through_policy_and_people() {
    let dir = tempfile::tempdir().unwrap();
    let answers = Arc::new(Mutex::new(Vec::new()));
    let log = answers.clone();
    let executor = Arc::new(Scripted::default().on("a", move |a, ch| {
        for action in [
            Action::Execute("npm test".into()),
            Action::Execute("rm -rf /".into()),
            Action::Write("../outside.txt".into()),
            Action::Execute("npm install left-pad".into()),
            Action::Execute("npm install left-pad".into()),
            Action::Read(".env".into()),
        ] {
            log.lock().unwrap().push(ch.request(action));
        }
        deliver(a, ch)
    }));
    let mut engine = start(
        spec(vec![task("a", "backend", &[])]),
        Budget::default(),
        executor,
        dir.path(),
    );

    for resolution in [
        Resolution::AllowForSession,
        Resolution::Deny,
        Resolution::AllowOnce,
    ] {
        assert_eq!(engine.run_until(LONG).unwrap(), Progress::AwaitingApproval);
        assert_eq!(status(&engine, "a"), TaskStatus::AwaitingApproval);
        let pending = engine.pending_approvals();
        assert_eq!(pending.len(), 1);
        let ticket = pending[0].0.ticket.clone();
        engine.resolve(&ticket, resolution).unwrap();
    }
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );
    assert_eq!(
        *answers.lock().unwrap(),
        [true, false, false, true, false, true]
    );

    let records = journal::read(engine.journal_path()).unwrap();
    let waits = records
        .iter()
        .filter(|r| matches!(r, Record::TaskWaiting { .. }))
        .count();
    assert_eq!(
        waits, 3,
        "an approved sensitive command is asked again next time"
    );
}

#[test]
fn approvals_expire_as_denials() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default().on("a", |a, ch| {
        if ch.request(Action::Execute("git push".into())) {
            panic!("expired approval must deny");
        }
        deliver(a, ch)
    }));
    let mut deps = deps(executor, dir.path());
    // Long enough to observe the pending approval before it expires on a slow runner;
    // at 50 ms the first check sometimes found it already expired.
    deps.config.approval_ttl_ms = 500;
    let mut engine = Engine::start(
        PlanId::new("p").unwrap(),
        spec(vec![task("a", "backend", &[])]),
        Budget::default(),
        &dir.path().join("j"),
        deps,
    )
    .unwrap();
    assert_eq!(engine.run_until(LONG).unwrap(), Progress::AwaitingApproval);
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );
}

#[test]
fn releases_need_a_human_sign_off() {
    for (resolution, expected) in [(Resolution::AllowOnce, true), (Resolution::Deny, false)] {
        let dir = tempfile::tempdir().unwrap();
        let executor = Arc::new(Scripted::default());
        let mut engine = start(
            spec(vec![task("ship", "release", &[])]),
            Budget::default(),
            executor,
            dir.path(),
        );
        assert_eq!(engine.run_until(LONG).unwrap(), Progress::AwaitingApproval);
        let ticket = engine.pending_approvals()[0].0.ticket.clone();
        assert_eq!(
            engine.pending_approvals()[0].0.action,
            Action::Tool("sign-off:ship".into())
        );
        engine.resolve(&ticket, resolution).unwrap();
        let progress = engine.run_until(LONG).unwrap();
        assert_eq!(
            progress == Progress::Settled(PlanState::Succeeded),
            expected,
            "{progress:?}"
        );
    }
}

#[test]
fn pause_resume_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default().on("slow", |_, ch| {
        ch.sleep(Duration::from_secs(30));
        Outcome::Cancelled
    }));
    let tasks = vec![
        task("a", "backend", &[]),
        task("slow", "backend", &["a"]),
        task("after", "backend", &["slow"]),
    ];
    let mut engine = start(spec(tasks), Budget::default(), executor.clone(), dir.path());
    engine.pause().unwrap();
    assert_eq!(engine.run_until(LONG).unwrap(), Progress::Paused);
    assert!(executor.order().is_empty());
    engine.resume().unwrap();
    assert_eq!(
        engine.run_until(Duration::from_millis(300)).unwrap(),
        Progress::TimedOut
    );
    assert_eq!(status(&engine, "slow"), TaskStatus::Running);

    let started = Instant::now();
    engine.cancel().unwrap();
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Cancelled)
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(status(&engine, "a"), TaskStatus::Succeeded);
    assert_eq!(status(&engine, "slow"), TaskStatus::Cancelled);
    assert_eq!(status(&engine, "after"), TaskStatus::Cancelled);
}

#[test]
fn recovers_interrupted_work_after_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default().on("b", |a, ch| {
        if a.attempt == 1 {
            ch.sleep(Duration::from_secs(30));
            return Outcome::Cancelled;
        }
        deliver(a, ch)
    }));
    let tasks = vec![
        task("a", "backend", &[]),
        task("b", "backend", &["a"]),
        task("c", "backend", &["b"]),
    ];
    let mut engine = start(spec(tasks), Budget::default(), executor.clone(), dir.path());
    assert_eq!(
        engine.run_until(Duration::from_millis(300)).unwrap(),
        Progress::TimedOut
    );
    assert_eq!(status(&engine, "b"), TaskStatus::Running);
    let journal = engine.journal_path().to_path_buf();
    drop(engine);

    let mut engine = Engine::recover(&journal, deps(executor.clone(), dir.path())).unwrap();
    assert_eq!(status(&engine, "a"), TaskStatus::Succeeded);
    assert_eq!(status(&engine, "b"), TaskStatus::Pending);
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );
    assert_eq!(
        (
            executor.runs_of("a"),
            executor.runs_of("b"),
            executor.runs_of("c")
        ),
        (1, 2, 1)
    );
    let records = journal::read(&journal).unwrap();
    assert!(
        records
            .iter()
            .any(|r| matches!(r, Record::TaskInterrupted { .. }))
    );
}

#[test]
fn recovery_reasks_pending_sign_off_without_rerunning() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default());
    let mut engine = start(
        spec(vec![task("ship", "release", &[])]),
        Budget::default(),
        executor.clone(),
        dir.path(),
    );
    assert_eq!(engine.run_until(LONG).unwrap(), Progress::AwaitingApproval);
    let journal = engine.journal_path().to_path_buf();
    drop(engine);
    let mut engine = Engine::recover(&journal, deps(executor.clone(), dir.path())).unwrap();
    assert_eq!(engine.run_until(LONG).unwrap(), Progress::AwaitingApproval);
    let ticket = engine.pending_approvals()[0].0.ticket.clone();
    engine.resolve(&ticket, Resolution::AllowOnce).unwrap();
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );
    assert_eq!(executor.runs_of("ship"), 1);
}

#[test]
fn unassignable_roles_fail_with_reasons() {
    let dir = tempfile::tempdir().unwrap();
    let mut deps = deps(Arc::new(Scripted::default()), dir.path());
    for agent in &mut deps.agents {
        agent.capabilities = [Capability::Frontend].into();
    }
    deps.agents[2].installed = None;
    let mut engine = Engine::start(
        PlanId::new("p").unwrap(),
        spec(vec![task("a", "backend", &[])]),
        Budget::default(),
        &dir.path().join("j"),
        deps,
    )
    .unwrap();
    engine.run_until(LONG).unwrap();
    let TaskStatus::Failed { reason } = status(&engine, "a") else {
        panic!()
    };
    assert!(
        reason.contains("missing required capabilities") && reason.contains("not probed"),
        "{reason}"
    );
}

#[test]
fn secrets_never_reach_shared_context() {
    let dir = tempfile::tempdir().unwrap();
    let executor = Arc::new(Scripted::default().on("a", |a, ch| {
        ch.report(Report::Publish(Draft::new(
            EntryKind::Finding,
            "OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwx",
        )));
        deliver(a, ch)
    }));
    let mut engine = start(
        spec(vec![task("a", "backend", &[])]),
        Budget::default(),
        executor,
        dir.path(),
    );
    engine.run_until(LONG).unwrap();
    assert_eq!(engine.context().of_kind(EntryKind::Finding).count(), 0);
    let raw = std::fs::read_to_string(engine.journal_path()).unwrap();
    assert!(!raw.contains("sk-proj-"));
    assert!(raw.contains("refused context entry"));
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn worktrees_isolate_tasks_and_dirty_ones_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README"), "x").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let executor = Arc::new(Scripted::default().on("build", |a, _| {
        std::fs::write(a.workdir.join("feature.rs"), "fn main() {}").unwrap();
        Outcome::Succeeded {
            summary: "built".into(),
        }
    }));
    let mut deps = deps(executor, dir.path());
    deps.workspace = Workspace::Worktrees {
        manager: WorktreeManager::new(&repo, dir.path().join("trees")),
        base_ref: "main".into(),
    };
    let tasks = vec![
        task("build", "backend", &[]),
        task("check", "reviewer", &["build"]),
    ];
    let mut engine = Engine::start(
        PlanId::new("p").unwrap(),
        spec(tasks),
        Budget::default(),
        &dir.path().join("j"),
        deps,
    )
    .unwrap();
    assert_eq!(
        engine.run_until(LONG).unwrap(),
        Progress::Settled(PlanState::Succeeded)
    );
    assert!(dir.path().join("trees/build/feature.rs").is_file());
    assert!(
        !repo.join("feature.rs").exists(),
        "work stays out of the main checkout"
    );

    let results: HashMap<_, _> = engine
        .release_worktrees()
        .unwrap()
        .into_iter()
        .map(|(id, r)| (id.to_string(), r))
        .collect();
    assert!(results["build"].is_err(), "dirty worktree is kept");
    assert!(results["check"].is_ok());
    assert!(dir.path().join("trees/build/feature.rs").is_file());
}

#[cfg(unix)]
mod cli {
    use super::*;
    use covalos_agents::{ProviderRegistry, credentials::MemoryStore, profile::ProfileStore};
    use covalos_engine::{Autonomy, Role, cli::CliExecutor, process::ProcessLimits};
    use std::os::unix::fs::PermissionsExt;

    fn fake_claude(dir: &Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("fake-claude");
        std::fs::write(
            &path,
            format!("#!/bin/sh\necho \"$@\" > args.txt\ncat > prompt.txt\n{body}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn cli_engine(dir: &Path, body: &str) -> Engine {
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let account = Account::new(
            AccountId::new("claude").unwrap(),
            ProviderKind::ClaudeCode,
            "Claude",
            ConnectionMethod::SystemCli,
        );
        let mut profiles = ProfileStore::open(dir.join("profiles")).unwrap();
        profiles
            .add(account.clone(), &ProviderRegistry::builtin())
            .unwrap();
        let executor = CliExecutor {
            registry: ProviderRegistry::builtin(),
            profiles: Arc::new(profiles),
            credentials: Arc::new(Mutex::new(MemoryStore::default())),
            programs: [(ProviderKind::ClaudeCode, fake_claude(dir, body))].into(),
            limits: ProcessLimits::default(),
            claude_settings: None,
            acp_execute: None,
        };
        let mut roles = RoleCatalog::builtin();
        roles
            .register(Role {
                id: RoleId::new("auditor").unwrap(),
                name: "Auditor".into(),
                responsibilities: vec!["Summarize the repository".into()],
                requires: [Capability::Custom("audit".into())].into(),
                grants: Vec::new(),
                autonomy: Autonomy::Advisory,
                done_when: Vec::new(),
                budget: Budget::default(),
                max_attempts: 1,
            })
            .unwrap();
        let profile = AgentProfile::from_account(
            &account,
            [Capability::Custom("audit".into())].into(),
            Some(true),
        );
        let deps = Deps {
            roles,
            agents: vec![profile],
            prefs: Preferences::default(),
            executor: Arc::new(executor),
            workspace: Workspace::Directory(work),
            config: EngineConfig::default(),
        };
        let mut audit = task("audit", "auditor", &[]);
        audit.title = "Audit the repository".into();
        Engine::start(
            PlanId::new("cli").unwrap(),
            spec(vec![audit]),
            Budget::default(),
            &dir.join("j"),
            deps,
        )
        .unwrap()
    }

    #[test]
    fn runs_a_provider_cli_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = cli_engine(
            dir.path(),
            r#"echo '{"type":"system","subtype":"init","session_id":"fake-1","model":"fake-model"}'
echo "{\"type\":\"result\",\"result\":\"done ${CARGO_PKG_NAME:-scrubbed}\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5},\"total_cost_usd\":0.002}""#,
        );
        assert_eq!(
            engine.run_until(LONG).unwrap(),
            Progress::Settled(PlanState::Succeeded)
        );
        let work = dir.path().join("work");
        assert!(
            std::fs::read_to_string(work.join("prompt.txt"))
                .unwrap()
                .contains("Audit the repository")
        );
        let args = std::fs::read_to_string(work.join("args.txt")).unwrap();
        assert!(
            args.contains("--output-format stream-json") && args.contains("--permission-mode plan"),
            "{args}"
        );
        let spend = engine.plan().spend;
        assert_eq!(
            (spend.tokens, spend.cost_micros, spend.cost_reported),
            (15, 2_000, true)
        );
        let completed: Vec<_> = engine
            .context()
            .of_kind(EntryKind::TaskCompleted)
            .map(|e| e.text.clone())
            .collect();
        assert_eq!(
            completed,
            ["done scrubbed"],
            "host environment was not inherited"
        );
        let session = engine.sessions().iter().next().unwrap();
        assert_eq!(session.model.as_deref(), Some("fake-model"));
    }

    #[test]
    fn provider_errors_and_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = cli_engine(
            dir.path(),
            r#"echo '{"type":"result","is_error":true,"result":"not signed in"}'"#,
        );
        engine.run_until(LONG).unwrap();
        let TaskStatus::Failed { reason } = status(&engine, "audit") else {
            panic!()
        };
        assert!(reason.contains("not signed in"), "{reason}");

        let dir = tempfile::tempdir().unwrap();
        let mut engine = cli_engine(dir.path(), "sleep 30");
        assert_eq!(
            engine.run_until(Duration::from_millis(300)).unwrap(),
            Progress::TimedOut
        );
        let started = Instant::now();
        engine.cancel().unwrap();
        assert_eq!(
            engine.run_until(LONG).unwrap(),
            Progress::Settled(PlanState::Cancelled)
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the provider process was killed"
        );
    }
}
