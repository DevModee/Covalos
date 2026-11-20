# Covalos releases

## Versioning

Covalos has two numbers on purpose:

| Name | Where | Current | Rule |
| --- | --- | --- | --- |
| **Public release** | `brand::RELEASE`, About window, top bar tag, git tag, release notes | **Alpha 0.1.0** (tag `v0.1.0-alpha`) | What people see. `Alpha 0.x.y` while the product is in alpha; bump `y` for fixes, `x` for new capabilities. Then `Beta`, then `1.0.0`. |
| **Internal build** | `Cargo.toml` `version`, Windows file version (`build.rs`), installer | **0.4.0** | Only ever increases. Installers and update checks compare this number, so it is never reset to match the public name. Bump it with every published build. |

About shows both: `Alpha 0.1.0 · build 0.4.0`.

Tags: `v<public>` with the alpha suffix, e.g. `v0.1.0-alpha`. A tag is created only when a build is actually published; nothing here publishes automatically.

## Building

Release builds go to a target directory outside the repository (a running `covalos.exe` locks its own folder):

```powershell
cargo build --release --target-dir E:\workspace\covalos-integration-build
```

Executables and installers are never committed. `target/` and build folders stay outside git.

## Platforms

| Area | Windows | Linux / macOS (prepared, not yet built for this release) |
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

Dependencies keep their own licenses. A published build must ship their notices (for example generated with `cargo about` or `cargo-license`) next to the executable or in the installer. Not generated yet for Alpha 0.1.0.
