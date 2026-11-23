//! The orchestration loop. The host drives it with `tick` (from a UI frame or
//! timer) or `run_until`; work happens on worker threads, and every state
//! change is journaled before it takes effect.

use crate::{
    agent::{AgentProfile, Preferences, Unassigned, select},
    budget::{Budget, Limit, Spend},
    executor::{Assignment, Body, CancelToken, Channel, Executor, Message, Outcome, Report},
    journal::{Journal, Record, Resolver},
    plan::{Plan, PlanSpec, PlanState, TaskStatus},
    process::{ProcessLimits, isolated, run_limited},
    role::{Criterion, Role, RoleCatalog},
    safety::{Risk, classify, touches_secrets},
};
use covalos_agents::{
    AgentEvent, Error, PlanId, Result, SessionId, TaskId,
    context::{Draft, EntryKind, Reference, SharedContext},
    now_ms,
    permission::{Action, ApprovalQueue, ApprovalRequest, Decision, Policy, Resolution},
    session::{Session, SessionRegistry},
    worktree::WorktreeManager,
};
use std::{
    collections::{BTreeMap, HashMap},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub max_parallel: usize,
    pub approval_ttl_ms: u64,
    pub retry_base_ms: u64,
    pub retry_max_ms: u64,
    pub context_capacity: usize,
    pub context_entry_chars: usize,
    pub context_digest_chars: usize,
    /// Limits for completion-criteria commands the engine runs itself.
    pub command_limits: ProcessLimits,
    /// Reasoning effort requested from every agent (`low`, `medium`, `high`).
    /// `None` keeps each provider's default.
    pub effort: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_parallel: 4,
            approval_ttl_ms: 30 * 60_000,
            retry_base_ms: 2_000,
            retry_max_ms: 60_000,
            context_capacity: 256,
            context_entry_chars: 2_000,
            context_digest_chars: 8_000,
            command_limits: ProcessLimits {
                wall: Duration::from_secs(10 * 60),
                max_output_bytes: 256 * 1024,
            },
            effort: None,
        }
    }
}

pub enum Workspace {
    /// Every task works in the same directory.
    Directory(PathBuf),
    /// Each task gets its own git worktree, kept across retries.
    Worktrees {
        manager: WorktreeManager,
        base_ref: String,
    },
}

pub struct Deps {
    pub roles: RoleCatalog,
    pub agents: Vec<AgentProfile>,
    pub prefs: Preferences,
    pub executor: Arc<dyn Executor>,
    pub workspace: Workspace,
    pub config: EngineConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    Settled(PlanState),
    AwaitingApproval,
    Paused,
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Budget(Limit),
    Plan,
}

struct Live {
    attempt: u32,
    session: SessionId,
    workdir: PathBuf,
    cancel: CancelToken,
    handle: Option<JoinHandle<()>>,
    started: Instant,
    stop: Option<Stop>,
}

enum Waiter {
    Action(Sender<bool>),
    SignOff,
}

struct Waiting {
    task: TaskId,
    waiter: Waiter,
}

pub struct Engine {
    plan: Plan,
    roles: RoleCatalog,
    agents: Vec<AgentProfile>,
    prefs: Preferences,
    executor: Arc<dyn Executor>,
    workspace: Workspace,
    config: EngineConfig,
    journal: Journal,
    context: SharedContext,
    sessions: SessionRegistry,
    approvals: ApprovalQueue,
    waiting: HashMap<String, Waiting>,
    policies: HashMap<TaskId, Policy>,
    workdirs: HashMap<TaskId, PathBuf>,
    live: BTreeMap<TaskId, Live>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
}

fn sign_off_action(task: &TaskId) -> Action {
    Action::Tool(format!("sign-off:{task}"))
}

impl Engine {
    /// Validates the spec, creates the journal and records the plan.
    pub fn start(
        id: PlanId,
        spec: PlanSpec,
        budget: Budget,
        journal: &Path,
        deps: Deps,
    ) -> Result<Self> {
        let spec = spec.resolve(&deps.roles)?;
        let record = Record::PlanCreated {
            plan: id,
            goal: spec.goal.clone(),
            budget,
            tasks: spec.tasks,
            created_ms: now_ms(),
        };
        let plan = Plan::from_record(&record)?;
        let mut journal = Journal::create(journal)?;
        journal.append(&record)?;
        let mut engine = Self::assemble(plan, journal, deps);
        let goal = engine.plan.goal.clone();
        engine.publish(Draft::new(EntryKind::Goal, goal).pinned())?;
        Ok(engine)
    }

