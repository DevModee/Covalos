# Contributing to COVALOS

Thanks for helping. This file covers how to build the editor, run the checks, and send a
change. Small fixes can go straight to a pull request; for larger features, open an issue
first so the design can be agreed before you write the code.

## Prerequisites

All platforms:

- Rust stable, installed with [rustup](https://rustup.rs). The crate uses edition 2024, so
  Rust 1.85 or newer. `rustfmt` and `clippy` components (`rustup component add rustfmt clippy`).
- A GPU and driver with OpenGL 3.3.
- Optional, for features that call out to other tools: `git`, `node`/`npm` (Library Studio
  installs), `gh` (GitHub tab), and the agent CLIs you want to use (`claude`, `codex`,
  `gemini`).

### Windows

The project builds with the **GNU** toolchain, because the Windows build ships
`WebView2Loader.dll` from `webview2-com-sys` (see `build.rs`):

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
```

No separate MinGW install is needed; the toolchain ships its own linker. To build the
installer you also need [Inno Setup 6.3+](https://jrsoftware.org/isdl.php).

### macOS

Xcode Command Line Tools (`xcode-select --install`). Both `aarch64-apple-darwin` and
`x86_64-apple-darwin` build on an Apple silicon Mac.

### Linux

Debian/Ubuntu packages for windowing, GTK file dialogs and TLS:

```bash
sudo apt-get install -y pkg-config libssl-dev libgtk-3-dev libxkbcommon-dev libwayland-dev \
  libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev
```

On Fedora the equivalents are `openssl-devel gtk3-devel libxkbcommon-devel wayland-devel
libX11-devel libXcursor-devel libXrandr-devel libXi-devel mesa-libGL-devel`.

## Build and run

```bash
cargo run -- path/to/a/project      # debug build, opens that folder
cargo build --release               # optimized binary in target/release/
```

A running `covalos.exe` locks its own file on Windows. If you test a release build while
using another COVALOS window, build into a separate target directory
(`cargo build --release --target-dir ../covalos-build`).

To try a change without touching your real settings, point the profile somewhere else:

```powershell
$env:COVALOS_CONFIG_DIR = "$env:TEMP\covalos-dev"; cargo run
```

```bash
COVALOS_CONFIG_DIR=/tmp/covalos-dev cargo run
```

## Checks

Run these before opening a pull request. CI runs the same commands on Windows, macOS and
Linux.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Tests that open native windows, call real agent CLIs (and spend quota) or need network
services are marked `#[ignore]` with the reason. Run them on purpose with
`cargo test -- --ignored <name>` when your change touches that area.

Some tests guard invariants that are easy to break by hand, for example that every string
routed through `tr()` has a Spanish translation, that the translation table has no duplicate
keys, and that `brand::RELEASE` matches the Cargo version. If one fails, fix the cause
rather than the test.

### Packaging locally

- Windows installer: `.\tools\build-windows-installer.ps1` (writes to `dist/`). See
  [`docs/WINDOWS_INSTALLER.md`](docs/WINDOWS_INSTALLER.md).
- macOS and Linux packages are built by `.github/workflows/release.yml`; the steps there can
  be run by hand. The `.deb` uses [`cargo-deb`](https://github.com/kornelski/cargo-deb) after
  `tools/third-party-notices.sh x86_64-unknown-linux-gnu build/notices`.

## Project layout

| Path | What lives there |
| --- | --- |
| `src/main.rs`, `src/editor.rs` | Entry point, app state and the frame loop |
| `src/editor/` | UI modules: shell, panels, themes, Guided Mode, Luma Notch, Agent Hub, terminal, LSP, Library Studio |
| `src/luma.rs` | Luma's decisions (effort rating, staffing); the runtime carries them out |
| `src/agents_runtime.rs`, `src/agent_provider.rs` | Agent sessions and provider CLIs |
| `crates/covalos-agents` | Provider-neutral agent types: accounts, permissions, sessions, worktrees |
| `crates/covalos-engine` | Autonomous engine: planner, executor, budgets, safety rules |
| `assets/` | Fonts, icons, brand marks, screenshots |
| `packaging/`, `tools/` | Installer definitions and build scripts |
| `docs/` | Design system, release process, installer and account notes |

## Code style

- `rustfmt` output is the formatting standard; do not hand-format around it.
- Keep changes focused. A bug fix does not need surrounding refactors.
- Comments explain why something is done or what a non-obvious piece of drawing code
  produces. Do not restate the code, and do not describe it by comparison to other products.
- Interface strings go through `tr("…")` (or `trf` for templates) and need an entry in
  `src/editor/i18n.rs`. Use plain, specific wording: say what a button does.
- Platform-specific code sits behind `#[cfg(windows)]` / `#[cfg(not(windows))]` and must
  keep compiling on the other platforms (CI checks all three).

## Branches and commits

- Branch from `main`. Name branches by type: `feat/…`, `fix/…`, `ui/…`, `docs/…`, `ci/…`.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org):
  `type(scope): summary`, for example `fix(guided): keep step order when a file is renamed`.
  Common types: `feat`, `fix`, `refactor`, `style`, `docs`, `test`, `ci`, `chore`.
- Write the summary in the imperative, under about 72 characters. Use the body to explain
  why the change is needed and anything a reviewer should know.
- One logical change per commit. Formatting-only changes go in their own commit.

## Pull requests

- Describe what changed and why, how you tested it (commands, platforms, screenshots for UI
  changes), and any follow-up work you know about.
- Link the issue it resolves (`Fixes #123`).
- Make sure the checks above pass. UI changes should be tried in the running app, not only
  in tests.
- Keep the pull request small enough to review in one sitting. Split large work into a
  series.

## Reporting bugs

Open an issue with:

- COVALOS version (About window, for example `Beta 0.8.8 · build 0.8.8`) and your OS.
- What you did, what you expected and what happened instead.
- Steps to reproduce, ideally with a small project.
- For agent problems: which CLI and version (`claude --version`, `codex --version`) and the
  error Luma or the agent panel showed.

Do not paste API keys, tokens or private code into issues. For a security problem, contact
the maintainer privately through GitHub instead of opening a public issue.

## Proposing features

Open an issue that describes the problem you want solved before the solution you have in
mind, who it is for, and what you considered instead. Mockups or screenshots help for UI.
