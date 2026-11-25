<p align="center">
  <img src="assets/brand/covalos-app-256.png" alt="COVALOS" width="96" height="96" />
</p>

<h1 align="center">COVALOS</h1>

<p align="center">
  Native desktop code editor written in Rust, with a built-in team of coding agents
  (Claude Code, Codex, Gemini) that run on your own CLIs and accounts.
</p>

<p align="center">
  <img alt="Rust Edition 2024" src="https://img.shields.io/badge/Rust-Edition_2024-8B5CF6?style=flat-square&logo=rust&logoColor=white" />
  <img alt="eframe/egui + OpenGL" src="https://img.shields.io/badge/UI-eframe_%2F_egui_+_OpenGL-6366F1?style=flat-square" />
  <img alt="Beta 0.8.8" src="https://img.shields.io/badge/Release-Beta_0.8.8-110F17?style=flat-square&labelColor=231F2E" />
</p>

## Overview

COVALOS is a single Rust binary drawn with `eframe`/`egui` on OpenGL. There is no bundled
browser runtime, and background shaders stop when the window loses focus.

It covers the daily editing workflow (files, search, terminal, Git, LSP) and adds two things
most editors do not have:

- **Luma**, a lead agent that takes an order, rates how hard it is, and staffs it with one
  agent or a planned team on the CLIs you already use. Writes, commands and installs stop for
  your approval.
- **Guided Mode**, which turns a plan into steps you type yourself, with explanations per
  line for people learning to code.

The project is in beta. Windows is the primary platform. macOS and Linux builds are produced
by the release workflow since 0.8.8 and get less testing; see the platform table in
[`docs/RELEASE.md`](docs/RELEASE.md).

## Screenshots

| Home (Nocturne) | Rust editor |
| :---: | :---: |
| ![Home view in the Nocturne theme](assets/screenshots/home-nocturne.png) | ![Editing a Rust file](assets/screenshots/editor-rust.png) |

| Floating editors | Library Studio |
| :---: | :---: |
| ![Editor tabs detached into floating windows](assets/screenshots/clean-code-floating.png) | ![Library Studio catalog](assets/screenshots/library-studio.png) |

## Install

