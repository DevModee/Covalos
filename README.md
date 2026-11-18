# AEROS Editor 0.3

AEROS Editor is a native Rust code editor for local projects. AEROS is the working name. The application uses eframe and egui for the desktop interface, syntect for highlighting, and native file dialogs through rfd. Branding and interface colors live in [the theme module](src/editor/theme.rs).

V3 introduces the Orbital visual system with integrated window chrome, Geist and IBM Plex Mono, a unified command palette, compact controls, and refined editor and agent surfaces. See [DESIGN.md](DESIGN.md) for the three proposals, actual before/after captures, validation, and performance.

## Run the Windows build

Open `target/v3/release/aeros-editor.exe`. You can pass a folder path to open it directly. The original V1 executable is at `target/release/aeros-editor.exe`.

For a fresh build, install stable Rust and the platform's native build prerequisites. Build the release and run the tests with these commands.

```sh
cargo build --release --locked --target-dir target/v3
cargo test --locked --target-dir target/v3
```

The application targets Windows, Linux x64 and macOS Apple Silicon. On Linux it needs a desktop session with OpenGL and an XDG file chooser portal.

## Edit and navigate

- The Explorer opens folders and files. Right-click a row to create a file or folder, rename or move an item, or delete a file or empty folder after confirmation. Moves refuse existing destinations.
- Open an existing folder, create a project folder, or open an individual file from the welcome view. The editor remembers recent projects and the sizes of the Explorer, chat, and bottom panels.
- Use the tabs and breadcrumbs to identify the active file. **Settings** adjusts the editor font size and adds higher-contrast control borders.
- The editor accepts UTF-8 files up to 1 MiB, preserves BOM and consistent CRLF endings, and checks for on-disk changes before replacing a saved file. Workspace search uses saved files and shows up to 200 matching lines.
- The Git panel shows the branch and file status, unstaged diff, and staged diff using the installed Git CLI.
- The **Terminal** button opens the resizeable bottom panel with the workspace command runner, editor output, and agent activity. Each command starts a fresh shell at the project root. The runner supports output and cancellation. Interactive programs and a persistent shell require a future PTY integration.

| Shortcut | Action |
| --- | --- |
| Ctrl/Cmd + P | Quick access |
| Ctrl/Cmd + O | Open a folder |
| Ctrl/Cmd + Shift + O | Open a file |
| Ctrl/Cmd + S | Save the active file |
| Ctrl/Cmd + W | Close the active tab |
| Ctrl/Cmd + F | Find in the active file |
| F3 / Shift + F3 | Next / previous match |
| Ctrl/Cmd + Shift + F | Search the workspace |

In Quick Access, type a path to find a workspace file, `>` for an editor command, `@` for a recent project, or `?` to ask Codex in the open workspace. File search respects ignore rules and indexes up to 12,000 paths each time the palette opens.

## Use Codex and Agent Hub

Install Codex CLI and sign in with `codex login`. AEROS uses the existing CLI session. It discovers supported executables on PATH and in common local locations. Set `AEROS_CODEX_BIN` or `AEROS_CLAUDE_BIN` to an absolute executable path to override discovery.

- **Workspace conversation** uses Codex in read-only mode and keeps its session across messages. Enable **Include active file** to send up to 24 KB from the workspace buffer, including unsaved changes. Selecting a file outside the workspace excludes it from attachment.
- **General conversation** starts in a temporary directory. Codex disables its shell and plugins and skips user config in this mode. Claude Code runs with its tool list empty, hooks disabled and MCP limited to explicit configuration. The provider handles its own sign-in.
- **Ask** runs a read-only plan and review. The default reviewer is Codex. The optional Claude reviewer receives the task and a bounded copy of the plan after you select it.
- **Assist** previews one contiguous replacement in an active workspace file under 24 KB. Approval applies the change to the editor buffer only if the buffer still matches the original. Save it explicitly to persist it.
- **Auto** is opt-in through Settings. It can apply an eligible replacement to an unchanged active buffer. Full-buffer replacements and removals still go through the approval view. Disk persistence uses the editor's normal Save action.
- **Project memory** stores manual notes and recent task outcomes locally. Sending saved notes requires selecting the option in Project memory. The application's chat transcript is kept in memory while the provider manages its own conversation data.
- **Skills** imports a public GitHub `…/blob/<ref>/…/SKILL.md` as text through curl. The user reviews and enables the skill, then selects it for tasks. The editor imports the named Markdown file only and records its origin and requested ref. Use a commit hash as the ref if you need stable provenance.

