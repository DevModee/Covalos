<p align="center">
  <img src="assets/brand/covalos-app-256.png" alt="COVALOS" width="96" height="96" />
</p>

<h1 align="center">COVALOS</h1>

<p align="center">
  <strong>A native Rust IDE built for speed, craft, guided learning, and multi-agent development.</strong><br/>
  Obsidian island workbench, GPU Dither & ASCII Flow backgrounds, Guided Mode (lyric-style code-along), Agent Hub, Clean Code floating editors, ConPTY terminal, LSP, Git/GitHub, Library Studio, and full session persistence, with zero Electron overhead.
</p>

<p align="center">
  <img alt="Rust Edition 2024" src="https://img.shields.io/badge/Rust-Edition_2024-8B5CF6?style=flat-square&logo=rust&logoColor=white" />
  <img alt="Renderer eframe/egui + OpenGL" src="https://img.shields.io/badge/Renderer-eframe_%2F_egui_+_OpenGL-6366F1?style=flat-square&logoColor=white" />
  <img alt="Design System Obsidian & Nocturne" src="https://img.shields.io/badge/Design_System-Obsidian_%2F_Nocturne-3B82F6?style=flat-square" />
  <img alt="Release v0.4.0 Beta" src="https://img.shields.io/badge/Release-v0.4.0_Beta-110F17?style=flat-square&labelColor=231F2E" />
</p>

---

## Vision

**COVALOS** is a native desktop IDE written in Rust that combines the speed, ergonomics, and visual discipline of **IntelliJ IDEA**, **Zed**, **Linear**, and **Raycast** with the multi-agent workflows of **Cursor**, **Kiro**, and **Google Antigravity**.

The name comes from *covalent*: distinct nuclei sharing a common field to form a stable structure. That is the core model of COVALOS, where the developer and multiple AI agents operate over a shared workspace with explicit supervision, bounded authority, and predictable state.

**Core principles:**
- **True native performance:** Single compiled Rust binary (`covalos.exe`) rendered on OpenGL, with no bundled browser runtime.
- **Minimalist craft:** The *Nocturne* dark design system uses rounded island surfaces, crisp 1 px borders, disciplined typography, and critically damped spring motion.
- **Developer control first:** Agents plan, review, and propose edits; code modifications and package installations require explicit user approval, backed by snapshots and rollback.

---

## Screenshots

> Captured from a Windows Release build (`covalos.exe`) running on OpenGL.

| Home · *Nocturne + Dither Field* | Rust Editor · *Nocturne Syntax* |
| :---: | :---: |
| ![COVALOS Home with Dither Field](assets/screenshots/home-nocturne.png) | ![COVALOS Rust Editor in Nocturne](assets/screenshots/editor-rust.png) |

| Clean Code · *Floating editors sharing the live buffer* | Library Studio · *Offline-first catalog + install review* |
| :---: | :---: |
| ![Clean Code floating editor](assets/screenshots/clean-code-floating.png) | ![Library Studio install review](assets/screenshots/library-studio.png) |

---

## Features

