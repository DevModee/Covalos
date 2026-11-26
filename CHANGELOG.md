# Changelog

Notable changes per published build. Versions follow the internal build number in
`Cargo.toml` (see [`docs/RELEASE.md`](docs/RELEASE.md)); the public name is "Beta" plus that
number. Builds before 0.8.4 are described only in the git history.

## [0.8.10] - 2026-11-26

### Added

- "Close project" action in the workspace menu to return to the home screen cleanly.

### Changed

- Retired Gemini CLI in favor of Antigravity as the Google provider.
- Redesigned the Terminal panel with softer padding and borderless pill tabs.
- Gave Guided Mode more room for code and notes by removing its title bar.
- Added padded text field styling across Settings and Student Mode, and turned the version label into a quiet badge.

### Fixed

- Student Mode now waits for and picks up already authenticated AI accounts (Claude Code, Codex, Antigravity).

## [0.8.9] - 2026-11-26

### Added

- Student Mode: lessons for Python (seven days), TypeScript and SQL with a tutor that never
  writes the solution, a three-step hint ladder, a smaller bridge exercise after two scores
  under 70, and a short review of earlier topics when a session starts.
- Run & Grade runs real tests on the student's code and scores it from 0 to 100
  (correctness, autonomy, clarity). Branch names, commit messages, project layout and
  English names are checked too, and those checks can be turned off.
- Student progress (streak, lessons, review queue) is saved locally first and synced to the
  COVALOS account when signed in. Exercise code and chat are never uploaded.
- Guided, Agents and Student share one switcher in the top bar (Ctrl+Shift+G, H, L).
- Automatic updates: COVALOS checks GitHub Releases at startup, downloads the installer for
  this system, verifies its SHA-256 and restarts into the new version on Windows.
- Optional "Keep running in the tray" setting, so closing the window keeps the phone bridge
  connected.

### Changed

- Busy indicators use a dot-matrix loader instead of the generic spinner.
- OLED Dark is true black with a cobalt dither at the top.
- COVALOS account settings were redesigned and use the hosted endpoint by default.
- The Guided Mode header, line notes and typo feedback were redesigned.
- Completions show kinds and signatures; ghost text appears before closing brackets.

### Fixed

- Agent chat labels showed double-encoded separators.
- Ctrl+X reached the editor instead of the terminal.
- Closing a terminal tab switched away from the active terminal.
- Realtime join replies without `join_ref` were rejected.

## [0.8.8] - 2026-11-25

### Added

- Release builds for macOS (Apple silicon and Intel, `.dmg`) and Linux x86_64 (`.AppImage`
  and `.deb`) next to the Windows installer, produced by `.github/workflows/release.yml`.
  These builds are new and less tested than Windows.
- CI workflow running `cargo fmt`, Clippy and the test suite on Windows, macOS and Linux.

### Changed

- The Luma Notch shares the top bar fill and joins it with concave corners, so it reads as
  part of the window chrome instead of a floating card.
- The Luma mascot faces the cursor and changes expression with the agent state.
- The bottom gap that appeared on maximized windows next to the Windows taskbar is gone.
- Guided Mode launcher copy describes what Learn and Code only do.

### Fixed

- Learn hover labelled every call as an "imported function" in any file that contained the
  word `import`. It now checks import, `use`, `from` and `require` lines for the identifier.
- The Spanish translation table had duplicated entries; a test now rejects repeated keys.

## [0.8.7]

### Added

- Guided Learning Studio: a launcher that takes a prompt, a repository or a course link and
  builds a step-by-step plan.
- Two Guided styles: **Learn** (short explanation per line, call flow on hover,
  "Explain another way", XP and streak) and **Code only** (the lines to type, without
  explanation cards).
- Learn hover: hovering a code block in the editor highlights it and lists what it calls.

### Changed

- Luma takes its colours from the active theme.

## [0.8.5]

### Added

- `Ctrl+P` also searches Settings (English and Spanish labels and keywords).
- Guided Mode reconciles its steps with the files on disk, so work written by an agent or by
  hand marks steps as done.
- Save prompt when closing files with unsaved changes.

### Changed

- Luma Notch layout and terminal panel polish.

## [0.8.4]

### Added

- Nocturne colour theme and syntax palette.
- Luma Notch at the top of the window, with Home and Chat tabs.
- Guided Mode can open in its own window, for a second monitor.
- Code minimap with an `Alt` peek.
- Short splash on launch.
