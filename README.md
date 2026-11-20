# COVALOS 0.4

**COVALOS** (formerly AEROS Editor) is a native Rust IDE built for speed, ergonomic editing, and multi-agent development. It uses `eframe`/`egui` with OpenGL (`glow`) for native rendering, `syntect` for semantic syntax highlighting, and `rfd` for OS file dialogs. Centralized product identity and provisional brand assets live in [`src/brand.rs`](src/brand.rs) and [`assets/brand/`](assets/brand/PROVENANCE.md).

## Build and run

Install stable Rust and build the Windows Release executable (`covalos.exe`):

```powershell
cargo build --release --target-dir E:\workspace\covalos-build
cargo test --target-dir E:\workspace\covalos-build
cargo clippy --all-targets --target-dir E:\workspace\covalos-build -- -D warnings
```

Run `covalos.exe` directly, optionally passing a workspace folder path as the first argument.

## Configuration and AEROS migration

Settings, conversations, Library Studio state, and agent-bridge files are stored in:

- **Windows:** `%LOCALAPPDATA%\Covalos`
- **macOS:** `~/Library/Application Support/Covalos`
- **Linux:** `$XDG_CONFIG_HOME/covalos` (or `~/.config/covalos`)

Override the profile directory with `COVALOS_CONFIG_DIR` (legacy `AEROS_CONFIG_DIR` is also honored for backwards compatibility).

On first launch using the default OS directory, COVALOS detects any legacy `AerosEditor` profile folder and performs a **one-time, non-destructive migration** into `Covalos`:
- Existing COVALOS files are never overwritten.
- The original `AerosEditor` folder is never modified or deleted.
- Legacy `aeros/*` native Library Studio IDs in favorites and history are automatically mapped to `covalos/*`.

## Key capabilities

- **Nocturne Design System:** Rounded island panels, Lucide interface icons (ISC), Material Icon Theme file icons (MIT), Geist & IBM Plex Mono typography (OFL), independent code syntax schemes, density/font/motion controls, and Spanish/English localization.
- **Dither Field:** Two-pass GLSL background renderer with Bayer 8×8 dithering, cursor trail, and click ripples (pauses cleanly without focus, when minimized, or with Reduce Motion).
- **Clean Code:** Detachable floating editor surfaces sharing the live document buffer with focus mode and animated docking.
- **Library Studio:** Offline-first catalog covering 2,000+ components from official registries, honest framework compatibility, conflict-aware install plans with backups and rollback, and structured agent CLI:
  ```powershell
  covalos.exe --library search <query> [--category C] [--project DIR] [--free] [--open-license] [--compatible] [--limit N]
  covalos.exe --library show <id> [--project DIR]
  covalos.exe --library compat <id> --project DIR
  covalos.exe --library plan <id> --project DIR
  covalos.exe --library installed
  covalos.exe --library sources
  covalos.exe --library sync [source]
  ```
- **Agent Hub & Luma Companion:** Workspace and isolated agent conversations (Codex and Claude Code adapters; Gemini CLI and Antigravity discovery), scoped approval hooks (`--agent-hook`), project memory, and GitHub `SKILL.md` import. Override provider binaries with `COVALOS_CODEX_BIN`, `COVALOS_CLAUDE_BIN`, `COVALOS_GEMINI_BIN`, or `COVALOS_AGY_BIN` (legacy `AEROS_*_BIN` fallbacks remain supported).

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl/Cmd + P` | Quick Access (files, `>` commands, `@` recent projects, `?` Codex) |
| `Ctrl/Cmd + Shift + L` | Toggle Library Studio (configurable in Settings) |
| `Ctrl/Cmd + O` | Open folder |
| `Ctrl/Cmd + Shift + O` | Open file |
| `Ctrl/Cmd + N` | New file |
| `Ctrl/Cmd + S` | Save active file |
| `Ctrl/Cmd + W` | Close active tab |
| `Ctrl/Cmd + F` | Find in active file (`F3` / `Shift + F3` next/prev) |
| `Ctrl/Cmd + Shift + F` | Search workspace |
| `` Ctrl/Cmd + ` `` | Toggle bottom tools / terminal |
