# AEROS Editor

A native Rust code editor for local projects with a Codex CLI panel. AEROS is the working name.

## Run locally

Install stable Rust and build with `cargo run --release --target-dir target`. To open a folder directly, run `cargo run --release --target-dir target -- /path/to/project`. The executable is `target/release/aeros-editor` (`aeros-editor.exe` on Windows).

The desktop UI targets Windows, macOS, and Linux. Linux requires a desktop session with OpenGL and an XDG file chooser portal. Android is a future target; the desktop file picker and CLI integration need mobile counterparts.

## Edit and search

Open a folder, choose a file from the Explorer, and save with Ctrl/Cmd+S. Ctrl/Cmd+F finds text in the active file, with F3 and Shift+F3 to cycle matches. Ctrl/Cmd+Shift+F searches UTF-8 files across the workspace. Workspace search uses saved files, respects ignore rules, and shows the first 200 matching lines.

The editor opens UTF-8 files up to 1 MiB, preserves a UTF-8 BOM and consistent CRLF endings, and checks for changes on disk before replacing a saved file.

## Use Codex

Install Codex CLI and sign in with `codex login`. The editor uses the CLI binary on PATH or the executable specified by `AEROS_CODEX_BIN`, then reuses its configured account and model. Open a workspace, enter a prompt in the Codex panel, and select Send. Follow-up prompts continue the current Codex session until you select New chat or switch workspaces.

Codex runs with read-only workspace access. The prompt includes the active file path when a file is open. Enable **Include active file** to send up to 24 KB of the current buffer, including unsaved edits, with that message. Direct filesystem inspection depends on the local CLI sandbox.

## Verify

Run `cargo test --locked` for local tests. To exercise the live integration, run `cargo test --locked codex_answers_and_resumes -- --ignored` with an authenticated CLI. The live test makes two Codex requests.
