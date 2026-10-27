# Downloads the latest prebuilt libmpv for Windows (shinchiro's mpv-winbuild-cmake
# builds, the same ones linked from mpv.io) and copies libmpv-2.dll next to the
# Lumen executables.
#
# Usage (from the repo root):  powershell -ExecutionPolicy Bypass -File scripts\fetch-libmpv.ps1
# Pass -V3 for the build optimized for newer CPUs (x86-64-v3, roughly Haswell / Zen and later).

param([switch]$V3)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$work = Join-Path $root "target\libmpv"
New-Item -ItemType Directory -Force -Path $work | Out-Null

Write-Host "Looking up the latest libmpv build..."
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest" `
    -Headers @{ "User-Agent" = "lumen-setup" }
$pattern = if ($V3) { '^mpv-dev-x86_64-v3-\d{8}-git-.*\.7z$' } else { '^mpv-dev-x86_64-\d{8}-git-.*\.7z$' }
$asset = $release.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1
if (-not $asset) {
    throw "No asset matching $pattern in release $($release.tag_name). Download mpv-dev-x86_64-*.7z manually from https://github.com/shinchiro/mpv-winbuild-cmake/releases and extract libmpv-2.dll next to lumen.exe."
}

$archive = Join-Path $work $asset.name
if (-not (Test-Path $archive)) {
    Write-Host "Downloading $($asset.name)..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archive
}

$extract = Join-Path $work "extract"
Remove-Item -Recurse -Force $extract -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $extract | Out-Null

# Recent Windows 11 builds of tar.exe read .7z; otherwise fall back to 7-Zip.
$ok = $false
try {
    tar -xf $archive -C $extract 2>$null
    $ok = $LASTEXITCODE -eq 0
} catch { }
if (-not $ok) {
    $sevenZip = @(
        (Get-Command 7z -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source),
        "$env:ProgramFiles\7-Zip\7z.exe",
        "${env:ProgramFiles(x86)}\7-Zip\7z.exe"
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $sevenZip) {
        throw "Could not extract $archive. Install 7-Zip (winget install 7zip.7zip) or extract it manually."
    }
    & $sevenZip x $archive "-o$extract" -y | Out-Null
}

$dll = Get-ChildItem -Path $extract -Recurse -Filter "libmpv-2.dll" | Select-Object -First 1
if (-not $dll) { throw "libmpv-2.dll not found inside $($asset.name)" }

foreach ($profile in "debug", "release") {
    $dest = Join-Path $root "target\$profile"
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    Copy-Item $dll.FullName $dest -Force
    Write-Host "Copied libmpv-2.dll -> $dest"
}
Write-Host "Done. Run: cargo run --release -- path\to\video.mkv"
