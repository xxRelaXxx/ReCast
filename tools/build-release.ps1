<#
.SYNOPSIS
    Builds Rewebcam: the desktop app, the phone app and the one-file launcher.

.DESCRIPTION
    Puts everything together and produces the release archive:

      1. cargo build --release -p rewebcam
      2. gradlew assembleRelease                (unless -SkipAndroid)
      3. stage the runtime into apps\launcher\payload
      4. cargo build --release --manifest-path apps\launcher\Cargo.toml
      5. copy the launcher to desktop\Rewebcam.exe and the APK to phone\RewebcamPhone.apk
      6. write the checksums and create release\ReWebCam-<version>-win64.zip

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

# ---------------------------------------------------------------------------
# Versions
# ---------------------------------------------------------------------------

$desktopVersion = (Select-String -Path (Join-Path $root 'apps\desktop\Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$launcherVersion = (Select-String -Path (Join-Path $root 'apps\launcher\Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$androidVersion = (Select-String -Path (Join-Path $root 'android\app\build.gradle.kts') -Pattern 'versionName\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

if (-not $Version) { $Version = $desktopVersion }

Write-Step "Rewebcam $Version"
Write-Info "desktop app $desktopVersion, launcher $launcherVersion, phone app $androidVersion"

if ($desktopVersion -ne $launcherVersion -or $desktopVersion -ne $androidVersion) {
    throw "The versions do not match. Update apps\desktop\Cargo.toml, apps\launcher\Cargo.toml and android\app\build.gradle.kts."
}

# ---------------------------------------------------------------------------
# 1. Desktop app
# ---------------------------------------------------------------------------

$desktopExe = Join-Path $root 'target\release\rewebcam.exe'

if ($SkipDesktop) {
    Write-Step 'Desktop app (skipped)'
} else {
    Write-Step 'Building the desktop app'
    Push-Location $root
    try {
        cargo build --release -p rewebcam
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
    Write-Info "target\release\rewebcam.exe ($((Get-Item $desktopExe).Length) bytes)"
}

# ---------------------------------------------------------------------------
# 2. Phone app
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
# 3. Runtime payload
# ---------------------------------------------------------------------------

Write-Step 'Checking the runtime payload'

$ffmpeg = Join-Path $toolsDir 'ffmpeg\ffmpeg.exe'
$vcam64 = Join-Path $toolsDir 'vcam\x64\softcam.dll'
$vcam86 = Join-Path $toolsDir 'vcam\x86\softcam.dll'

foreach ($required in @($ffmpeg, $vcam64, $vcam86, $desktopExe, $apk)) {
    if (-not (Test-Path $required)) {
        throw "Missing: $required`nBuild it with the instructions in docs\BUILDING.md."
    }
    Write-Info "$([System.IO.Path]::GetFileName($required)) $(Get-FileSha256 $required)"
}

New-Item -ItemType Directory -Force -Path $payload | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $payload 'vcam\x64') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $payload 'vcam\x86') | Out-Null

Copy-Item $desktopExe (Join-Path $payload 'rewebcam.exe') -Force
Copy-Item $ffmpeg (Join-Path $payload 'ffmpeg.exe') -Force
Copy-Item $apk (Join-Path $payload 'RewebcamPhone.apk') -Force
Copy-Item $vcam64 (Join-Path $payload 'vcam\x64\softcam.dll') -Force
Copy-Item $vcam86 (Join-Path $payload 'vcam\x86\softcam.dll') -Force

Write-Info 'payload staged in apps\launcher\payload'

# ---------------------------------------------------------------------------
# 4. Launcher
# ---------------------------------------------------------------------------

$launcherExe = Join-Path $root 'apps\launcher\target\release\Rewebcam.exe'

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
    Write-Info "apps\launcher\target\release\Rewebcam.exe ($((Get-Item $launcherExe).Length) bytes)"
}

# ---------------------------------------------------------------------------
# 5. Publish
# ---------------------------------------------------------------------------

Write-Step 'Publishing the ready-to-use files'

New-Item -ItemType Directory -Force -Path $desktopOut | Out-Null
New-Item -ItemType Directory -Force -Path $phoneOut | Out-Null

Copy-Item $launcherExe (Join-Path $desktopOut 'Rewebcam.exe') -Force
Copy-Item $apk (Join-Path $phoneOut 'RewebcamPhone.apk') -Force

Write-Info "desktop\Rewebcam.exe        $(Get-FileSha256 (Join-Path $desktopOut 'Rewebcam.exe'))"
Write-Info "phone\RewebcamPhone.apk    $(Get-FileSha256 (Join-Path $phoneOut 'RewebcamPhone.apk'))"

# ---------------------------------------------------------------------------
# 6. Archive and checksums
# ---------------------------------------------------------------------------

if (-not $NoZip) {
    Write-Step 'Creating the release archive'

    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
    $staging = Join-Path $releaseDir "ReWebCam-$Version"
    if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }

    New-Item -ItemType Directory -Force -Path (Join-Path $staging 'desktop') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $staging 'phone') | Out-Null

    Copy-Item (Join-Path $desktopOut 'Rewebcam.exe') (Join-Path $staging 'desktop\Rewebcam.exe') -Force
    Copy-Item (Join-Path $phoneOut 'RewebcamPhone.apk') (Join-Path $staging 'phone\RewebcamPhone.apk') -Force
    Copy-Item (Join-Path $root 'README.md') $staging -Force
    Copy-Item (Join-Path $root 'LICENSE') $staging -Force
    Copy-Item (Join-Path $root 'THIRD_PARTY_NOTICES.md') $staging -Force
    Copy-Item (Join-Path $root 'docs\README-first.txt') $staging -Force
    Copy-Item (Join-Path $root 'docs\USER-GUIDE.md') (Join-Path $staging 'USER-GUIDE.md') -Force

    $zip = Join-Path $releaseDir "ReWebCam-$Version-win64.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zip -CompressionLevel Optimal

    $checksums = @(
        "$(Get-FileSha256 $zip)  ReWebCam-$Version-win64.zip",
        "$(Get-FileSha256 (Join-Path $desktopOut 'Rewebcam.exe'))  desktop/Rewebcam.exe",
        "$(Get-FileSha256 (Join-Path $phoneOut 'RewebcamPhone.apk'))  phone/RewebcamPhone.apk",
        "$(Get-FileSha256 $ffmpeg)  ffmpeg.exe"
    )
    $checksums | Set-Content -Path (Join-Path $releaseDir 'SHA256SUMS.txt') -Encoding ascii

    Write-Info "release\ReWebCam-$Version-win64.zip ($((Get-Item $zip).Length) bytes)"
    Write-Info 'release\SHA256SUMS.txt'
}

Write-Step 'Done'
Write-Host ''
Write-Host "Desktop app : desktop\Rewebcam.exe" -ForegroundColor Green
Write-Host "Phone app   : phone\RewebcamPhone.apk" -ForegroundColor Green
Write-Host ''
Write-Host 'Commit the two files above before tagging a release.' -ForegroundColor Yellow
