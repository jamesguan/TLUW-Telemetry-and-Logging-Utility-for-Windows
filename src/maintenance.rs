//! Optional OS integration: run at logon, re-apply after Windows Update.

use std::process::{Command, Stdio};
use winreg::enums::*;
use winreg::RegKey;

const RUN_VALUE: &str = "WindowsDiagnostics";
pub const TASK_NAME: &str = "WindowsDiagnosticsPostUpdate";

fn install_dir() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "cannot resolve install directory".into())
}

fn gui_exe_path() -> Result<std::path::PathBuf, String> {
    let dir = install_dir()?;
    let gui = dir.join("windows-diagnostics-gui.exe");
    if gui.exists() {
        Ok(gui)
    } else {
        Ok(std::env::current_exe().map_err(|e| e.to_string())?)
    }
}

fn cli_exe_path() -> Result<std::path::PathBuf, String> {
    let dir = install_dir()?;
    let cli = dir.join("windows-diagnostics.exe");
    if cli.exists() {
        Ok(cli)
    } else {
        Ok(std::env::current_exe().map_err(|e| e.to_string())?)
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
    Command::new("schtasks.exe")
        .args(["/Query", "/TN", TASK_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// After a successful Windows Update (Event ID 19) and as a logon backup,
/// re-run `windows-diagnostics disable`.
pub fn set_post_update_task(enabled: bool) -> Result<String, String> {
    if !enabled {
        let _ = Command::new("schtasks.exe")
            .args(["/Delete", "/TN", TASK_NAME, "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
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
    <Description>Re-apply Windows Diagnostics lockdown after Windows Update and at logon.</Description>
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

    let tmp = std::env::temp_dir().join("windows-diagnostics-postupdate.xml");
    // UTF-16 LE with BOM — schtasks /XML expects this on many Windows builds.
    let mut utf16: Vec<u8> = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(&tmp, &utf16).map_err(|e| e.to_string())?;

    let _ = Command::new("schtasks.exe")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let out = Command::new("schtasks.exe")
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
    let _ = Command::new("schtasks.exe")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let tr = format!("\"{cli}\" disable --no-elevate");
    let out = Command::new("schtasks.exe")
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