The providers panel distinguishes CLI detection from session status. Gemini CLI and Antigravity have discovery entries. Their runtime adapters are pending.

Settings are stored in `AerosEditor/editor.json` under the OS config location. Set `AEROS_CONFIG_DIR` to use a different folder.

## V1 and V2

| Area | V1 | V2 |
| --- | --- | --- |
| Workspace | Open folders and files, tabs, search, save | Welcome and recent projects, Quick Access, breadcrumbs, Explorer operations |
| Appearance | Fixed theme and basic layout | Orbital tokens, custom icons, font and contrast controls, panel persistence |
| Development tools | File editing | Git status and diffs, command runner, output and agent activity |
| AI | Codex conversation and active file attachment | Agent Hub, general and workspace conversations, a Claude adapter, planning, review, and supervised edits |
| Project context | Current session | Local notes, recent task outcomes, and reviewed text skills |

## Verification

The V3 validation includes 12 local regression tests, the authenticated two-turn Codex test, strict Clippy, Windows Release, cross-target checks for Linux and macOS, and 72 native captures covering both target resolutions at 100%, 125%, and 150% egui scale. The capture fixtures and measured performance are documented in [DESIGN.md](DESIGN.md).

## Remaining work and integration decisions

- Full LSP completion, diagnostics, formatting, and navigation remain separate from syntax highlighting. VS Code language servers and debug adapters are the interoperable entry points. Static snippets, themes, and grammars can be imported independently of extensions that depend on the VS Code extension host and `vscode.*` runtime API. [VS Code extension capabilities](https://code.visualstudio.com/api/extension-capabilities/overview)
- Browser integration remains pending. Obscura v0.2.7 publishes a Windows x64 package with runtime DLLs. It uses V8 and CDP and is licensed Apache 2.0. Its own security model calls for OS-level isolation for adversarial pages because the engine executes their JavaScript in-process. Browser work should compare restricted HTTP text extraction, isolated Chromium for broader JS and rendering compatibility, and an isolated Obscura worker. Windows consumption and rendering parity have yet to be measured locally. [Obscura architecture](https://github.com/h4ckf0r0day/obscura/blob/main/docs/README.md) · [Windows packaging](https://github.com/h4ckf0r0day/obscura/releases/tag/v0.2.7) · [Security model](https://github.com/h4ckf0r0day/obscura/security)
- Gemini CLI documents headless JSON streaming and Antigravity has the official `agy` headless interface. Each adapter needs its own permission policy and authenticated verification. [Gemini CLI headless mode](https://geminicli.com/docs/cli/headless/) · [Antigravity CLI headless mode](https://antigravity.google/docs/cli/headless/)
- The current task runner handles a plan followed by review or a single-buffer edit. Multi-file approval, dependency graphs, background task queues, and automatic command execution remain pending.
- The terminal panel lacks PTY support. Split editors, a full Markdown renderer, additional interface themes, recursive deletion, and a VS Code extension host remain outside the current build.

## Autonomous quality checkpoint

This branch restores complete themes, configurable native backgrounds and ASCII motion, the original Luma companion, live agent supervision, scoped approval hooks, and allocation-free cursor position counting. See [improvement log](docs/AUTONOMOUS_IMPROVEMENTS.md), [companion integration](docs/AGENT_COMPANION.md), and [V4/V5 integration guide](docs/INTEGRATION_V4_V5.md).

Windows Release: E:/workspace/aeros-quality-build/release/aeros-editor.exe. The main V3 checkout and its artifacts are preserved.