### 1. Native Editor, Terminal, LSP & Daily Workflow
- **Island workbench & Explorer (`Ctrl+B`):** Two-cluster Activity Rail, collapsible Explorer with live file filter, colored monogram project switcher, capsule-pill code tabs, breadcrumbs with file icons, and drag-to-close side/bottom panels.
- **Full session persistence:** Automatically restores your last workspace, open tabs, active file, Agent Hub session/model/effort, and Guided Mode progress across restarts.
- **Reliable file handling:** UTF-8 files up to 1 MiB, BOM and CRLF/LF preservation, external-change detection before overwriting, in-file find (`Ctrl+F`, `F3` / `Shift+F3`), and ignore-aware workspace search (`Ctrl+Shift+F`).
- **LSP & semantic syntax highlighting:** Language Server Protocol support (diagnostics, completion, UTF-16 position mapping) paired with `syntect` grammars, a semantic token classifier, and independent code color schemes (*Obsidian*, *Nocturne*, *Graphite*, *Daylight*, *High Contrast*).
- **Floating Command Palette (`Ctrl+P`):** Category filter pills (`All`, `Files`, `Navigation`, `Git`, `Settings`) for workspace navigation and commands.
- **Git & GitHub integration:** Branch detection, status grouped by state (*staged*, *working tree*, *untracked*, *conflicts*), built-in commit composer (`Stage all` + `Commit`), change filter, searchable commit history, inline diffs, and a **GitHub** PRs/Issues tab powered by `gh` with strict URL validation.
- **Interactive ConPTY Terminal:** Native Windows ConPTY terminal with ANSI rendering, clickable dev-server URL detection, port attribution, and rounded island chrome.
- **Notifications & safe settings import:** Floating Notifications overlay (`All`, `Covalos`, `Agent`, `GitHub`), two-column Settings modal with global `Surface brightness` slider, and a read-only importer for VS Code, Cursor, Windsurf, and Zed settings.

### 2. Guided Mode (*Modo Guiado*)
- **Learn by coding:** Toggle **Guided Mode** in the Session Home composer (or click **Open in Guided Mode** on any AI plan) to launch a floating lyrics-style code window synced live to your editor buffer.
- **Deterministic dependency ordering:** Before a session starts, Covalos inspects local imports (`TS/JS`, `Rust`, `Python`) and orders or splits steps so you never type an `import` for a file that does not exist yet:
  - **Natural (Just-in-Time):** Start in your entry file (e.g. `main.tsx`) up to the line where a dependency is needed, switch to create and write that file first, then return to `main.tsx` to import and use it.
  - **Dependencies first (Bottom-Up):** Write leaf modules and components first, then wire them together in the entry file.
- **Live karaoke sync & persistence:** Completed lines dim above, the active line highlights with a real-time progress bar as you type (*Flexible* or *Strict* matching), optional 1-line `Why?` explanations, and automatic progress saving per workspace.

### 3. Obsidian & Nocturne Design System & Motion
- **Continuous island architecture:** Seamless outer shell framing a darker rounded editor well with hairline 1 px borders, global `Surface brightness` control, and high-contrast WCAG AA typography.
- **Dither Field, Dither Bloom & ASCII Flow:** Two-pass GLSL background renderer (value-noise fbm, Bayer 8x8 ordered dithering, and square pixel grid with pointer trail). Pauses automatically without focus or with *Reduce Motion*. Benchmarked on AMD Ryzen 5 5600G + integrated Radeon Graphics at ~2.1% total CPU (~6% GPU) in steady-state animation and 0.0% CPU/GPU when disabled.
- **Spring motion engine (`src/editor/motion.rs`):** Interruptible critically damped springs (*Snappy*, *Smooth*, *Gentle*), global motion intensity slider, and *Reduce Motion* support.
- **Customization & localization:** 7 themes (*Obsidian*, *COVALOS Nocturne*, *Graphite*, *Midnight*, *Violet*, *Light*, *High Contrast*), configurable font size, compact/comfortable density, live syntax preview, and full **English** / **Spanish** localization.

### 4. Clean Code (Floating Editors)
- Detach any open tab into a floating editor window without copying state: every floating surface operates directly on the shared `Document` buffer, dirty flag, save path, and exit guards.
- Supports multiple simultaneous floating editors, focus mode, and spring-animated docking back into the tab strip.

### 5. Library Studio
- **Local-first component catalog:** Incremental ETag sync across 10 official registries (**2,050+ entries** synced in ~3-4 s), 5 link-only sources, and 7 offline native COVALOS resources with live interactive playgrounds.
- **License compliance:** Permissive (MIT/ISC) sources cache source code locally; non-redistributable sources (Commons Clause / Aceternity) store metadata only and fetch source on demand; commercial libraries show links only.
- **Compatibility & rollback:** Detects project framework and package manager (`npm`, `pnpm`, `yarn`, `bun`), previews exact CLI commands, target files, dependencies, and conflicts before running, backs up manifests and conflicting files, and supports full rollback.
- **Structured Agent CLI (`--library`):** JSON output designed for agent workflows with explicit untrusted-data notices.

