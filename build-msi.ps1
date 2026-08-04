# Build MSI with cargo-wix (WiX Toolset).
# Prefer a separate target dir so a running GUI does not lock release binaries.

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$wixBin = Join-Path $PSScriptRoot "tools\wix"
$wixRoot = Join-Path $PSScriptRoot "tools\wix-root"
if (-not (Test-Path "$wixBin\candle.exe")) {
    Write-Error "WiX binaries missing. Expected candle.exe under tools\wix. See README."
}
if (-not (Test-Path "$wixRoot\bin\candle.exe")) {
    New-Item -ItemType Directory -Force -Path "$wixRoot\bin" | Out-Null
    Copy-Item "$wixBin\*" "$wixRoot\bin\" -Force
}

$env:WIX = $wixRoot
$env:PATH = "$wixBin;$env:PATH"
$env:CARGO_TARGET_DIR = Join-Path $PSScriptRoot "target-msi"

Write-Host "Building release binaries into target-msi ..." -ForegroundColor Cyan
cargo build --release --features "cli,gui"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Creating MSI with cargo wix ..." -ForegroundColor Cyan
$binDir = Join-Path $env:CARGO_TARGET_DIR "release"
cargo wix --no-build --bin-path $wixBin --target-bin-dir $binDir
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$msi = Get-ChildItem (Join-Path $env:CARGO_TARGET_DIR "wix") -Filter "*.msi" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

# Also copy under standard target\wix for docs
$outDir = Join-Path $PSScriptRoot "target\wix"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
if ($msi) {
    Copy-Item $msi.FullName $outDir -Force
    Write-Host "MSI: $($msi.FullName)" -ForegroundColor Green
    Write-Host "Also: $(Join-Path $outDir $msi.Name)" -ForegroundColor Green
} else {
    Get-ChildItem (Join-Path $PSScriptRoot "target\wix") -Filter "*.msi" -ErrorAction SilentlyContinue
    Write-Host "Look under target\wix or target-msi\wix for the MSI." -ForegroundColor Yellow
}
