# Build Windows executables (CLI + GUI).
# Outputs:
#   target\release\windows-diagnostics.exe
#   target\release\windows-diagnostics-gui.exe

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

Write-Host "Building windows-diagnostics (CLI + GUI)..." -ForegroundColor Cyan
cargo build --release --features "cli,gui"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$cli = Join-Path $PSScriptRoot "target\release\windows-diagnostics.exe"
$gui = Join-Path $PSScriptRoot "target\release\windows-diagnostics-gui.exe"
Write-Host "OK:" -ForegroundColor Green
Get-Item $cli, $gui | Format-Table Name, Length, LastWriteTime -AutoSize