### 6. Agent Hub, Luma Companion & Discord Rich Presence
- **Session Home & unified Agent Hub:** Start directly from `"Where should we begin?"` with official provider logos, curated model picker with per-model **Thinking effort** (`Low`, `Medium`, `High`, `Extra High`), `/model` and `/effort` slash commands, and a local **Agent usage** dashboard (input/output tokens, turns, models).
- **Official CLI drivers & one-click installer:** Headless streaming adapters for **Codex** (ChatGPT) and **Claude Code** (with **Gemini CLI** account registration and chat adapter coming soon), plus an allowlisted one-click npm installer for `@anthropic-ai/claude-code`, `@openai/codex`, and `@google/gemini-cli`.
- **Global local rules & skills:** Edit persistent local agent instructions (`agent-rules.md`) directly inside Covalos, toggle **Auto skills**, and import `SKILL.md` files from public GitHub repositories.
- **Hook bridge (`--agent-hook`), worktrees & Luma:** Scoped permission approvals, optional per-session git worktree isolation, and live status reflected by Luma, a procedural vector companion rendered natively in `egui`.
- **Privacy-first Discord Rich Presence:** Built-in official Covalos Discord IPC integration (opt-in, off by default) showing language icon, Covalos badge, session timer, optional project name, and a link to the Covalos Beta page, without sending file paths, filenames, prompts, or credentials.

### 7. Native Packaging & Migration
- **Windows x64 installer & executable identity:** `build.rs` embeds a multi-resolution `.ico` (16-256 px) and Windows PE `VERSIONINFO` (`COVALOS`, `0.4.0.0`) directly into `covalos.exe`, packaged with Inno Setup 6 via `tools/build-windows-installer.ps1`.
- **Non-destructive migration from AEROS:** Automatically detects legacy `AerosEditor` profiles on first launch, migrates settings, conversations, Library Studio state, and agent-bridge data once without overwriting existing COVALOS files or deleting legacy folders.

---

## Tech Stack & Assets

### Core Rust Stack
| Area | Crates / Technology | Role |
| --- | --- | --- |
| Language | **Rust (Edition 2024)** | Native desktop performance, memory safety, thin-LTO release builds |
| UI & Renderer | **`eframe` + `egui` 0.33.3 (`glow`)** | Native OpenGL desktop UI, floating surfaces, custom GLSL shaders |
| Syntax | **`syntect` 5.3 + `egui_extras`** | Sublime grammars + custom semantic token classifier & themes |
| Workspace I/O | **`ignore`**, **`regex`**, **`rfd`**, **`tempfile`** | `.gitignore`-aware indexing, native OS dialogs, atomic file writes |
| Data & Imaging | **`serde`**, **`serde_json`**, **`png`** | Preferences, offline catalog, state persistence, mipmapped RGBA8 textures |
| OS Integration | **`windows-sys` 0.61**, **`libc`** | System locale detection, filesystem APIs, clean subprocess handling |

### Typography, Icons & Third-Party Marks
- **Fonts:** *Geist* (UI) and *IBM Plex Mono* (code), licensed under **SIL OFL** (`assets/fonts/`).
- **UI Icons:** *Lucide 0.460.0* (**ISC**), 71 mipmapped monochrome icon masks (`assets/icons/ui/`).
- **File & Folder Icons:** *Material Icon Theme 5.40.0* (**MIT**), 116 icons (`assets/icons/material/`).
- **Provider Marks:** Documented with full terms and provenance in [`assets/brands/PROVENANCE.md`](assets/brands/PROVENANCE.md).
- **COVALOS Mark (Provisional):** Centralized in [`assets/brand/`](assets/brand/PROVENANCE.md) and regenerated from a single master SVG via [`tools/generate_brand_assets.py`](tools/generate_brand_assets.py).

---

## Windows Installer

