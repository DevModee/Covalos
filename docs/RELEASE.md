# Covalos releases

## Versioning

Covalos has two numbers on purpose:

| Name | Where | Current | Rule |
| --- | --- | --- | --- |
| **Public release** | `brand::RELEASE`, About window, top bar tag, git tag, release notes | **Beta 0.8.8** | What people see. `Alpha 0.x.y` while the product is in alpha; bump `y` for fixes, `x` for new capabilities. Then `Beta`, then `1.0.0`. |
| **Internal build** | `Cargo.toml` `version`, Windows file version (`build.rs`), installer | **0.8.8** | Only ever increases. Installers and update checks compare this number, so it is never reset to match the public name. Bump it with every published build. |

About shows both: `Beta 0.8.8 · build 0.8.8`. A test in `src/brand.rs` fails when `brand::RELEASE` does not end with the Cargo version, so bump both together.

Tags are `v<Cargo version>`, for example `v0.8.8`. Pushing a tag runs `.github/workflows/release.yml`, which:

1. checks that the tag matches `Cargo.toml`;
2. builds the Windows installer (`tools/build-windows-installer.ps1`, GNU toolchain, Inno Setup), the macOS disk images (Apple silicon and Intel) and the Linux AppImage and `.deb`;
3. publishes them, each with a `.sha256`, to the GitHub release for the tag, with notes taken from the matching section of `CHANGELOG.md`.

Asset names do not contain the version (`Covalos-Windows-x64-Setup.exe`, `Covalos-macOS-arm64.dmg`, `Covalos-macOS-x64.dmg`, `Covalos-Linux-x86_64.AppImage`, `Covalos-Linux-amd64.deb`), so `releases/latest/download/<name>` always points at the newest build. The release is marked as latest; a prerelease would not be. The workflow can be re-run for an existing tag from the Actions tab (`workflow_dispatch`, input `tag`), which replaces the assets.

Release checklist: bump `Cargo.toml` and `brand::RELEASE`, add a `CHANGELOG.md` section, run `cargo test`, commit, then `git tag v<version>` and `git push origin v<version>`.

## Building

Release builds go to a target directory outside the repository (a running `covalos.exe` locks its own folder):

```powershell
cargo build --release --target-dir E:\workspace\covalos-integration-build
```

Executables and installers are never committed. `target/` and build folders stay outside git.

## Platforms

| Area | Windows | Linux / macOS (built by the release workflow since 0.8.8, less tested) |
| --- | --- | --- |
| Window, editor, agents | eframe/glow | same code |
| Terminal | ConPTY through `portable-pty` | Unix PTY through `portable-pty`, `$SHELL` |
| Web preview | WebView2 (`wry`, Windows-only dependency) | "Open in the browser" only, until a WebKitGTK/WKWebView build is set up |
| Ports manager | `GetExtendedTcpTable` + Toolhelp | returns no ports yet (`#[cfg(not(windows))]` stubs) |
| Credential store | Windows Credential Manager | memory only (API keys are not kept between runs) until Keychain / Secret Service support exists |
| Process cleanup | Job objects + `taskkill /T` | process groups + `kill -KILL -pgid` |
| App icon and file version | `build.rs` resource | not embedded |

Before the first Linux/macOS build: implement listeners/process tables for Ports, decide the preview backend, and test the PTY on both systems.

## Third-party licenses

Dependencies keep their own licenses. A published build must ship their notices (for example generated with `cargo about` or `cargo-license`) next to the executable or in the installer. The installer build collects license files and generates third-party notices.
