# Telemetry and Logging Utility for Windows

Published by **Chillcoders LLC**. Author: **James Guan**. Modular Rust tools to inspect and toggle Windows diagnostic data / telemetry.

| Artifact | Role |
|----------|------|
| `tluw.exe` | **CLI** |
| `tluw-gui.exe` | **GUI** |
| `Telemetry Logging Utility-*.msi` | **Installer** (desktop shortcut, optional startup + post-update) |

**Modularity:** core logic lives in the library (`telemetry`, `maintenance`). The CLI exposes commands; the GUI is a thin front-end over the same APIs (status, disable, enable, set, startup, post-update, integration). Prefer extending the library + CLI first; keep both binaries independently runnable.

```text
src/
  lib.rs / telemetry.rs / maintenance.rs / gui.rs
  bin/cli.rs
  bin/gui.rs
wix/main.wxs                  MSI definition (cargo-wix)
.github/workflows/release.yml  Merge to main → semver bump → MSI → GitHub Release
```

---

## Install (end users)

1. Download the `.msi` from [GitHub Releases](https://github.com/jamesguan/TLUW-Telemetry-and-Logging-Utility-for-Windows/releases).
2. Run the installer (Administrator).
3. On the feature page you can enable:
   - **Desktop shortcut** (on by default)
   - **Add to PATH** (on by default)
   - **Run when Windows starts** (off by default) — Startup-folder shortcut to the GUI
   - **Re-apply after Windows Update** (off by default) — scheduled task on WU Event ID 19 + logon backup that runs `tluw disable`

You can also toggle startup / post-update later in the GUI (**Automation**) or via CLI:

```powershell
tluw startup on
tluw post-update on
tluw integration
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
target\release\tluw.exe
target\release\tluw-gui.exe
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
| Re-apply after Windows Update | Off | Task `TelemetryLoggingUtilityPostUpdate` |

**Post-update behavior:** listens for Microsoft Windows Update Client **Event ID 19** (successful install), waits 2 minutes, then runs `tluw disable --no-elevate`. A **logon** trigger is also registered as a backup when the event is missed (common after large feature updates).

---

## How to run

### GUI

```powershell
.\target\release\tluw-gui.exe
# or after install:
# & "${env:ProgramFiles}\Telemetry Logging Utility\bin\tluw-gui.exe"
```

### CLI

```powershell
.\target\release\tluw.exe status
.\target\release\tluw.exe disable
.\target\release\tluw.exe set diagtrack off
.\target\release\tluw.exe explain diagnostic-data
.\target\release\tluw.exe startup on
.\target\release\tluw.exe post-update on
.\target\release\tluw.exe integration

# Windows logs / tools (same buttons as GUI)
.\target\release\tluw.exe logs
.\target\release\tluw.exe open event-viewer
.\target\release\tluw.exe open privacy-feedback

# Clear logs (destructive; most need admin + --confirm)
.\target\release\tluw.exe clear-list
.\target\release\tluw.exe clear diagnosis --confirm
.\target\release\tluw.exe clear event-application --confirm
.\target\release\tluw.exe clear-all --confirm
```

---

## How to test

```powershell
cargo build --release
.\target\release\tluw.exe status
.\target\release\tluw.exe set diagtrack off
.\target\release\tluw.exe status

# GUI: Verify status → change toggles → Verify status again
.\target\release\tluw-gui.exe

# Installer locally
.\build-msi.ps1
# Run the MSI, confirm Desktop shortcut, optional features, then:
tluw status
tluw integration
schtasks /Query /TN TelemetryLoggingUtilityPostUpdate
```

---

## GitHub Releases (automatic)

Every **merge to `main`** runs `.github/workflows/release.yml`, which:

1. Bumps the semver in `Cargo.toml` from [Conventional Commits](https://www.conventionalcommits.org/) since the last `v*` tag  
   - `feat:` → **minor** · `fix:` / other → **patch** · `BREAKING CHANGE` / `!:` → **major**
2. Builds the MSI + `tluw.exe` / `tluw-gui.exe`
3. Commits `chore(release): vX.Y.Z`, tags it, and publishes a GitHub Release

```powershell
# Typical flow
git checkout -b feat/my-change
# … commit with a conventional message, e.g. "feat: …" or "fix: …"
git push -u origin HEAD
# Open a PR → merge to main → release is created by CI
```

Manual bump (Actions → **Release** → **Run workflow**): choose `patch` / `minor` / `major` / `auto`.

First release after this setup: if `Cargo.toml` is ahead of the latest `v*` tag (e.g. bumped to **1.0.0** while `v0.3.0` exists), CI ships that version as a catch-up. Later merges bump from the new tag.

Repo: https://github.com/jamesguan/TLUW-Telemetry-and-Logging-Utility-for-Windows

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

- **Renamed in v0.4.0** from “Windows Diagnostics”. First launch migrates prefs/disclaimer/history from `HKCU\Software\WindowsDiagnostics` and `%APPDATA%\WindowsDiagnostics`, then removes those legacy locations (only when they look like this app’s data). Binaries are now `tluw.exe` / `tluw-gui.exe`.
- Does not disable Windows Update or Defender.
- Post-update re-apply cannot catch every Microsoft reset; the logon backup helps after feature updates.
- Windows Home may ignore some policy keys.
- Reboot after a full lockdown is recommended.
- GUI **Dashboard** shows telemetry ON/OFF chips and daily GB freed by log/temp clears (`tluw history`).
- Optional **system tray** (Automation): close hides to tray; right-click for Open / Disable telemetry / Clear safe logs / Quit.

## Disclaimer / liability

**USE AT YOUR OWN RISK.** This software is provided **AS IS** with **NO WARRANTY**. The authors accept **NO LIABILITY** for data loss, system damage, compliance issues, or any other claim arising from its use.

Full waiver (also shown on first GUI launch): [DISCLAIMER.md](DISCLAIMER.md). CLI: `tluw disclaimer`.

This is not legal advice; consult an attorney if you need formal protection.

## License

[PolyForm Noncommercial License 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0) — see [LICENSE](LICENSE). The license’s **No Liability** section applies; [DISCLAIMER.md](DISCLAIMER.md) adds product-specific risk and indemnity language.

**Personal / private and other non-commercial use only.** Commercial use is not allowed.  
(This is source-available, not OSI “Open Source,” which requires allowing commercial use.)
