//! Optional OS integration: run at logon, re-apply after Windows Update.

use std::process::Stdio;
use winreg::enums::*;
use winreg::RegKey;

use crate::identity::{self, CLI_BIN, GUI_BIN, RUN_VALUE, TASK_NAME};
use crate::win_cmd;

fn install_dir() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "cannot resolve install directory".into())
}

fn gui_exe_path() -> Result<std::path::PathBuf, String> {
    let dir = install_dir()?;
    let gui = dir.join(format!("{GUI_BIN}.exe"));
    if gui.exists() {
        Ok(gui)
    } else {
        // Side-by-side during upgrades: former binary name.
        let legacy = dir.join("windows-diagnostics-gui.exe");
        if legacy.exists() {
            Ok(legacy)
        } else {
            Ok(std::env::current_exe().map_err(|e| e.to_string())?)
        }
    }
}

pub(crate) fn cli_exe_path() -> Result<std::path::PathBuf, String> {
    let dir = install_dir()?;
    let cli = dir.join(format!("{CLI_BIN}.exe"));
    if cli.exists() {
        Ok(cli)
    } else {
        let legacy = dir.join("windows-diagnostics.exe");
        if legacy.exists() {
            Ok(legacy)
        } else {
            Ok(std::env::current_exe().map_err(|e| e.to_string())?)
        }
    }
}

pub(crate) fn task_exists(name: &str) -> bool {
    win_cmd::command("schtasks.exe")
        .args(["/Query", "/TN", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(crate) fn delete_task(name: &str) {
    let _ = win_cmd::command("schtasks.exe")
        .args(["/Delete", "/TN", name, "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Move legacy post-update task to the new name (idempotent).
pub fn migrate_legacy_post_update() {
    let legacy = identity::LEGACY_TASK_NAME;
    let had_legacy = task_exists(legacy);
    if had_legacy {
        delete_task(legacy);
    }
    if had_legacy && !task_exists(TASK_NAME) {
        let _ = set_post_update_task(true);
    }
}

/// HKCU Run key — launches the GUI at user logon.
pub fn is_run_at_startup_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .ok()
        .and_then(|k| k.get_value::<String, _>(RUN_VALUE).ok())
        .is_some()
}

pub fn set_run_at_startup(enabled: bool) -> Result<String, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;

    // Always drop the former Run value name.
    let _ = key.delete_value("WindowsDiagnostics");

    if enabled {
        let path = gui_exe_path()?;
        let value = format!("\"{}\"", path.display());
        key.set_value(RUN_VALUE, &value)
            .map_err(|e| e.to_string())?;
        Ok(format!("Run at startup enabled → {value}"))
    } else {
        let _ = key.delete_value(RUN_VALUE);
        Ok("Run at startup disabled".into())
    }
}

pub fn is_post_update_enabled() -> bool {
    task_exists(TASK_NAME)
}

/// After a successful Windows Update (Event ID 19) and as a logon backup,
/// re-run `tluw disable`.
pub fn set_post_update_task(enabled: bool) -> Result<String, String> {
    // Clean former task name whenever we touch integration.
    delete_task(identity::LEGACY_TASK_NAME);

    if !enabled {
        delete_task(TASK_NAME);
        return Ok("Post-update task removed".into());
    }

    let cli = cli_exe_path()?;
    let cli_str = cli.to_string_lossy().replace('&', "&amp;");

    // Build task XML without format!-brace conflicts.
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-16"?>"#);
    xml.push('\n');
    xml.push_str(
        r#"<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Re-apply Telemetry and Logging Utility for Windows lockdown after Windows Update and at logon.</Description>
  </RegistrationInfo>
  <Triggers>
    <EventTrigger>
      <Enabled>true</Enabled>
      <Subscription>&lt;QueryList&gt;&lt;Query Id="0" Path="Microsoft-Windows-WindowsUpdateClient/Operational"&gt;&lt;Select Path="Microsoft-Windows-WindowsUpdateClient/Operational"&gt;*[System[Provider[@Name='Microsoft-Windows-WindowsUpdateClient'] and (EventID=19)]]&lt;/Select&gt;&lt;/Query&gt;&lt;/QueryList&gt;</Subscription>
      <Delay>PT2M</Delay>
    </EventTrigger>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <Delay>PT1M</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT10M</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
"#,
    );
    xml.push_str("      <Command>");
    xml.push_str(&cli_str);
    xml.push_str("</Command>\n");
    xml.push_str("      <Arguments>disable --no-elevate</Arguments>\n");
    xml.push_str(
        r#"    </Exec>
  </Actions>
</Task>
"#,
    );

    let tmp = std::env::temp_dir().join("tluw-postupdate.xml");
    // UTF-16 LE with BOM — schtasks /XML expects this on many Windows builds.
    let mut utf16: Vec<u8> = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(&tmp, &utf16).map_err(|e| e.to_string())?;

    delete_task(TASK_NAME);

    let out = win_cmd::command("schtasks.exe")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/XML",
            tmp.to_str().unwrap_or_default(),
            "/F",
        ])
        .output()
        .map_err(|e| e.to_string())?;

    let _ = std::fs::remove_file(&tmp);

    if out.status.success() {
        Ok(format!(
            "Post-update task '{TASK_NAME}' created (WU Event 19 + logon backup)"
        ))
    } else {
        let msg = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        );
        create_simple_post_update_fallback(cli.to_str().unwrap_or_default()).map_err(|e| {
            format!("event-based task failed ({msg}); fallback also failed: {e}")
        })?;
        Ok("Post-update fallback task created (ONLOGON) — event XML unavailable on this PC".into())
    }
}

fn create_simple_post_update_fallback(cli: &str) -> Result<(), String> {
    delete_task(TASK_NAME);

    let tr = format!("\"{cli}\" disable --no-elevate");
    let out = win_cmd::command("schtasks.exe")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &tr,
            "/SC",
            "ONLOGON",
            "/RL",
            "HIGHEST",
            "/F",
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        ))
    }
}

pub struct IntegrationState {
    pub run_at_startup: bool,
    pub post_update: bool,
}

pub fn read_integration() -> IntegrationState {
    IntegrationState {
        run_at_startup: is_run_at_startup_enabled(),
        post_update: is_post_update_enabled(),
    }
}