Build a standard Windows x64 installer with `.\tools\build-windows-installer.ps1`. It registers COVALOS in Installed apps, adds a Start menu shortcut and offers desktop/context menu integration. See [`docs/WINDOWS_INSTALLER.md`](docs/WINDOWS_INSTALLER.md) for the installer location, rebuild command, validation and signing status.

---

## Building & Running

### Prerequisites
- Stable Rust toolchain (`rustup` / `cargo`)
- OpenGL 3.3+ capable GPU (tested on integrated AMD Radeon Graphics)
- Optional: `git`, `node` / `npm` (for Library Studio React component installs), and `codex` / `claude` CLIs for Agent Hub.

### Build, Lint & Test

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release

# Launch COVALOS directly into a workspace
.\target\release\covalos.exe C:\path\to\your\project
```

### Library Studio CLI (JSON)

```powershell
covalos.exe --library search "animated button" --project C:\path\to\project --compatible --limit 10
covalos.exe --library show shadcn/button --project C:\path\to\project
covalos.exe --library compat shadcn/button --project C:\path\to\project
covalos.exe --library plan shadcn/button --project C:\path\to\project
covalos.exe --library sources
covalos.exe --library sync [source]
covalos.exe --library installed
```

### Configuration & Environment Variables

| Path / Variable | Purpose |
| --- | --- |
| `%LOCALAPPDATA%\Covalos` | Default profile location on Windows (`~/Library/Application Support/Covalos` on macOS, `~/.config/covalos` on Linux) |
| `COVALOS_CONFIG_DIR` | Override configuration directory (falls back to `AEROS_CONFIG_DIR` for compatibility) |
| `COVALOS_CODEX_BIN` / `COVALOS_CLAUDE_BIN` | Optional absolute path override for agent CLIs (`COVALOS_GEMINI_BIN` and `COVALOS_AGY_BIN` supported as well) |

---

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl / Cmd + P` | Quick Access (files, `>` commands, `@` recent projects, `?` Codex) |
| `Ctrl / Cmd + Shift + L` | Toggle Library Studio *(configurable in Settings)* |
| `Ctrl / Cmd + O` | Open folder |
| `Ctrl / Cmd + Shift + O` | Open file |
| `Ctrl / Cmd + N` | New file |
| `Ctrl / Cmd + S` | Save active file |
| `Ctrl / Cmd + W` | Close active tab |
| `Ctrl / Cmd + F` | Find in active file (`F3` / `Shift + F3` next / previous) |
| `Ctrl / Cmd + Shift + F` | Search workspace |
| `` Ctrl / Cmd + ` `` | Toggle bottom tools & terminal panel |

---

## Roadmap

- [ ] **Final brand isotype sign-off**.
- [x] **Windows x64 installer** with Windows registration, shortcuts, optional context menus and non-destructive uninstallation.
- [ ] **Full multi-agent orchestration:** Headless adapters for **Gemini CLI** and **Antigravity (`agy`)**, shared cross-agent memory, and automatic Library Studio CLI guidance inside Agent Hub.
- [ ] **Isolated WebView2 previews** for React components in Library Studio.
- [ ] **Advanced IDE capabilities:** LSP (completion, diagnostics, formatting, go-to-definition), DAP debugging, persistent PTY terminal, and extension support.
- [ ] **Instant minimize pause on borderless Windows frames** using native `IsIconic` state detection.

---

## Documentation

- [`docs/DESIGN_SYSTEM.md`](docs/DESIGN_SYSTEM.md) - Nocturne design system specification, tokens, and GLSL shaders.
- [`docs/WINDOWS_INSTALLER.md`](docs/WINDOWS_INSTALLER.md) - Windows x64 installer build, verification, and signing notes.
- [`assets/brand/PROVENANCE.md`](assets/brand/PROVENANCE.md) - Provisional brand assets and replacement workflow.
- [`assets/brands/PROVENANCE.md`](assets/brands/PROVENANCE.md) - Third-party provider mark licenses and terms.