Download the build for your system from the
[latest release](https://github.com/DevModee/Covalos/releases/latest):

| System | File |
| --- | --- |
| Windows 10/11 x64 | `Covalos-Windows-x64-Setup.exe` (per-user install, no admin rights) |
| macOS, Apple silicon | `Covalos-macOS-arm64.dmg` |
| macOS, Intel | `Covalos-macOS-x64.dmg` |
| Linux x86_64 | `Covalos-Linux-x86_64.AppImage` or `Covalos-Linux-amd64.deb` |

Every file has a `.sha256` checksum next to it. Builds are not code-signed yet, so Windows
SmartScreen and macOS Gatekeeper ask for confirmation on first launch (on macOS, right-click
the app and choose Open).

To build from source, see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Features

### Interface

- Island layout: side panels, editor and agent panel sit on separate rounded surfaces over
  the window background. Themes include **Nocturne** (default dark: plum-charcoal surfaces,
  ivory text, lilac accent) and a matte light theme for screens in direct light.
- Optional animated GLSL backgrounds (Dither Field, Dither Bloom, ASCII Flow). They pause when
  the window is unfocused or Reduce Motion is on.
- English and Spanish interface. Font size, density and UI scale are configurable.

### Luma

Luma lives in the Notch at the top of the window.

- **Home** tab: Luma, recent workspace actions and shortcuts to Guided Mode, the terminal and
  search.
- **Chat** tab: the order box, Luma's log of what it launched, the team checklist with live
  status, and approval requests (Allow / Deny).
- **Effort ceiling** (Low, Medium, High): Luma rates each order from its wording and never
  staffs it above the ceiling you set. The ceiling decides the model, the reasoning effort
  and whether one agent or a planned team does the work.
- Agents run through the official CLIs: Claude Code, Codex, Gemini CLI, plus Antigravity and
  OpenCode. COVALOS does not proxy your account; it starts the CLI you installed and signed
  in to.
- When a task fails, Luma shows the CLI's actual error instead of a generic message.

### Agents and Agent Hub

- Agent panel with model and effort selection (`/model`, `/effort`), conversations grouped
  by project, pin, archive and delete.
- Optional git worktree per session. Unchanged worktrees are removed when the session closes.
- Scoped approvals through the `--agent-hook` bridge: allow once, for the session, or deny.
- Local agent rules (`agent-rules.md`) and `SKILL.md` import from public GitHub repositories.
- Agent Hub: provider accounts, live sessions with their approvals, and the autonomous
  engine.

### Guided Mode and Learn mode

Guided Mode turns a prompt, a repository or a course link into a step-by-step code-along
synced to the editor buffer: the line you are typing is highlighted and the step advances as
the buffer matches.

- **Learn** and **Code only** are separate styles. Learn adds a short explanation per line,
  "Explain another way" (asks the agent for a simpler explanation of the current block), XP
  and a streak counter. Code only shows the lines to type without explanation cards.
- **Learn hover**: while a session is active, hovering a code block in the editor highlights
  it and lists what it declares and which functions it calls (local, imported or method
  calls).
- Local imports (TS/JS, Rust, Python) are inspected before starting, so steps never reference
  a file that does not exist yet. Two orderings: just-in-time (start in the entry file and
  branch out when a dependency is needed) or dependencies first.
- Steps are reconciled with the files on disk, so code written by an agent or in another
  editor marks steps as done.
- **Pop out** moves Guided Mode into its own OS window, for a second monitor. Progress is
  saved per workspace.

### Editor

- Explorer with live filter, project switcher, tabs, breadcrumbs, resizable panels and session
  restore (workspace, tabs, active file, agent session, Guided progress).
- UTF-8 files up to 1 MiB with BOM and line-ending preservation, and external change
  detection before overwrite.
- `Ctrl+P` searches files, commands and Settings options (English and Spanish labels) from one
  box.
- Find in file (`Ctrl+F`, `F3`/`Shift+F3`) and `.gitignore`-aware workspace search
  (`Ctrl+Shift+F`).
- LSP client (diagnostics, completion) on top of `syntect` highlighting.
- Code minimap on the right edge with click and drag scrubbing. When it is off, hold `Alt` to
  peek.
- Integrated terminal: ConPTY on Windows, a Unix PTY with `$SHELL` elsewhere. Dev server URLs
  in the output are detected.
- Git: status, stage, commit, history, inline diffs, and a GitHub PR/issue tab using `gh`.
- Floating editors: any tab can be detached into a floating window that shares the same
  buffer, dirty state and save path.

### Library Studio

- Local component catalog synced with ETags from 10 registries, plus link-only sources.
- Respects source licenses: permissive sources are cached, non-redistributable ones keep only
  metadata, commercial ones are link-only.
- Shows the exact commands and affected files before installing, backs up touched files and
  supports rollback. Also available as a JSON CLI (`--library`) for scripts and agents.

### Account and sync (optional)

Signing in to a COVALOS account enables Cloud Config Sync: theme, fonts, density, pinned
chats, `agent-rules.md` and skills follow you between computers. Workspace paths, recent
folders, notes, open tabs, provider accounts and API keys never leave the machine, and text
that looks like a secret is dropped before upload. The editor works fully without an account.

## Privacy

- Files, prompts and agent output stay on your machine. Agents talk to their providers
  through the CLIs you installed, under your own accounts.
- API keys go to the Windows Credential Manager. On macOS and Linux they are kept in memory
  only until Keychain / Secret Service support lands.
- Discord Rich Presence is off by default; when on, it never sends paths, file names or
  prompts.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+P` | Search files, commands and settings |
| `Ctrl+O` / `Ctrl+Shift+O` | Open folder / open file |
| `Ctrl+N` / `Ctrl+S` / `Ctrl+W` | New file / save / close tab |
| `Ctrl+F` / `Ctrl+Shift+F` | Find in file / search workspace |
| `Ctrl+B` | Toggle Explorer |
| `` Ctrl+` `` | Toggle terminal |
| `Ctrl+Shift+A` | Toggle agent panel |
| `Ctrl+Shift+L` | Library Studio (configurable) |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Zoom in / out / reset |
| `Alt` (hold) | Peek the minimap |
| `Alt+Left` / `Alt+Right` | Back / forward |
| `Esc` | Close the topmost overlay |

On macOS use `Cmd` instead of `Ctrl`.

## Configuration

| Path / variable | Purpose |
| --- | --- |
| `%LOCALAPPDATA%\Covalos` | Profile directory on Windows (`~/Library/Application Support/Covalos` on macOS, `~/.config/covalos` on Linux) |
| `COVALOS_CONFIG_DIR` | Overrides the profile directory (`AEROS_CONFIG_DIR` is still read) |
| `COVALOS_CODEX_BIN`, `COVALOS_CLAUDE_BIN`, `COVALOS_GEMINI_BIN`, `COVALOS_AGY_BIN` | Absolute paths to the agent CLIs |

## Roadmap

- Code signing and notarization for Windows and macOS.
- Ports manager, credential store and web preview on macOS and Linux.
- DAP debugging, formatting and go-to-definition through LSP.
- Final brand mark.

## Documentation

- [`CONTRIBUTING.md`](CONTRIBUTING.md): building, testing, branches, commits and pull requests.
- [`CHANGELOG.md`](CHANGELOG.md): changes per release.
- [`docs/RELEASE.md`](docs/RELEASE.md): versioning, the release workflow and platform status.
- [`docs/WINDOWS_INSTALLER.md`](docs/WINDOWS_INSTALLER.md): installer build, verification and signing.
- [`docs/DESIGN_SYSTEM.md`](docs/DESIGN_SYSTEM.md): design tokens and shaders.
- [`docs/CLOUD_ACCOUNT.md`](docs/CLOUD_ACCOUNT.md): account server setup.

## Third-party assets

- Fonts: Geist and IBM Plex Mono (SIL OFL), in `assets/fonts/`.
- UI icons: Lucide 0.460.0 (ISC), in `assets/icons/ui/`.
- File icons: Material Icon Theme 5.40.0 (MIT), in `assets/icons/material/`.
- Provider marks: see [`assets/brands/PROVENANCE.md`](assets/brands/PROVENANCE.md).
- COVALOS mark: see [`assets/brand/PROVENANCE.md`](assets/brand/PROVENANCE.md), generated by
  [`tools/generate_brand_assets.py`](tools/generate_brand_assets.py).

Release builds bundle the license notices of every Rust dependency (`THIRD_PARTY_NOTICES.txt`).

## License

The repository does not include a license file yet. Until one is added, the code is not
licensed for reuse; contributions are welcome under the terms described in
[`CONTRIBUTING.md`](CONTRIBUTING.md).
