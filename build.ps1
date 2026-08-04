# Build Windows executables (CLI + GUI).
# Outputs:
#   target\release\tluw.exe
#   target\release\tluw-gui.exe

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

Write-Host "Building Telemetry and Logging Utility for Windows (CLI + GUI)..." -ForegroundColor Cyan
cargo build --release --features "cli,gui"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$cli = Join-Path $PSScriptRoot "target\release\tluw.exe"
$gui = Join-Path $PSScriptRoot "target\release\tluw-gui.exe"
Write-Host "OK:" -ForegroundColor Green
Get-Item $cli, $gui | Format-Table Name, Length, LastWriteTime -AutoSize
