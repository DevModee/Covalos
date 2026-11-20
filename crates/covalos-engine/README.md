# covalos-engine

The autonomous orchestration engine for COVALOS. It builds on `covalos-agents` and has no UI dependency.

## Flow

```
goal ──► Planner ──► PlanSpec ──► resolve (roles, budgets, cycle check, topological order)
                                      │
                                      ▼
            Engine ── schedule ready tasks ──► select agent ──► worker thread ──► Executor
              ▲   journal every record              (role, capabilities,        │
              │                                      probe, models, load,       │ Report / Approval / Done
              └──────────── Channel ◄───────────────  preferences)  ◄───────────┘
```

## Modules

| Module | Responsibility |
| --- | --- |
| `planner` | The `Planner` trait. `TemplatePlanner` is a deterministic, offline decomposition of English or Spanish goals: architecture → schema → backend ∥ frontend → tests → review → release. It refuses goals it cannot recognize. For richer plans, an Architect agent returns JSON for `PlanSpec::from_json`. |
| `plan` | `PlanSpec` validation (unknown roles, missing dependencies, cycles, size limits), plus `Plan` and `TaskStatus`. All changes go through `Plan::apply(&Record)`. |
| `role` | `Capability`, `Autonomy` (mapped to the hub's permission preset and the provider sandbox) and completion `Criterion`. `RoleCatalog` holds the 8 built-in roles and user-defined ones; pre-approving a command that is not `Safe` is rejected. |
| `agent` | `AgentProfile` built from an `Account` and a real binary probe, plus `Preferences` (provider order, pinned roles, exclusions, models). `select` explains why no agent qualifies. |
| `engine` | Scheduling with a parallelism limit, attempts with exponential backoff, budgets for tasks and plans, approvals, sign-off, pause, resume, cancel, crash recovery and worktree release. |
| `executor` | The `Executor` trait, `Assignment` (prompt and context digest, never transcripts), `Channel` (reports, blocking approvals, cancellable sleep) and `CancelToken`. |
| `cli` | `CliExecutor` runs Claude Code or Codex headless through the hub's `LaunchPlan` and stream decoders, in a scrubbed environment, with process-group kill, a wall-clock limit and an output limit. |
| `journal` | Append-only JSON Lines, synced per record. A torn tail is truncated when the journal is reopened, and corruption in the middle is an error. |
| `safety` | Static command classification into `Safe`, `Sensitive` (always needs a person) or `Forbidden` (always refused). |
| `process` | `INHERITED_ENV` allowlist, process groups, `kill_tree` and `run_limited`. |
| `budget` | `Budget`, `Spend` and `Limit`. Cost counts only when the provider reports it (`cost_reported`). |

## Safety model

- **Engine gate (`Engine::decide`):**
  - `Forbidden` commands are denied.
  - `Sensitive` commands are asked every time. Session grants never cover them: publishing, deploying, exposing ports, installing dependencies, credentials, irreversible git or file operations, shell composition and inline scripts.
  - Paths that look like credentials need approval even inside the worktree.
  - Everything else follows the task's `Policy`, which always denies writes outside the workspace.
- **Approvals:** expire as denials, and are denied when a task finishes or the plan is cancelled.
- **Process limits:** a scrubbed environment (host tokens such as cloud keys are not inherited), a killable process group, a wall-clock limit and an output cap. CPU, memory and network isolation are **not** enforced here; the provider CLI's own sandbox (`--permission-mode` / `--sandbox`) governs tool use inside a headless turn.
- **Worktrees:** they are not trusted by default. Dirty worktrees are never removed and branches are never deleted.
- **Shared context:** refuses secret-looking text and forces the author to the reporting session.
- **Accounts:** no account rotation and no fallback to other accounts to stretch quotas. Selection only balances load across accounts the user configured.

## Verification

```sh
cd crates/covalos-engine
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The integration tests (`tests/engine.rs`) use deterministic scripted agents. They cover the Spanish web-app goal end to end with identical replay, the parallelism bound, retries, dependents skipped after a failure, completion criteria, task and plan budgets, approvals (including expiry and re-asking sensitive commands), human sign-off, pause, resume and cancel, crash recovery, sign-off re-ask after recovery, unassignable roles, secret refusal and worktree isolation.

The `cli` tests run `CliExecutor` against a fake provider binary that speaks Claude's `stream-json`. They exercise process start, stdin prompt, flags, usage and cost, environment scrubbing, provider errors and cancellation. They do not contact a real provider.
