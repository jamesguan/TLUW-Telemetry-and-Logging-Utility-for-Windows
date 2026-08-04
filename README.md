# Windows Diagnostics

A small Rust CLI for **Windows 11 Pro** that turns off diagnostic data collection, disables the Connected User Experiences and Telemetry service (`DiagTrack`), and shuts down common Customer Experience Improvement Program (CEIP) / feedback scheduled tasks.

Double-click the exe (or a desktop shortcut). It will prompt for Administrator via UAC.

## Features

- Sets diagnostic data policy to **Security-only** (`AllowTelemetry = 0`)
- Stops and **disables** the `DiagTrack` service
- Disables CEIP, Compatibility Appraiser, Feedback, Maps, and related scheduled tasks (skips any missing on your build)
- Turns off advertising ID, tailored experiences, CEIP, and app inventory policies

## Requirements

- Windows 10/11 (**Pro** recommended — Home may ignore some policy keys)
- Rust 1.70+ to build from source ([rustup](https://rustup.rs/))
- Administrator rights (requested automatically)

## Build

```powershell
git clone https://github.com/YOUR_USER/windows-diagnostics.git
cd windows-diagnostics
cargo build --release
```

Binary output:

```text
target\release\windows-diagnostics.exe
```

## Usage

```powershell
.\target\release\windows-diagnostics.exe
```

Or create a desktop shortcut:

```powershell
$exe = Resolve-Path .\target\release\windows-diagnostics.exe
$desktop = [Environment]::GetFolderPath("Desktop")
$w = New-Object -ComObject WScript.Shell
$s = $w.CreateShortcut("$desktop\Windows Diagnostics.lnk")
$s.TargetPath = $exe
$s.WorkingDirectory = Split-Path $exe
$s.Description = "Disable Windows diagnostic data"
$s.Save()
```

After a major Windows feature update, run it again — Microsoft sometimes re-enables tasks or services.

A reboot is recommended after the first successful run.

## What it changes

| Area | Action |
|------|--------|
| `HKLM\...\Policies\...\DataCollection` | `AllowTelemetry` / `MaxTelemetryAllowed` → `0` |
| Service `DiagTrack` | Stop + startup type Disabled |
| CEIP / App Experience / Feedback tasks | Disabled via `schtasks` |
| Advertising ID, tailored experiences | Set to off |
| SQM CEIP / AppCompat inventory | Policy disabled |

It does **not** disable Windows Update, Microsoft Defender, or BITS.

## Limitations

- You cannot fully remove all Microsoft telemetry from Windows; this minimizes the supported surface.
- Blocking Microsoft domains in `hosts` or deleting system binaries is out of scope and can break updates.
- On Windows Home, Group Policy-equivalent registry keys may have limited effect.

## Publish to GitHub

From this folder (install [GitHub CLI](https://cli.github.com/) if you want `gh`):

```powershell
cd C:\Users\Garuda\Projects\windows-diagnostics

# If you use GitHub CLI:
gh repo create windows-diagnostics --public --source=. --remote=origin --push

# Or create an empty repo on github.com, then:
git remote add origin https://github.com/YOUR_USER/windows-diagnostics.git
git push -u origin main
```

Replace `YOUR_USER` with your GitHub username. Update the clone URL in this README to match.

## License

MIT — see [LICENSE](LICENSE).