    /// Rebuilds the engine from its journal after a restart or crash.
    /// Attempts that were in flight are marked interrupted and re-queued
    /// within their attempt limit; pending sign-offs are asked again.
    pub fn recover(journal: &Path, deps: Deps) -> Result<Self> {
        let (journal, records) = Journal::open(journal)?;
        let first = records
            .first()
            .ok_or_else(|| Error::Invalid("journal is empty".into()))?;
        let plan = Plan::from_record(first)?;
        let mut engine = Self::assemble(plan, journal, deps);
        let mut last_wait: HashMap<TaskId, Action> = HashMap::new();
        for record in &records[1..] {
            match record {
                Record::Context { draft } => {
                    engine.context.publish(draft.clone())?;
                }
                Record::TaskWaiting { task, action, .. } => {
                    last_wait.insert(task.clone(), action.clone());
                }
                _ => {}
            }
            engine.plan.apply(record)?;
        }
        let in_flight: Vec<_> = engine
            .plan
            .tasks()
            .filter(|t| matches!(t.status, TaskStatus::Running | TaskStatus::AwaitingApproval))
            .map(|t| (t.spec.id.clone(), t.attempts, t.session.clone()))
            .collect();
        for (task, attempt, session) in in_flight {
            let signing = last_wait.get(&task) == Some(&sign_off_action(&task));
            match session {
                Some(session) if signing && !engine.plan.state.is_settled() => {
                    engine.ask_sign_off(&task, session)?;
                }
                _ => engine.commit(Record::TaskInterrupted { task, attempt })?,
            }
        }
        engine.commit(Record::Decision {
            task: None,
            text: format!("recovered from journal with {} records", records.len()),
        })?;
        Ok(engine)
    }

