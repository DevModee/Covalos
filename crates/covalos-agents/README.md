# covalos-agents

The provider-neutral foundation for the COVALOS V4 multi-agent hub. It has no UI dependency and nothing in the editor depends on it yet.

## Modules

| Module | Responsibility |
| --- | --- |
| `provider` | `ProviderKind` (Claude Code, Codex, Gemini, custom) and `ProviderSpec`: executable, binary override variable, config-dir variable, API-key variable, login flow and headless support. `ProviderRegistry` holds the built-ins and validated custom providers. |
| `account` | `Account` with a `ConnectionMethod`: `SystemCli` (the user's existing CLI sign-in), `OfficialLogin` (isolated config directory signed in through the provider's own flow) or `ApiKey`. `validate` rejects methods a provider cannot support. |
| `credentials` | `Secret` (redacted `Debug`, wiped on drop), `CredentialKey`, the `CredentialStore` trait and an in-memory store. |
| `profile` | `ProfileStore`: versioned `accounts.json` with atomic writes, per-account isolated directories (`0700` on Unix), and cleanup of the secret and directory on removal. It never stores secrets. |
| `launch` | Builds a `LaunchPlan` for `Login`, `Probe` or a headless `Turn`. It injects the config dir and API key through environment variables and strips stray API keys from isolated accounts. It also provides binary discovery and a probe runner with a timeout. |
| `event` | Normalized `AgentEvent`, plus `ClaudeDecoder` (`stream-json`) and `CodexDecoder` (`exec --json`) behind `StreamDecoder`. |
| `session` | `Session` state machine (`Created → Running ⇄ AwaitingApproval → Idle`, plus `Failed`, `Cancelled` and `Closed`) and `SessionRegistry`. Usage, model and provider session id are tracked per session. |
| `permission` | `Policy` with the `ReadOnly`, `Supervised` and `Worktree` presets and session grants, and `ApprovalQueue` with tickets, expiry and per-session cancellation. Writes that escape the worktree (`..`, absolute paths, symlinks) are always denied. |
| `context` | `SharedContext`: a bounded, append-only board with pinned entries, cursor reads and a budgeted prompt digest that excludes the reader's own entries. |
| `worktree` | `WorktreeManager`: one `covalos/agent/<session>` branch and directory per session through the git CLI. Dirty worktrees are kept unless forced, and branches are never deleted. |
| `models` | `ModelCatalog` per account. Models are declared by the user or observed in real runs, and availability is `Unknown` until proven. `QuotaSnapshot` exists only through `QuotaSnapshot::official(origin, …)`. |
| `plan` | `TaskGraph` with dependency-gated readiness, assignment to sessions, and cascading skips on failure. This is the substrate for a future autonomous orchestrator. |

## Ground rules

- Sign-in always goes through the provider's own CLI. COVALOS only chooses the directory the CLI writes to and never reads its tokens.
- Only documented environment variables are encoded: `CLAUDE_CONFIG_DIR`, `ANTHROPIC_API_KEY`, `CODEX_HOME` and `GEMINI_API_KEY`. Capabilities that could not be verified stay `None` or `Unsupported`:
  - Codex has no API-key variable.
  - Gemini has no isolated login and no headless decoder.
- Quotas and model availability are never estimated.
- Turn arguments mirror the flags the editor already uses in `src/assistant/providers.rs`. Claude's `--permission-mode plan|acceptEdits` was checked against `claude --help` (2.1.329).

## Verification

```sh
cd crates/covalos-agents
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`launch::tests::probes_installed_claude_code` runs the real `claude --version` when the binary is on `PATH` and skips otherwise. The worktree tests need `git`.
