# COVALOS Windows installer

The distributable is `dist/Covalos-0.4.0-Windows-x64-Setup.exe` (about 6 MB). Run it normally. It installs for the current Windows user without administrator access.

- Default location: `%LOCALAPPDATA%\Programs\Covalos`.
- Registered in Windows Settings > Apps > Installed apps, with an uninstaller.
- Start menu shortcut; optional desktop shortcut and file/folder context menus.
- Spanish and English wizard. No file extension defaults or user PATH changes.
- Re-running the setup updates the existing installation using a stable application ID.
- Uninstallation removes installed files and registrations, preserving preferences, projects, legacy AEROS profiles and files the installer did not create.
- Windows 10/11 x64; the editor needs an OpenGL-capable graphics driver. Only Windows system DLLs are imported by the packaged binary. Rust, Node.js and Git are not needed to start the editor; developer integrations still need their respective tools.
- This build is **unsigned**. Production Authenticode signing requires a code-signing certificate; no certificate was available for this build.

## Rebuild

Use Windows PowerShell 5.1+, stable Rust and Inno Setup 6.3 or newer. From this checkout:

~~~powershell
.\tools\build-windows-installer.ps1
~~~

To package the already verified release in this workstation:

~~~powershell
.\tools\build-windows-installer.ps1 -SkipBuild -BinaryPath E:\workspace\covalos-build\release\covalos.exe -IsccPath 'C:\Users\Matias\AppData\Local\Programs\Antigravity IDE\resources\app\node_modules\innosetup\bin\ISCC.exe'
~~~

The script checks the executable's COVALOS identity, version and x64 PE architecture, collects third-party dependency/asset license notices, compiles the installer and writes a SHA-256 checksum plus `build-info.json`. Supply `-OutputDir` or `-TargetDir` to override the defaults. `-SkipBuild` still requires Cargo for dependency notices.

## Validation

~~~powershell
node .\tools\verify-windows-installer.cjs --installer .\dist\Covalos-0.4.0-Windows-x64-Setup.exe --binary E:\workspace\covalos-build\release\covalos.exe
~~~

This verifier refuses an existing COVALOS installation/integration. It installs into a temporary directory containing spaces and Unicode, checks the installed binary hash, Windows registration, App Paths, quoted file/folder context commands and the installed Library Studio CLI; then reinstalls and uninstalls. It checks that settings and untracked user files survive and all application registry entries are removed. It uses `/NOICONS` to keep the user's real desktop and Start menu untouched; shortcut definitions were compiled and reviewed, not exercised by this automatic test.

Results: `artifacts/windows-installer/verification.json`; detailed setup logs sit beside the report. Existing COVALOS and AEROS profile folders are not used during these tests.
