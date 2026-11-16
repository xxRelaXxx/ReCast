<#
.SYNOPSIS
    Builds ReCast: the desktop app, the command line tool, the phone app and
    the one-file launcher.

.DESCRIPTION
    Puts everything together and produces the release archive:

      1. cargo build --release -p recast-app
      2. cargo build --release -p recast-cli
      3. gradlew assembleRelease                (unless -SkipAndroid)
      4. stage the runtime into apps\launcher\payload
      5. cargo build --release --manifest-path apps\launcher\Cargo.toml
      6. copy the launcher to desktop\ReCast.exe and the APK to phone\ReCastPhone.apk
      7. write the SHA-256 checksums and create release\ReCast-<version>-win64.zip

.EXAMPLE
    .\tools\build-release.ps1

.EXAMPLE
    .\tools\build-release.ps1 -SkipAndroid -Version 1.0.1
#>

[CmdletBinding()]
param(
    [string] $Version = '',
    [switch] $SkipDesktop,
    [switch] $SkipAndroid,
    [switch] $SkipLauncher,
    [switch] $NoZip
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
$payload = Join-Path $root 'apps\launcher\payload'
$desktopOut = Join-Path $root 'desktop'
$phoneOut = Join-Path $root 'phone'
$releaseDir = Join-Path $root 'release'
$toolsDir = Join-Path $root 'tools'

function Write-Step([string] $Text) {
    Write-Host ''
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Write-Info([string] $Text) {
    Write-Host "    $Text" -ForegroundColor DarkGray
}

function Get-FileSha256([string] $Path) {
    return (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-CrateVersion([string] $Manifest) {
    $match = Select-String -Path $Manifest -Pattern '^version\s*=\s*"([^"]+)"'
    return $match.Matches[0].Groups[1].Value
}

# ---------------------------------------------------------------------------
# Versions
# ---------------------------------------------------------------------------

$appVersion = Get-CrateVersion (Join-Path $root 'apps\app\Cargo.toml')
$cliVersion = Get-CrateVersion (Join-Path $root 'apps\cli\Cargo.toml')
$coreVersion = Get-CrateVersion (Join-Path $root 'apps\core\Cargo.toml')
$launcherVersion = Get-CrateVersion (Join-Path $root 'apps\launcher\Cargo.toml')
$androidVersion = (Select-String -Path (Join-Path $root 'android\app\build.gradle.kts') -Pattern 'versionName\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

if (-not $Version) { $Version = $appVersion }

Write-Step "ReCast $Version"
Write-Info "app $appVersion, cli $cliVersion, core $coreVersion, launcher $launcherVersion, phone app $androidVersion"

$versions = @(@($appVersion, $cliVersion, $coreVersion, $launcherVersion, $androidVersion) | Select-Object -Unique)
if ($versions.Count -ne 1) {
    throw "The versions do not match. Update the Cargo.toml files and android\app\build.gradle.kts."
}

# ---------------------------------------------------------------------------
# 1. Desktop app
# ---------------------------------------------------------------------------

$appExe = Join-Path $root 'target\release\recast-app.exe'

if ($SkipDesktop) {
    Write-Step 'Desktop app (skipped)'
} else {
    Write-Step 'Building the desktop app'
    Push-Location $root
    try {
        cargo build --release -p recast-app
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
    Write-Info "target\release\recast-app.exe ($((Get-Item $appExe).Length) bytes)"
}

# ---------------------------------------------------------------------------
# 2. Command line tool
# ---------------------------------------------------------------------------

$cliExe = Join-Path $root 'target\release\recast.exe'

if ($SkipDesktop) {
    Write-Step 'Command line tool (skipped)'
} else {
    Write-Step 'Building the command line tool'
    Push-Location $root
    try {
        cargo build --release -p recast-cli
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
    Write-Info "target\release\recast.exe ($((Get-Item $cliExe).Length) bytes)"
}

# ---------------------------------------------------------------------------
# 3. Phone app
# ---------------------------------------------------------------------------

$apk = Join-Path $root 'android\app\build\outputs\apk\release\app-release.apk'

if ($SkipAndroid) {
    Write-Step 'Phone app (skipped)'
    if (-not (Test-Path $apk)) {
        throw "No APK found at $apk. Build it first or drop -SkipAndroid."
    }
} else {
    Write-Step 'Building the phone app'
    Push-Location (Join-Path $root 'android')
    try {
        & (Join-Path $root 'android\gradlew.bat') assembleRelease
        if ($LASTEXITCODE -ne 0) { throw "gradlew assembleRelease failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
    Write-Info "app-release.apk ($((Get-Item $apk).Length) bytes)"
}

# ---------------------------------------------------------------------------
# 4. Runtime payload
# ---------------------------------------------------------------------------

Write-Step 'Checking the runtime payload'

$ffmpeg = Join-Path $toolsDir 'ffmpeg\ffmpeg.exe'
$vcam64 = Join-Path $toolsDir 'vcam\x64\softcam.dll'
$vcam86 = Join-Path $toolsDir 'vcam\x86\softcam.dll'

foreach ($required in @($ffmpeg, $vcam64, $vcam86, $appExe, $cliExe, $apk)) {
    if (-not (Test-Path $required)) {
        throw "Missing: $required`nBuild it with the instructions in docs\BUILDING.md."
    }
    Write-Info "$([System.IO.Path]::GetFileName($required)) $(Get-FileSha256 $required)"
}

New-Item -ItemType Directory -Force -Path $payload | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $payload 'vcam\x64') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $payload 'vcam\x86') | Out-Null

Copy-Item $appExe (Join-Path $payload 'recast-app.exe') -Force
Copy-Item $cliExe (Join-Path $payload 'recast-cli.exe') -Force
Copy-Item $ffmpeg (Join-Path $payload 'ffmpeg.exe') -Force
Copy-Item $apk (Join-Path $payload 'ReCastPhone.apk') -Force
Copy-Item $vcam64 (Join-Path $payload 'vcam\x64\softcam.dll') -Force
Copy-Item $vcam86 (Join-Path $payload 'vcam\x86\softcam.dll') -Force

Write-Info 'payload staged in apps\launcher\payload'

# ---------------------------------------------------------------------------
# 5. Launcher
# ---------------------------------------------------------------------------

$launcherExe = Join-Path $root 'apps\launcher\target\release\ReCast.exe'

if ($SkipLauncher) {
    Write-Step 'Launcher (skipped)'
} else {
    Write-Step 'Building the launcher'
    Push-Location $root
    try {
        cargo build --release --manifest-path (Join-Path $root 'apps\launcher\Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
    Write-Info "apps\launcher\target\release\ReCast.exe ($((Get-Item $launcherExe).Length) bytes)"
}

# ---------------------------------------------------------------------------
# 6. Publish
# ---------------------------------------------------------------------------

Write-Step 'Publishing the ready-to-use files'

New-Item -ItemType Directory -Force -Path $desktopOut | Out-Null
New-Item -ItemType Directory -Force -Path $phoneOut | Out-Null

Copy-Item $launcherExe (Join-Path $desktopOut 'ReCast.exe') -Force
Copy-Item $apk (Join-Path $phoneOut 'ReCastPhone.apk') -Force

Write-Info "desktop\ReCast.exe         $(Get-FileSha256 (Join-Path $desktopOut 'ReCast.exe'))"
Write-Info "phone\ReCastPhone.apk     $(Get-FileSha256 (Join-Path $phoneOut 'ReCastPhone.apk'))"

# ---------------------------------------------------------------------------
# 7. Archive and checksums
# ---------------------------------------------------------------------------

if (-not $NoZip) {
    Write-Step 'Creating the release archive'

    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
    $staging = Join-Path $releaseDir "ReCast-$Version"
    if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }

    New-Item -ItemType Directory -Force -Path (Join-Path $staging 'desktop') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $staging 'phone') | Out-Null

    Copy-Item (Join-Path $desktopOut 'ReCast.exe') (Join-Path $staging 'desktop\ReCast.exe') -Force
    Copy-Item (Join-Path $phoneOut 'ReCastPhone.apk') (Join-Path $staging 'phone\ReCastPhone.apk') -Force
    Copy-Item (Join-Path $root 'README.md') $staging -Force
    Copy-Item (Join-Path $root 'LICENSE') $staging -Force
    Copy-Item (Join-Path $root 'CONTRIBUTING.md') $staging -Force
    Copy-Item (Join-Path $root 'THIRD_PARTY_NOTICES.md') $staging -Force
    Copy-Item (Join-Path $root 'docs\README-first.txt') $staging -Force
    Copy-Item (Join-Path $root 'docs\USER-GUIDE.md') (Join-Path $staging 'USER-GUIDE.md') -Force
    New-Item -ItemType Directory -Force -Path (Join-Path $staging 'docs\images') | Out-Null
    Copy-Item (Join-Path $root 'docs\ARCHITECTURE.md') (Join-Path $staging 'docs') -Force
    Copy-Item (Join-Path $root 'docs\BUILDING.md') (Join-Path $staging 'docs') -Force
    Copy-Item (Join-Path $root 'docs\USER-GUIDE.md') (Join-Path $staging 'docs') -Force
    Copy-Item (Join-Path $root 'docs\images\*') (Join-Path $staging 'docs\images') -Force

    $zip = Join-Path $releaseDir "ReCast-$Version-win64.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zip -CompressionLevel Optimal

    $checksums = @(
        "$(Get-FileSha256 $zip)  ReCast-$Version-win64.zip",
        "$(Get-FileSha256 (Join-Path $desktopOut 'ReCast.exe'))  desktop/ReCast.exe",
        "$(Get-FileSha256 (Join-Path $phoneOut 'ReCastPhone.apk'))  phone/ReCastPhone.apk",
        "$(Get-FileSha256 $ffmpeg)  ffmpeg.exe"
    )
    $checksums | Set-Content -Path (Join-Path $releaseDir 'SHA256SUMS.txt') -Encoding ascii

    Write-Info "release\ReCast-$Version-win64.zip ($((Get-Item $zip).Length) bytes)"
    Write-Info 'release\SHA256SUMS.txt'
}

Write-Step 'Done'
Write-Host ''
Write-Host "Desktop app : desktop\ReCast.exe" -ForegroundColor Green
Write-Host "Phone app   : phone\ReCastPhone.apk" -ForegroundColor Green
Write-Host "CLI         : recast  (installed to %LOCALAPPDATA%\ReCast\bin on first run)" -ForegroundColor Green
Write-Host ''
Write-Host 'Commit the two files above before tagging a release.' -ForegroundColor Yellow