    fn assemble(plan: Plan, journal: Journal, deps: Deps) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            context: SharedContext::new(
                deps.config.context_capacity,
                deps.config.context_entry_chars,
            ),
            plan,
            roles: deps.roles,
            agents: deps.agents,
            prefs: deps.prefs,
            executor: deps.executor,
            workspace: deps.workspace,
            config: deps.config,
            journal,
            sessions: SessionRegistry::default(),
            approvals: ApprovalQueue::default(),
            waiting: HashMap::new(),
            policies: HashMap::new(),
            workdirs: HashMap::new(),
            live: BTreeMap::new(),
            tx,
            rx,
        }
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn context(&self) -> &SharedContext {
        &self.context
    }

    pub fn sessions(&self) -> &SessionRegistry {
        &self.sessions
    }

    pub fn journal_path(&self) -> &Path {
        self.journal.path()
    }

    /// Approvals waiting for a person: ticket, task and requested action.
    pub fn pending_approvals(&self) -> Vec<(&ApprovalRequest, &TaskId)> {
        self.approvals
            .pending()
            .iter()
            .filter_map(|r| self.waiting.get(&r.ticket).map(|w| (r, &w.task)))
            .collect()
    }

    /// Adds an entry from the user or orchestrator to shared context.
    pub fn publish(&mut self, draft: Draft) -> Result<u64> {
        let seq = self.context.publish(draft.clone())?;
        self.journal.append(&Record::Context { draft })?;
        Ok(seq)
    }

    fn commit(&mut self, record: Record) -> Result<()> {
        self.journal.append(&record)?;
        self.plan.apply(&record)
    }

    fn decision(&mut self, task: Option<&TaskId>, text: String) -> Result<()> {
        self.commit(Record::Decision {
            task: task.cloned(),
            text,
        })
    }

    pub fn pause(&mut self) -> Result<()> {
        if self.plan.state == PlanState::Running {
            self.commit(Record::Paused)?;
        }
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        if self.plan.state == PlanState::Paused {
            self.commit(Record::Resumed)?;
        }
        Ok(())
    }

    /// Stops scheduling, cancels running work and denies open approvals.
    pub fn cancel(&mut self) -> Result<()> {
        self.abort(None)
    }

    fn abort(&mut self, failure: Option<String>) -> Result<()> {
        if self.plan.state.is_settled() {
            return Ok(());
        }
        self.commit(Record::Cancelled)?;
        if let Some(reason) = failure {
            self.commit(Record::Settled {
                state: PlanState::Failed { reason },
            })?;
        }
        for live in self.live.values_mut() {
            live.stop.get_or_insert(Stop::Plan);
            live.cancel.cancel();
        }
        let tickets: Vec<String> = self.waiting.keys().cloned().collect();
        for ticket in tickets {
            self.close_wait(&ticket, false, Resolver::Cancelled)?;
        }
        Ok(())
    }

    pub fn resolve(&mut self, ticket: &str, resolution: Resolution) -> Result<()> {
        let waiting = self
            .waiting
            .get(ticket)
            .ok_or_else(|| Error::NotFound(format!("approval {ticket}")))?;
        let allowed = match waiting.waiter {
            Waiter::SignOff => resolution != Resolution::Deny,
            Waiter::Action(_) => {
                let policy = self
                    .policies
                    .get_mut(&waiting.task)
                    .ok_or_else(|| Error::NotFound(format!("policy for task {}", waiting.task)))?;
                self.approvals.resolve(ticket, resolution, policy)?.1
            }
        };
        self.close_wait(ticket, allowed, Resolver::User)
    }

    fn close_wait(&mut self, ticket: &str, allowed: bool, by: Resolver) -> Result<()> {
        let Some(waiting) = self.waiting.remove(ticket) else {
            return Ok(());
        };
        let task = waiting.task;
        self.approvals.cancel_ticket(ticket);
        match waiting.waiter {
            Waiter::Action(reply) => {
                let _ = reply.send(allowed);
                self.commit(Record::TaskResumed {
                    task: task.clone(),
                    ticket: ticket.to_owned(),
                    allowed,
                    by,
                })?;
                if let Some(live) = self.live.get(&task) {
                    let _ = self.sessions.apply(
                        &live.session,
                        &AgentEvent::ApprovalResolved {
                            ticket: ticket.to_owned(),
                            allowed,
                        },
                    );
                }
            }
            Waiter::SignOff => {
                let attempt = self.plan.task(&task)?.attempts;
                let status = match (allowed, by) {
                    (true, _) => TaskStatus::Succeeded,
                    (false, Resolver::Cancelled) => TaskStatus::Cancelled,
                    (false, Resolver::Expired) => TaskStatus::Failed {
                        reason: "sign-off expired".into(),
                    },
                    (false, _) => TaskStatus::Failed {
                        reason: "sign-off declined".into(),
                    },
                };
                self.finish(&task, attempt, status)?;
            }
        }
        Ok(())
    }

    /// Processes everything that is ready without blocking.
    pub fn tick(&mut self) -> Result<()> {
        while let Ok(message) = self.rx.try_recv() {
            self.handle(message)?;
        }
        self.enforce_budgets()?;
        for request in self.approvals.expire(now_ms()) {
            self.close_wait(&request.ticket, false, Resolver::Expired)?;
        }
        self.requeue_retries()?;
        self.schedule()?;
        self.settle()
    }

    /// Drives the plan until it settles, needs a person, is paused with no
    /// work in flight, or `timeout` elapses.
    pub fn run_until(&mut self, timeout: Duration) -> Result<Progress> {
        let deadline = Instant::now() + timeout;
        loop {
            self.tick()?;
            if self.live.is_empty() && self.plan.state.is_settled() {
                return Ok(Progress::Settled(self.plan.state.clone()));
            }
            let all_waiting = self.live.keys().all(|t| {
                self.plan
                    .task(t)
                    .is_ok_and(|t| t.status == TaskStatus::AwaitingApproval)
            });
            if !self.waiting.is_empty() && all_waiting {
                return Ok(Progress::AwaitingApproval);
            }
            if self.live.is_empty() && self.plan.state == PlanState::Paused {
                return Ok(Progress::Paused);
            }
            if Instant::now() >= deadline {
                return Ok(Progress::TimedOut);
            }
            if let Ok(message) = self.rx.recv_timeout(Duration::from_millis(10)) {
                self.handle(message)?;
            }
        }
    }

    fn handle(&mut self, message: Message) -> Result<()> {
        let current = self
            .live
            .get(&message.task)
            .is_some_and(|l| l.attempt == message.attempt);
        if !current {
            if let Body::Approval { reply, .. } = message.body {
                let _ = reply.send(false);
            }
            return Ok(());
        }
        let task = message.task;
        match message.body {
            Body::Report(report) => self.on_report(&task, report),
            Body::Approval { action, reply } => self.on_approval(&task, action, reply),
            Body::Done(outcome) => self.on_done(&task, outcome),
        }
    }

    fn on_report(&mut self, task: &TaskId, report: Report) -> Result<()> {
        let live = &self.live[task];
        let (session, workdir, attempt) =
            (live.session.clone(), live.workdir.clone(), live.attempt);
        match report {
            Report::Event(event) => {
                let _ = self.sessions.apply(&session, &event);
                let spend = match event {
                    AgentEvent::Usage(usage) => Spend {
                        tokens: usage.input_tokens + usage.output_tokens,
                        ..Spend::default()
                    },
                    AgentEvent::Cost { usd_micros } => Spend {
                        cost_micros: usd_micros,
                        cost_reported: true,
                        ..Spend::default()
                    },
                    _ => return Ok(()),
                };
                self.commit(Record::TaskSpend {
                    task: task.clone(),
                    attempt,
                    spend,
                })?;
                self.enforce_budgets()
            }
            Report::FileChanged(path) => {
                let relative = path.components().all(|c| matches!(c, Component::Normal(_)));
                let decision = self.policies[task].evaluate(&Action::Write(path.clone()));
                if !relative || decision == Decision::Deny {
                    return self.blocker(
                        task,
                        &session,
                        format!(
                            "reported a change outside its workspace: {}",
                            path.display()
                        ),
                    );
                }
                let mut draft =
                    Draft::new(EntryKind::FileChange, path.display().to_string()).by(session);
                if let Ok(reference) = Reference::file(&workdir, &path) {
                    draft = draft.with_ref(reference);
                }
                self.publish(draft).map(drop)
            }
            Report::Publish(mut draft) => {
                draft.author = Some(session);
                draft.pinned = false;
                if let Err(error) = self.publish(draft) {
                    self.decision(Some(task), format!("refused context entry: {error}"))?;
                }
                Ok(())
            }
        }
    }

    fn blocker(&mut self, task: &TaskId, session: &SessionId, text: String) -> Result<()> {
        self.decision(Some(task), text.clone())?;
        self.publish(Draft::new(EntryKind::Blocker, text).by(session.clone()))
            .map(drop)
    }

    /// Engine-level gate. Forbidden commands are refused, sensitive ones always
    /// reach a person (session grants never cover them), and everything else
    /// follows the task's policy.
    fn decide(&self, task: &TaskId, action: &Action) -> Decision {
        let policy = &self.policies[task];
        let by_policy = policy.evaluate(action);
        if by_policy == Decision::Deny {
            return Decision::Deny;
        }
        let sensitive = match action {
            Action::Execute(command) => match classify(command) {
                Risk::Forbidden(_) => return Decision::Deny,
                Risk::Sensitive(_) => true,
                Risk::Safe => false,
            },
            Action::Read(path) | Action::Write(path) => touches_secrets(&path.to_string_lossy()),
            Action::Network(_) | Action::Tool(_) => false,
        };
        if sensitive { Decision::Ask } else { by_policy }
    }

    fn on_approval(&mut self, task: &TaskId, action: Action, reply: Sender<bool>) -> Result<()> {
        match self.decide(task, &action) {
            Decision::Ask => {
                let session = self.live[task].session.clone();
                let ticket = self.approvals.request(
                    session.clone(),
                    action.clone(),
                    now_ms(),
                    self.config.approval_ttl_ms,
                );
                self.waiting.insert(
                    ticket.clone(),
                    Waiting {
                        task: task.clone(),
                        waiter: Waiter::Action(reply),
                    },
                );
                let _ = self.sessions.apply(
                    &session,
                    &AgentEvent::ApprovalRequested {
                        ticket: ticket.clone(),
                        summary: format!("{action:?}"),
                    },
                );
                self.commit(Record::TaskWaiting {
                    task: task.clone(),
                    ticket,
                    action,
                })
            }
            decision => {
                let allowed = decision == Decision::Allow;
                let _ = reply.send(allowed);
                self.commit(Record::Approval {
                    task: task.clone(),
                    action,
                    allowed,
                    by: Resolver::Policy,
                })
            }
        }
    }

    fn on_done(&mut self, task: &TaskId, outcome: Outcome) -> Result<()> {
        let mut live = self.live.remove(task).expect("current attempt is live");
        if let Some(handle) = live.handle.take() {
            let _ = handle.join();
        }
        let wall = Spend {
            wall_ms: live
                .started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            ..Spend::default()
        };
        self.commit(Record::TaskSpend {
            task: task.clone(),
            attempt: live.attempt,
            spend: wall,
        })?;
        for request in self.approvals.cancel_session(&live.session) {
            self.close_wait(&request.ticket, false, Resolver::Cancelled)?;
        }
        let status = match (live.stop, outcome) {
            (Some(Stop::Budget(limit)), _) => TaskStatus::Failed {
                reason: format!("{limit:?} budget exceeded"),
            },
            (Some(Stop::Plan), _) | (None, Outcome::Cancelled) => TaskStatus::Cancelled,
            (None, Outcome::Failed { reason, retryable }) => {
                self.retry_or_fail(task, reason, retryable)?
            }
            (None, Outcome::Succeeded { summary }) => match self.verify(task, &live)? {
                Ok(())
                    if self
                        .role_of(task)?
                        .done_when
                        .contains(&Criterion::HumanSignOff) =>
                {
                    let _ = self
                        .sessions
                        .apply(&live.session, &AgentEvent::TurnCompleted);
                    self.publish(Draft::new(EntryKind::Handoff, summary).by(live.session.clone()))?;
                    return self.ask_sign_off(task, live.session);
                }
                Ok(()) => {
                    let text = if summary.trim().is_empty() {
                        "completed".to_owned()
                    } else {
                        summary
                    };
                    self.publish(
                        Draft::new(EntryKind::TaskCompleted, text)
                            .by(live.session.clone())
                            .with_ref(Reference::Task { id: task.clone() }),
                    )?;
                    TaskStatus::Succeeded
                }
                Err(unmet) => {
                    self.retry_or_fail(task, format!("completion check failed: {unmet}"), true)?
                }
            },
        };
        let event = match &status {
            TaskStatus::Succeeded => AgentEvent::TurnCompleted,
            TaskStatus::Cancelled => AgentEvent::Cancelled,
            _ => AgentEvent::Failed {
                message: format!("{status:?}"),
            },
        };
        let _ = self.sessions.apply(&live.session, &event);
        let _ = self.sessions.apply(&live.session, &AgentEvent::Closed);
        if let TaskStatus::Failed { reason } = &status {
            self.publish(
                Draft::new(EntryKind::Blocker, format!("{task} failed: {reason}")).by(live.session),
            )?;
        }
        self.finish(task, live.attempt, status)
    }

    fn finish(&mut self, task: &TaskId, attempt: u32, status: TaskStatus) -> Result<()> {
        if status.is_terminal() {
            self.policies.remove(task);
        }
        self.commit(Record::TaskFinished {
            task: task.clone(),
            attempt,
            status,
        })
    }

    fn ask_sign_off(&mut self, task: &TaskId, session: SessionId) -> Result<()> {
        let action = sign_off_action(task);
        let ticket = self.approvals.request(
            session,
            action.clone(),
            now_ms(),
            self.config.approval_ttl_ms,
        );
        self.waiting.insert(
            ticket.clone(),
            Waiting {
                task: task.clone(),
                waiter: Waiter::SignOff,
            },
        );
        self.commit(Record::TaskWaiting {
            task: task.clone(),
            ticket,
            action,
        })
    }

    fn retry_or_fail(
        &mut self,
        task: &TaskId,
        reason: String,
        retryable: bool,
    ) -> Result<TaskStatus> {
        let task = self.plan.task(task)?;
        if retryable && task.attempts < task.max_attempts() {
            let exponent = task.attempts.saturating_sub(1).min(16);
            let delay = self
                .config
                .retry_base_ms
                .saturating_mul(1 << exponent)
                .min(self.config.retry_max_ms);
            Ok(TaskStatus::Retrying {
                not_before_ms: now_ms() + delay,
                reason,
            })
        } else {
            Ok(TaskStatus::Failed { reason })
        }
    }

    fn role_of(&self, task: &TaskId) -> Result<&Role> {
        self.roles.get(&self.plan.task(task)?.spec.role)
    }

    /// Checks the role's completion criteria. The outer error is an engine
    /// failure; the inner one explains which criterion is unmet.
    fn verify(&mut self, task: &TaskId, live: &Live) -> Result<std::result::Result<(), String>> {
        let criteria = self.role_of(task)?.done_when.clone();
        for criterion in criteria {
            let unmet = match &criterion {
                Criterion::PathExists(path) => (!live.workdir.join(path).exists())
                    .then(|| format!("{} does not exist", path.display())),
                Criterion::FilesChanged => {
                    let reported = self
                        .context
                        .of_kind(EntryKind::FileChange)
                        .any(|e| e.author.as_ref() == Some(&live.session));
                    let dirty = matches!(self.workspace, Workspace::Worktrees { .. })
                        && run_limited(
                            isolated(
                                "git",
                                &[
                                    "-C",
                                    &live.workdir.to_string_lossy(),
                                    "status",
                                    "--porcelain",
                                ],
                            ),
                            self.config.command_limits,
                        )
                        .is_ok_and(|f| f.success && !f.output.trim().is_empty());
                    (!reported && !dirty).then(|| "no files were changed".to_owned())
                }
                Criterion::Published(kind) => {
                    let found = self
                        .context
                        .of_kind(*kind)
                        .any(|e| e.author.as_ref() == Some(&live.session));
                    (!found).then(|| format!("nothing was published as {kind:?}"))
                }
                Criterion::Command(command) => {
                    let action = Action::Execute(command.clone());
                    if self.decide(task, &action) != Decision::Allow {
                        Some(format!("`{command}` is not authorized for this role"))
                    } else {
                        let words: Vec<&str> = command.split_whitespace().collect();
                        let mut process = isolated(words[0], &words[1..]);
                        process.current_dir(&live.workdir);
                        let result = run_limited(process, self.config.command_limits);
                        let (passed, detail) = match &result {
                            Ok(f) if f.success => (true, "passed".to_owned()),
                            Ok(f) if f.timed_out => (false, "timed out".to_owned()),
                            Ok(f) => (false, tail(&f.output, 600)),
                            Err(error) => (false, error.to_string()),
                        };
                        self.publish(
                            Draft::new(EntryKind::TestResult, format!("`{command}`: {detail}"))
                                .by(live.session.clone()),
                        )?;
                        (!passed).then(|| format!("`{command}` did not pass"))
                    }
                }
                Criterion::HumanSignOff => None,
            };
            if let Some(unmet) = unmet {
                return Ok(Err(unmet));
            }
        }
        Ok(Ok(()))
    }

    fn enforce_budgets(&mut self) -> Result<()> {
        let mut plan_spend = self.plan.spend;
        let mut over = Vec::new();
        for (id, live) in &self.live {
            let elapsed: u64 = live
                .started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX);
            plan_spend.wall_ms += elapsed;
            let task = self.plan.task(id)?;
            let mut spend = task.spend;
            spend.wall_ms += elapsed;
            if live.stop.is_none()
                && let Some(limit) = task.budget().exceeded(&spend)
            {
                over.push((id.clone(), limit));
            }
        }
        for (id, limit) in over {
            let live = self.live.get_mut(&id).expect("live task");
            live.stop = Some(Stop::Budget(limit));
            live.cancel.cancel();
            self.decision(Some(&id), format!("stopped: {limit:?} budget exceeded"))?;
        }
        if let Some(limit) = self.plan.budget.exceeded(&plan_spend)
            && !self.plan.state.is_settled()
        {
            self.abort(Some(format!("plan {limit:?} budget exceeded")))?;
        }
        Ok(())
    }

    fn requeue_retries(&mut self) -> Result<()> {
        let now = now_ms();
        let due: Vec<TaskId> = self
            .plan
            .tasks()
            .filter(|t| matches!(t.status, TaskStatus::Retrying { not_before_ms, .. } if not_before_ms <= now))
            .map(|t| t.spec.id.clone())
            .collect();
        for task in due {
            self.commit(Record::TaskRequeued { task })?;
        }
        Ok(())
    }

    fn schedule(&mut self) -> Result<()> {
        if self.plan.state != PlanState::Running {
            return Ok(());
        }
        let mut load: BTreeMap<_, usize> = BTreeMap::new();
        for live in self.live.keys() {
            if let Some(account) = &self.plan.task(live)?.account {
                *load.entry(account.clone()).or_default() += 1;
            }
        }
        let pending: Vec<TaskId> = self
            .plan
            .tasks()
            .filter(|t| t.status == TaskStatus::Pending)
            .map(|t| t.spec.id.clone())
            .collect();
        for id in pending {
            let task = self.plan.task(&id)?;
            let mut ready = true;
            let mut blocked_by = None;
            for dep in &task.spec.depends_on {
                match &self.plan.task(dep)?.status {
                    TaskStatus::Succeeded => {}
                    status if status.is_terminal() => blocked_by = Some(dep.clone()),
                    _ => ready = false,
                }
            }
            if let Some(because) = blocked_by {
                let attempt = task.attempts;
                self.finish(&id, attempt, TaskStatus::Skipped { because })?;
                continue;
            }
            if !ready || self.live.len() >= self.config.max_parallel.max(1) {
                continue;
            }
            let role = match self.roles.get(&task.spec.role) {
                Ok(role) => role.clone(),
                Err(error) => {
                    let attempt = task.attempts;
                    self.finish(
                        &id,
                        attempt,
                        TaskStatus::Failed {
                            reason: error.to_string(),
                        },
                    )?;
                    continue;
                }
            };
            match select(&role, &self.agents, &load, &self.prefs) {
                Ok(selection) => {
                    *load.entry(selection.account.clone()).or_default() += 1;
                    self.start_attempt(&id, role, selection)?;
                }
                Err(Unassigned::Busy) => {}
                Err(Unassigned::Impossible(reasons)) => {
                    let attempt = task.attempts;
                    let reason =
                        format!("no agent can take role {}: {}", role.id, reasons.join("; "));
                    self.finish(&id, attempt, TaskStatus::Failed { reason })?;
                }
            }
        }
        Ok(())
    }

    fn workdir(&mut self, task: &TaskId) -> Result<PathBuf> {
        if let Some(dir) = self.workdirs.get(task) {
            return Ok(dir.clone());
        }
        let dir = match &self.workspace {
            Workspace::Directory(dir) => dir.clone(),
            Workspace::Worktrees { manager, base_ref } => {
                let key = SessionId::new(task.as_str())?;
                let path = manager.path_for(&key);
                if path.is_dir() {
                    path
                } else {
                    manager.create(&key, base_ref)?.path
                }
            }
        };
        self.workdirs.insert(task.clone(), dir.clone());
        Ok(dir)
    }

    fn start_attempt(
        &mut self,
        id: &TaskId,
        role: Role,
        selection: crate::agent::Selection,
    ) -> Result<()> {
        let attempt = self.plan.task(id)?.attempts + 1;
        let workdir = match self.workdir(id) {
            Ok(dir) => dir,
            Err(error) => {
                return self.finish(
                    id,
                    attempt - 1,
                    TaskStatus::Failed {
                        reason: format!("workspace unavailable: {error}"),
                    },
                );
            }
        };
        let mut policy = Policy::new(&workdir, role.autonomy.preset())?;
        for grant in &role.grants {
            policy.grant(grant.clone());
        }
        self.policies.insert(id.clone(), policy);
        let session = SessionId::new(format!("{id}-{attempt}"))?;
        self.commit(Record::TaskStarted {
            task: id.clone(),
            attempt,
            account: selection.account.clone(),
            model: selection.model.clone(),
            session: session.clone(),
        })?;
        let mut record = Session::new(
            session.clone(),
            selection.account.clone(),
            Some(workdir.clone()),
        );
        record.begin_turn()?;
        self.sessions.insert(record)?;

        let assignment = Assignment {
            goal: self.plan.goal.clone(),
            task: self.plan.task(id)?.spec.clone(),
            role,
            account: selection.account,
            provider: selection.provider,
            model: selection.model,
            workdir: workdir.clone(),
            context: self
                .context
                .digest(Some(&session), self.config.context_digest_chars),
            attempt,
            session: session.clone(),
            effort: self.config.effort.clone(),
        };
        let cancel = CancelToken::default();
        let channel = Channel::new(self.tx.clone(), id.clone(), attempt, cancel.clone());
        let executor = Arc::clone(&self.executor);
        let handle = std::thread::Builder::new()
            .name(format!("covalos-{id}"))
            .spawn(move || {
                let outcome =
                    catch_unwind(AssertUnwindSafe(|| executor.execute(&assignment, &channel)))
                        .unwrap_or(Outcome::Failed {
                            reason: "executor panicked".into(),
                            retryable: false,
                        });
                channel.send(Body::Done(outcome));
            })?;
        self.live.insert(
            id.clone(),
            Live {
                attempt,
                session,
                workdir,
                cancel,
                handle: Some(handle),
                started: Instant::now(),
                stop: None,
            },
        );
        Ok(())
    }

    fn settle(&mut self) -> Result<()> {
        if self.plan.state.is_settled()
            || !self.live.is_empty()
            || !self.waiting.is_empty()
            || !self.plan.all_terminal()
        {
            return Ok(());
        }
        let failures = failure_lines(&self.plan);
        let state = if failures.is_empty() {
            PlanState::Succeeded
        } else {
            PlanState::Failed {
                reason: failures.join(
                    "
",
                ),
            }
        };
        self.commit(Record::Settled { state })
    }

    /// Removes worktrees of finished tasks. Worktrees with uncommitted changes
    /// are kept and reported; branches are never deleted.
    pub fn release_worktrees(&mut self) -> Result<Vec<(TaskId, std::result::Result<(), String>)>> {
        let Workspace::Worktrees { manager, .. } = &self.workspace else {
            return Ok(Vec::new());
        };
        let mut results = Vec::new();
        for task in self.plan.tasks().filter(|t| t.status.is_terminal()) {
            let id = &task.spec.id;
            if !self.workdirs.contains_key(id) {
                continue;
            }
            let key = SessionId::new(id.as_str())?;
            let result = manager.remove(&key, false).map_err(|e| e.to_string());
            results.push((id.clone(), result));
        }
        for (id, result) in &results {
            let text = match result {
                Ok(()) => "worktree removed".to_owned(),
                Err(error) => format!("worktree kept: {error}"),
            };
            if result.is_ok() {
                self.workdirs.remove(id);
            }
            self.decision(Some(id), text)?;
        }
        Ok(results)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        for live in self.live.values() {
            live.cancel.cancel();
        }
    }
}

/// One line per task that did not succeed, naming the task and the real
/// reason, so a failed plan never reports only a count.
fn failure_lines(plan: &Plan) -> Vec<String> {
    let total = plan.tasks().count();
    let mut lines: Vec<String> = plan
        .tasks()
        .filter_map(|task| {
            let why = match &task.status {
                TaskStatus::Succeeded => return None,
                TaskStatus::Failed { reason } => reason.clone(),
                TaskStatus::Skipped { because } => format!("skipped because {because} failed"),
                TaskStatus::Cancelled => "cancelled".to_owned(),
                other => format!("{other:?}"),
            };
            Some(format!("{} ({}): {why}", task.spec.title, task.spec.id))
        })
        .collect();
    if !lines.is_empty() {
        lines.insert(
            0,
            format!("{} of {total} task(s) did not succeed:", lines.len()),
        );
    }
    lines
}

fn tail(text: &str, max: usize) -> String {
    let start = text.len().saturating_sub(max);
    let start = (start..text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(text.len());
    text[start..].trim().to_owned()
}
