//! Disable Windows diagnostic data / telemetry.
//!
//! Run as Administrator (double-click or shortcut — will prompt to elevate).
//! Sets AllowTelemetry=0, stops DiagTrack, and disables common CEIP tasks.

#![cfg(windows)]

use std::io::{self, Write};
use std::process::{Command, Stdio};
use winreg::enums::*;
use winreg::RegKey;

fn main() {
    if !is_elevated() {
        println!("Administrator rights required — requesting elevation...");
        if let Err(e) = relaunch_elevated() {
            eprintln!("Failed to elevate: {e}");
            pause();
            std::process::exit(1);
        }
        return;
    }

    println!("windows-diagnostics — Windows diagnostic lockdown\n");

    let mut ok = true;
    ok &= set_telemetry_policy(0);
    ok &= disable_diagtrack();
    ok &= disable_scheduled_tasks();
    ok &= set_extra_privacy_keys();

    println!();
    if ok {
        println!("Done. Diagnostic data policy is Security-only (0).");
        println!("A reboot is recommended so all settings stick.");
    } else {
        println!("Finished with some errors — see messages above.");
        std::process::exit(1);
    }
    pause();
}

fn is_elevated() -> bool {
    Command::new("net")
        .arg("session")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn relaunch_elevated() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let exe_str = exe.to_string_lossy().replace('\'', "''");

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Start-Process -FilePath '{exe_str}' -Verb RunAs -Wait"),
        ])
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "UAC elevation was cancelled or failed",
        ))
    }
}

fn set_telemetry_policy(level: u32) -> bool {
    println!("[1/4] Setting AllowTelemetry = {level} (policy)...");

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let paths = [
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection",
    ];

    let mut all_ok = true;
    for path in paths {
        match hklm.create_subkey(path) {
            Ok((key, _)) => {
                if let Err(e) = key.set_value("AllowTelemetry", &level) {
                    eprintln!("  FAIL {path}\\AllowTelemetry: {e}");
                    all_ok = false;
                } else {
                    println!("  OK  {path}\\AllowTelemetry = {level}");
                }
                // Cap UI max as well when present on this path
                let _ = key.set_value("MaxTelemetryAllowed", &level);
            }
            Err(e) => {
                eprintln!("  FAIL create {path}: {e}");
                all_ok = false;
            }
        }
    }
    all_ok
}

fn disable_diagtrack() -> bool {
    println!("[2/4] Stopping and disabling Connected User Experiences (DiagTrack)...");

    let stop = Command::new("sc.exe")
        .args(["stop", "DiagTrack"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let config = Command::new("sc.exe")
        .args(["config", "DiagTrack", "start=", "disabled"])
        .output();

    match (&stop, &config) {
        (_, Ok(out)) if out.status.success() => {
            println!("  OK  DiagTrack set to disabled");
            true
        }
        (_, Ok(out)) => {
            let msg = String::from_utf8_lossy(&out.stderr);
            let msg2 = String::from_utf8_lossy(&out.stdout);
            eprintln!("  FAIL sc config DiagTrack: {}{}", msg.trim(), msg2.trim());
            false
        }
        (_, Err(e)) => {
            eprintln!("  FAIL sc.exe: {e}");
            false
        }
    }
}

fn disable_scheduled_tasks() -> bool {
    println!("[3/4] Disabling Customer Experience / feedback scheduled tasks...");

    // Full task paths for schtasks /Change
    let tasks = [
        r"\Microsoft\Windows\Customer Experience Improvement Program\Consolidator",
        r"\Microsoft\Windows\Customer Experience Improvement Program\UsbCeip",
        r"\Microsoft\Windows\Application Experience\Microsoft Compatibility Appraiser",
        r"\Microsoft\Windows\Application Experience\ProgramDataUpdater",
        r"\Microsoft\Windows\Application Experience\PcaPatchDbTask",
        r"\Microsoft\Windows\Application Experience\StartupAppTask",
        r"\Microsoft\Windows\Feedback\Siuf\DmClient",
        r"\Microsoft\Windows\Feedback\Siuf\DmClientOnScenarioDownload",
        r"\Microsoft\Windows\Maps\MapsToastTask",
        r"\Microsoft\Windows\Maps\MapsUpdateTask",
        r"\Microsoft\Windows\Shell\FamilySafetyMonitor",
        r"\Microsoft\Windows\Shell\FamilySafetyRefreshTask",
        r"\Microsoft\Windows\Windows Error Reporting\QueueReporting",
    ];

    for task in tasks {
        let output = Command::new("schtasks.exe")
            .args(["/Change", "/TN", task, "/Disable"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                println!("  OK  disabled {task}");
            }
            Ok(_) => {
                // Missing task on this build is fine
                println!("  --  skip (not found) {task}");
            }
            Err(e) => {
                eprintln!("  FAIL schtasks: {e}");
            }
        }
    }
    true
}

fn set_extra_privacy_keys() -> bool {
    println!("[4/4] Extra privacy policy keys...");

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut all_ok = true;

    // Advertising ID (current user)
    match hkcu.create_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo") {
        Ok((key, _)) => {
            if key.set_value("Enabled", &0u32).is_ok() {
                println!("  OK  AdvertisingInfo\\Enabled = 0");
            }
        }
        Err(e) => {
            eprintln!("  FAIL AdvertisingInfo: {e}");
            all_ok = false;
        }
    }

    // Tailored experiences
    match hkcu.create_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Privacy") {
        Ok((key, _)) => {
            if key
                .set_value("TailoredExperiencesWithDiagnosticDataEnabled", &0u32)
                .is_ok()
            {
                println!("  OK  TailoredExperiencesWithDiagnosticDataEnabled = 0");
            }
        }
        Err(e) => {
            eprintln!("  FAIL Privacy: {e}");
            all_ok = false;
        }
    }

    // Disable CEIP via policy
    match hklm.create_subkey(r"SOFTWARE\Policies\Microsoft\SQMClient\Windows") {
        Ok((key, _)) => {
            if key.set_value("CEIPEnable", &0u32).is_ok() {
                println!("  OK  SQMClient\\Windows\\CEIPEnable = 0");
            }
        }
        Err(e) => {
            eprintln!("  FAIL SQMClient: {e}");
            all_ok = false;
        }
    }

    // App telemetry / inventory
    match hklm.create_subkey(r"SOFTWARE\Policies\Microsoft\Windows\AppCompat") {
        Ok((key, _)) => {
            let _ = key.set_value("AITEnable", &0u32);
            let _ = key.set_value("DisableInventory", &1u32);
            println!("  OK  AppCompat AITEnable=0, DisableInventory=1");
        }
        Err(e) => {
            eprintln!("  FAIL AppCompat: {e}");
            all_ok = false;
        }
    }

    all_ok
}

fn pause() {
    print!("\nPress Enter to close...");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
}
