# Package a Windows build of SubText.
#
#     pwsh tools/package-windows.ps1
#     pwsh tools/package-windows.ps1 -NoBuild
#     pwsh tools/package-windows.ps1 -Target x86_64-pc-windows-msvc
#
# Produces dist/SubText-<version>-windows-<arch>.zip, containing subtext.exe,
# the resource directory beside it, and the licence files. The executable is a
# GUI subsystem binary (see main.rs), so double-clicking it opens a window and
# not a console.
#
# There is no installer: the archive is portable, which keeps the release free
# of an MSI toolchain and of the registry writes an installer would make. A user
# unzips it anywhere and runs subtext.exe.

[CmdletBinding()]
param(
    [string]$Binary = "",
    [string]$Target = "",
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

if (-not $Binary) {
    if ($Target) {
        $Binary = Join-Path $Root "target/$Target/release/subtext.exe"
    } else {
        $Binary = Join-Path $Root "target/release/subtext.exe"
    }
}

# Workspace version: the first `version = "…"` in Cargo.toml is
# [workspace.package]; member crates inherit it.
$Version = (Select-String -Path "Cargo.toml" -Pattern '^version = "(.*)"' |
    Select-Object -First 1).Matches.Groups[1].Value
if (-not $Version) {
    throw "could not read the version from Cargo.toml"
}

switch -Wildcard ($Target) {
    "*aarch64*" { $Arch = "aarch64" }
    "*x86_64*"  { $Arch = "x86_64" }
    default {
        if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { $Arch = "aarch64" }
        else { $Arch = "x86_64" }
    }
}

if (-not $NoBuild) {
    Write-Host "==> building subtext $Version for $(if ($Target) { $Target } else { 'the host' }) (release)"
    if ($Target) {
        cargo build --release --bin subtext --target $Target
    } else {
        cargo build --release --bin subtext
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}

if (-not (Test-Path $Binary)) {
    throw "$Binary does not exist — run without -NoBuild"
}

$Name = "SubText-$Version-windows-$Arch"
$Dist = Join-Path $Root "dist"
$Stage = Join-Path $Dist $Name

Write-Host "==> assembling $Stage"
if (Test-Path $Stage) { Remove-Item $Stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $Stage "resources") | Out-Null

Copy-Item $Binary (Join-Path $Stage "subtext.exe")

# Resources::discover() looks for `resources` beside the executable first, so
# the archive keeps the loose directory the build tree uses.
foreach ($resource in @("syntaxes", "themes", "queries")) {
    $source = Join-Path $Root "resources/$resource"
    if (Test-Path $source) {
        Copy-Item $source (Join-Path $Stage "resources/$resource") -Recurse
    }
}
Copy-Item "resources/menu.json" (Join-Path $Stage "resources/menu.json")

Copy-Item "LICENSE" $Stage
Copy-Item "CHANGELOG.md" $Stage
Copy-Item "README.md" $Stage
if (Test-Path "THIRD-PARTY-NOTICES.md") {
    Copy-Item "THIRD-PARTY-NOTICES.md" $Stage
}

$Archive = Join-Path $Dist "$Name.zip"
Write-Host "==> archiving $Archive"
if (Test-Path $Archive) { Remove-Item $Archive -Force }
Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Archive -CompressionLevel Optimal

# The archive is what ships; the staging directory is only an intermediate.
Remove-Item $Stage -Recurse -Force

Write-Host ""
Write-Host "done."
Write-Host "  archive: $Archive"
Write-Host "  run:     expand the archive and double-click subtext.exe"
