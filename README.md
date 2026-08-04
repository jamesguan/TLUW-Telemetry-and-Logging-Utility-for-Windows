# Windows Diagnostics

Modular Rust tools to inspect and toggle Windows diagnostic data / telemetry.

| Binary | Role |
|--------|------|
| `windows-diagnostics.exe` | **CLI** — scripts, terminals, one-shot commands |
| `windows-diagnostics-gui.exe` | **GUI** — same library, toggles + **Verify status** |

Both call `windows_diagnostics::telemetry`. The GUI does not reimplement policy logic.

```text
src/
  lib.rs              shared crate root
  telemetry.rs        read / apply settings (core)
  gui.rs              egui UI (feature = gui)
  bin/cli.rs          CLI entry
  bin/gui.rs          GUI entry
```

---

## How to build

Requires [Rust](https://rustup.rs/) on Windows 10/11.

```powershell
cd C:\Users\Garuda\Projects\windows-diagnostics
cargo build --release
# or
.\build.ps1
```

Outputs:

```text
target\release\windows-diagnostics.exe        # CLI
target\release\windows-diagnostics-gui.exe    # GUI
```

### Features

| Feature | Default | Provides |
|---------|---------|----------|
| `cli` | yes | `windows-diagnostics` + clap |
| `gui` | yes | `windows-diagnostics-gui` + egui |

CLI-only:

```powershell
cargo build --release --no-default-features --features cli
```

---

## How to run

### CLI

Open PowerShell in the project folder (or use the full path to the exe):

```powershell
cd C:\Users\Garuda\Projects\windows-diagnostics

# Show live ON/OFF + raw values (no admin needed for most reads)
.\target\release\windows-diagnostics.exe status

# Same as status (default when no subcommand)
.\target\release\windows-diagnostics.exe

# One-shot lockdown — all OFF (UAC prompt)
.\target\release\windows-diagnostics.exe disable

# Re-enable all (Basic diagnostic level where applicable)
.\target\release\windows-diagnostics.exe enable

# Toggle one field
.\target\release\windows-diagnostics.exe set diagtrack off
.\target\release\windows-diagnostics.exe set diagnostic-data on

# Help / docs
.\target\release\windows-diagnostics.exe --help
.\target\release\windows-diagnostics.exe list
.\target\release\windows-diagnostics.exe explain
.\target\release\windows-diagnostics.exe explain ceip-tasks
```

Mutating commands (`disable` / `enable` / `set`) request Administrator via UAC unless you pass `--no-elevate`.

### GUI

```powershell
.\target\release\windows-diagnostics-gui.exe
```

Or use the **Windows Diagnostics** desktop shortcut.

1. Accept the UAC prompt (needed to change settings; Verify still works read-only without it).
2. Click **Verify status** — re-reads every registry key, service, and task and shows a **Verified values** table (setting id, ON/OFF, live value, where it lives).
3. Each card also has a per-field **Verify** button and a **Value:** line with the current reading.
4. Use **ON/OFF** on a card, or **Turn all OFF / ON**, then click **Verify status** again to confirm the new values stuck.

---

## How to test

### 1. Smoke-test the CLI (no changes)

```powershell
cd C:\Users\Garuda\Projects\windows-diagnostics
cargo build --release
.\target\release\windows-diagnostics.exe list
.\target\release\windows-diagnostics.exe status
.\target\release\windows-diagnostics.exe explain diagnostic-data
```

You should see seven settings with `ON`/`OFF` and a detail string (e.g. `AllowTelemetry = 1`).

### 2. Change one setting and verify

```powershell
# Before
.\target\release\windows-diagnostics.exe status

# Turn DiagTrack off (UAC)
.\target\release\windows-diagnostics.exe set diagtrack off

# After — expect diagtrack STATE = OFF, note mentions Disabled
.\target\release\windows-diagnostics.exe status
```

Optional cross-check with Windows itself:

```powershell
sc.exe qc DiagTrack
# START_TYPE should be DISABLED (4) after "set diagtrack off"
```

### 3. Full lockdown + verify

```powershell
.\target\release\windows-diagnostics.exe disable
.\target\release\windows-diagnostics.exe status
```

Expect most rows `OFF`. Then:

```powershell
.\target\release\windows-diagnostics.exe enable
.\target\release\windows-diagnostics.exe status
```

### 4. Test the GUI verify flow

1. Run `.\target\release\windows-diagnostics-gui.exe` and allow UAC.
2. Click **Verify status** — the table should match `windows-diagnostics status`.
3. Click **Turn all OFF**, then **Verify status** again — rows should flip toward `OFF` with updated live values.
4. Click a card’s **Verify** — footer should show that field’s live value; the table updates.
5. Toggle one card ON/OFF, **Verify status** — that row’s value should match.

### 5. Independent registry / service checks (optional)

```powershell
# Diagnostic data policy
Get-ItemProperty -Path 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection' -Name AllowTelemetry -ErrorAction SilentlyContinue

# Advertising ID (current user)
Get-ItemProperty -Path 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo' -Name Enabled

# DiagTrack
Get-Service DiagTrack | Format-List Name, Status, StartType
```

These should agree with **Verify status** / `windows-diagnostics status`.

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

## Requirements

- Windows 10/11 (**Pro** recommended — Home may ignore some policy keys)
- Administrator for changes

## Notes

- Does not disable Windows Update or Defender.
- Feature updates may re-enable tasks — run `disable` or **Verify status** after big upgrades.
- A reboot is recommended after a full lockdown so service/policy state is consistent.

## Publish

```powershell
gh repo create windows-diagnostics --public --source=. --remote=origin --push
```

## License

MIT — see [LICENSE](LICENSE).
