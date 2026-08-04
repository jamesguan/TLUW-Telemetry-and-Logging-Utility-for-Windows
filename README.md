# Windows Diagnostics

Modular Rust tools to inspect and toggle Windows diagnostic data / telemetry.

| Artifact | Role |
|----------|------|
| `windows-diagnostics.exe` | **CLI** |
| `windows-diagnostics-gui.exe` | **GUI** |
| `Windows Diagnostics-*.msi` | **Installer** (desktop shortcut, optional startup + post-update) |

**Modularity:** core logic lives in the library (`telemetry`, `maintenance`). The CLI exposes commands; the GUI is a thin front-end over the same APIs (status, disable, enable, set, startup, post-update, integration). Prefer extending the library + CLI first; keep both binaries independently runnable.

```text
src/
  lib.rs / telemetry.rs / maintenance.rs / gui.rs
  bin/cli.rs
  bin/gui.rs
wix/main.wxs                  MSI definition (cargo-wix)
.github/workflows/release.yml  Merge to main → build MSI → GitHub Release
```

---

## Install (end users)

1. Download the `.msi` from [GitHub Releases](https://github.com/jamesguan/disable-windows-diagnostics/releases).
2. Run the installer (Administrator).
3. On the feature page you can enable:
   - **Desktop shortcut** (on by default)
   - **Add to PATH** (on by default)
   - **Run when Windows starts** (off by default) — Startup-folder shortcut to the GUI
   - **Re-apply after Windows Update** (off by default) — scheduled task on WU Event ID 19 + logon backup that runs `windows-diagnostics disable`

You can also toggle startup / post-update later in the GUI (**Automation**) or via CLI:

```powershell
windows-diagnostics startup on
windows-diagnostics post-update on
windows-diagnostics integration
```

---

## How to build (developers)

Requires [Rust](https://rustup.rs/).

```powershell
cd C:\Users\Garuda\Projects\windows-diagnostics
cargo build --release
```

Outputs:

```text
target\release\windows-diagnostics.exe
target\release\windows-diagnostics-gui.exe
```

### Build the MSI installer (`cargo wix`)

WiX Toolset binaries are downloaded once into `tools\wix` (gitignored):

```powershell
.\build-msi.ps1
```

Or manually:

```powershell
# One-time: install cargo-wix + WiX binaries (see build-msi.ps1)
cargo install cargo-wix --locked
cargo build --release
cargo wix --no-build --bin-path .\tools\wix
```

MSI output: `target\wix\`. `build-msi.ps1` uses `target-msi\` so a running GUI does not lock `target\release\*.exe`.

Installer options (feature tree):

| Feature | Default | Effect |
|---------|---------|--------|
| Desktop shortcut | On | Shortcut to GUI on Desktop |
| Add to PATH | On | `bin` on system PATH |
| Run when Windows starts | Off | Startup folder → GUI |
| Re-apply after Windows Update | Off | Task `WindowsDiagnosticsPostUpdate` |

**Post-update behavior:** listens for Microsoft Windows Update Client **Event ID 19** (successful install), waits 2 minutes, then runs `windows-diagnostics disable --no-elevate`. A **logon** trigger is also registered as a backup when the event is missed (common after large feature updates).

---

## How to run

### GUI

```powershell
.\target\release\windows-diagnostics-gui.exe
# or after install:
# & "${env:ProgramFiles}\Windows Diagnostics\bin\windows-diagnostics-gui.exe"
```

### CLI

```powershell
.\target\release\windows-diagnostics.exe status
.\target\release\windows-diagnostics.exe disable
.\target\release\windows-diagnostics.exe set diagtrack off
.\target\release\windows-diagnostics.exe explain diagnostic-data
.\target\release\windows-diagnostics.exe startup on
.\target\release\windows-diagnostics.exe post-update on
.\target\release\windows-diagnostics.exe integration

# Windows logs / tools (same buttons as GUI)
.\target\release\windows-diagnostics.exe logs
.\target\release\windows-diagnostics.exe open event-viewer
.\target\release\windows-diagnostics.exe open privacy-feedback

# Clear logs (destructive; most need admin + --confirm)
.\target\release\windows-diagnostics.exe clear-list
.\target\release\windows-diagnostics.exe clear diagnosis --confirm
.\target\release\windows-diagnostics.exe clear event-application --confirm
.\target\release\windows-diagnostics.exe clear-all --confirm
```

---

## How to test

```powershell
cargo build --release
.\target\release\windows-diagnostics.exe status
.\target\release\windows-diagnostics.exe set diagtrack off
.\target\release\windows-diagnostics.exe status

# GUI: Verify status → change toggles → Verify status again
.\target\release\windows-diagnostics-gui.exe

# Installer locally
.\build-msi.ps1
# Run the MSI, confirm Desktop shortcut, optional features, then:
windows-diagnostics status
windows-diagnostics integration
schtasks /Query /TN WindowsDiagnosticsPostUpdate
```

---

## GitHub Releases (automatic)

Every **push/merge to `main`** runs `.github/workflows/release.yml`, builds the MSI + binaries, and publishes a GitHub Release.

- Tag format: `v{Cargo.toml version}-build.{run_number}` (unique each merge)
- Also still works for manual `v*` tags and **Actions → Release → Run workflow**

```powershell
# Typical flow: merge PR / push to main — release is created by CI
git checkout main
git merge james
git push origin main
```

Optional manual tag release:

```powershell
git tag v0.3.1
git push origin v0.3.1
```

Workflow: `.github/workflows/release.yml`

Repo: https://github.com/jamesguan/disable-windows-diagnostics

---

## Setting ids

| Id | What it controls |
|----|------------------|
| `diagnostic-data` | `AllowTelemetry` policy |
| `diagtrack` | Connected User Experiences service |
| `ceip-tasks` | CEIP / feedback scheduled tasks |
| `advertising-id` | Advertising ID |
| `tailored-experiences` | Tailored experiences |
| `ceip-policy` | SQM / CEIP policy |
| `app-inventory` | AppCompat AIT / inventory |

---

## Notes

- Does not disable Windows Update or Defender.
- Post-update re-apply cannot catch every Microsoft reset; the logon backup helps after feature updates.
- Windows Home may ignore some policy keys.
- Reboot after a full lockdown is recommended.

## License

[PolyForm Noncommercial License 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0) — see [LICENSE](LICENSE).

**Personal / private and other non-commercial use only.** Commercial use is not allowed.  
(This is source-available, not OSI “Open Source,” which requires allowing commercial use.)
