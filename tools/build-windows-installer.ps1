#requires -Version 5.1
[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$TargetDir,
    [string]$OutputDir,
    [string]$IsccPath,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $TargetDir) { $TargetDir = Join-Path $repo 'target\installer' }
if (-not $OutputDir) { $OutputDir = Join-Path $repo 'dist' }
$TargetDir = [IO.Path]::GetFullPath($TargetDir)
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
if (-not $BinaryPath) { $BinaryPath = Join-Path $TargetDir 'release\covalos.exe' }
$BinaryPath = [IO.Path]::GetFullPath($BinaryPath)
$manifest = Get-Content -LiteralPath (Join-Path $repo 'Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"') { throw 'Expected a three-part package version in Cargo.toml.' }
$version = $Matches[1]
if (-not $IsccPath) {
    $compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($compiler) { $IsccPath = $compiler.Source }
    else {
        $programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
        $candidates = @(
            "$programFilesX86\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
            "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
        )
        $IsccPath = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
}
if (-not $IsccPath -or -not (Test-Path -LiteralPath $IsccPath)) {
    throw 'Inno Setup 6.3+ is required. Install it from https://jrsoftware.org/isdl.php or pass -IsccPath.'
}
$IsccPath = [IO.Path]::GetFullPath($IsccPath)
Push-Location $repo
try {
    if (-not $SkipBuild) {
        & cargo build --release --locked --target-dir $TargetDir
        if ($LASTEXITCODE -ne 0) { throw 'Cargo release build failed.' }
    }
    if (-not (Test-Path -LiteralPath $BinaryPath)) { throw "Missing executable: $BinaryPath" }
    $binary = Get-Item -LiteralPath $BinaryPath
    if ($binary.VersionInfo.ProductName -ne 'COVALOS' -or $binary.VersionInfo.ProductVersion -ne "$version.0") {
        throw "Executable identity must be COVALOS $version.0. Rebuild from this checkout."
    }
    $bytes = [IO.File]::ReadAllBytes($BinaryPath)
    $pe = [BitConverter]::ToInt32($bytes, 0x3c)
    if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) { throw 'Expected a Windows x64 executable.' }
    $stage = Join-Path $repo 'target\windows-package'
    New-Item -ItemType Directory -Force -Path $stage,$OutputDir | Out-Null
    Copy-Item -LiteralPath $BinaryPath -Destination (Join-Path $stage 'covalos.exe') -Force
    $loader = Join-Path (Split-Path -Parent $BinaryPath) 'WebView2Loader.dll'
    if (-not (Test-Path -LiteralPath $loader -PathType Leaf)) { throw 'The Windows GNU build is missing WebView2Loader.dll. Build Covalos before packaging.' }
    Copy-Item -LiteralPath $loader -Destination (Join-Path $stage 'WebView2Loader.dll') -Force
    $probeDir = Join-Path $TargetDir ('startup-check-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $probeDir | Out-Null
    $previousConfig = $env:COVALOS_CONFIG_DIR
    try {
        $env:COVALOS_CONFIG_DIR = $probeDir
        $probe = Start-Process -FilePath (Join-Path $stage 'covalos.exe') -ArgumentList '--library','sources' -WorkingDirectory $stage -WindowStyle Hidden -RedirectStandardOutput (Join-Path $probeDir 'sources.json') -PassThru
        $null = $probe.Handle
        if (-not $probe.WaitForExit(15000)) { $probe.Kill(); throw 'The staged application timed out at startup.' }
        if ($probe.ExitCode -ne 0) { throw "The staged application failed at startup with exit code $($probe.ExitCode)." }
        Get-Content -LiteralPath (Join-Path $probeDir 'sources.json') -Raw | ConvertFrom-Json | Out-Null
    } finally {
        $env:COVALOS_CONFIG_DIR = $previousConfig
    }
    Copy-Item -LiteralPath (Join-Path $repo 'assets\brand\covalos-icon.ico') -Destination (Join-Path $stage 'covalos.ico') -Force
    $licenseDir = Join-Path $stage 'licenses'
    New-Item -ItemType Directory -Force -Path $licenseDir | Out-Null
    $assetNotices = @(
        'assets/fonts/Geist-OFL.txt', 'assets/fonts/IBMPlexMono-OFL.txt',
        'assets/icons/ui/LICENSE', 'assets/icons/ui/provenance.json',
        'assets/icons/material/LICENSE', 'assets/icons/material/provenance.json',
        'assets/icons/vscode/LICENSE', 'assets/icons/vscode/provenance.json',
        'assets/brands/PROVENANCE.md'
    )
    foreach ($relative in $assetNotices) {
        $source = Join-Path $repo $relative
        if (Test-Path -LiteralPath $source) {
            $destination = Join-Path $licenseDir ($relative -replace '/', '-')
            Copy-Item -LiteralPath $source -Destination $destination -Force
        }
    }
    # Resolve production dependencies for Windows; omit dev-only dependency edges.
    $metadataJson = & cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-gnu
    if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve dependency notices with cargo metadata.' }
    $metadata = ($metadataJson -join [Environment]::NewLine) | ConvertFrom-Json
    $nodes = @{}
    $packages = @{}
    foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
    foreach ($package in $metadata.packages) { $packages[$package.id] = $package }
    $seen = New-Object 'System.Collections.Generic.HashSet[string]'
    $pending = New-Object 'System.Collections.Generic.Queue[string]'
    $pending.Enqueue($metadata.resolve.root)
    while ($pending.Count -gt 0) {
        $id = $pending.Dequeue()
        if (-not $seen.Add($id)) { continue }
        foreach ($dep in $nodes[$id].deps) {
            if (@($dep.dep_kinds | Where-Object { $_.kind -ne 'dev' }).Count -gt 0) { $pending.Enqueue($dep.pkg) }
        }
    }
    $notices = New-Object 'System.Collections.Generic.List[string]'
    $notices.Add('COVALOS - Third-party notices')
    $notices.Add('Fonts: Geist and IBM Plex Mono (SIL OFL). UI: Lucide (ISC). File icons: Material Icon Theme and vscode-icons (MIT).')
    $notices.Add('WebView2Loader.dll is distributed with the Microsoft Edge WebView2 SDK. The embedded web preview also uses the Microsoft Edge WebView2 Runtime installed on the machine.')
    $notices.Add('Provider marks identify their respective integrations and remain the property of their owners. See licenses/assets-brands-PROVENANCE.md.')
    $notices.Add('The license texts below apply to the named third-party components. They do not grant a license to COVALOS itself.')
    foreach ($package in @($seen | Where-Object { $_ -ne $metadata.resolve.root } | ForEach-Object { $packages[$_] } | Sort-Object name,version)) {
        $notices.Add("")
        $notices.Add(("{0} {1} | {2} | {3}" -f $package.name,$package.version,$package.license,$package.repository))
        $packageDir = Split-Path -Parent $package.manifest_path
        foreach ($license in @(Get-ChildItem -LiteralPath $packageDir -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)([._-].*)?$' })) {
            $destination = Join-Path $licenseDir ("{0}-{1}-{2}" -f $package.name,$package.version,$license.Name)
            Copy-Item -LiteralPath $license.FullName -Destination $destination -Force
        }
        if ($package.license_file) {
            $licenseFile = Join-Path $packageDir $package.license_file
            if (Test-Path -LiteralPath $licenseFile -PathType Leaf) {
                Copy-Item -LiteralPath $licenseFile -Destination (Join-Path $licenseDir ("{0}-{1}-LICENSE-file.txt" -f $package.name,$package.version)) -Force
            }
        }
    }
    $notices | Set-Content -LiteralPath (Join-Path $stage 'THIRD_PARTY_NOTICES.txt') -Encoding UTF8
    & $IsccPath "/DAppVersion=$version" "/DPayloadDir=$stage" "/DBuildOutputDir=$OutputDir" (Join-Path $repo 'packaging\windows\covalos.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed.' }
    $installer = Join-Path $OutputDir "Covalos-$version-Windows-x64-Setup.exe"
    if (-not (Test-Path -LiteralPath $installer)) { throw 'Compiler did not produce the expected installer.' }
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($installer))" | Set-Content -LiteralPath "$installer.sha256" -Encoding ASCII
    [ordered]@{
        name = 'COVALOS'; version = $version; architecture = 'x64'; scope = 'current-user'
        commit = ((& git rev-parse HEAD) -join '').Trim()
        binary_sha256 = (Get-FileHash -LiteralPath $BinaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
        installer_sha256 = $hash; built_at_utc = [DateTime]::UtcNow.ToString('o')
        signed = ((Get-AuthenticodeSignature -LiteralPath $installer).Status -eq 'Valid')
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDir 'build-info.json') -Encoding UTF8
    Write-Host "Created $installer"
}
finally { Pop-Location }
